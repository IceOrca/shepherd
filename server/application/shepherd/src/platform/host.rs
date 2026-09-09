use std::sync::Arc;
use axum::{
    Extension, Json, Router,
    extract::{State, Request},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use infra_auth::{AuthService, ext_service::AuthedPrincipal};
use infra_kernel::debug::{Debugging, LogMutationCtx};
use serde_json::json;
use super::{database, core::*};
use tracing::{error, warn, info, debug, trace};

mod bootstrap;
use bootstrap::TenantBootstrapEndpoint;

type ApiError = (StatusCode, Json<serde_json::Value>);
fn failure(s: StatusCode, msg: &str) -> ApiError {
    (s, Json(json!({"message": msg, "code": "platform_request_failed"})))
}

async fn profile(auth: &AuthService, principal: &AuthedPrincipal) -> Result<Option<PlatformProfile>, ApiError> {
    database::administrator(auth.db.pool(), &principal.issuer, &principal.subject)
        .await
        .map_err(|error: sqlx::Error| {
            tracing::error!(operation = "platform.authorize", reason = %error, "Platform authority lookup failed");
            failure(
                StatusCode::SERVICE_UNAVAILABLE,
                "Không thể kiểm tra quyền quản trị hệ thống.",
            )
        })
}

pub fn routes(auth: Arc<AuthService>) -> Router {
    let bootstrap: Arc<TenantBootstrapEndpoint> = Arc::new(TenantBootstrapEndpoint::new(
        auth.db.pool().clone(),
        Arc::clone(&auth.auth_admin),
    ));
    let admin: Router<Arc<AuthService>> = Router::new()
        .route("/platform/tenants", post(create_tenant).layer(Extension(bootstrap)))
        .route("/platform/log-level", get(log_filter).put(set_log_level))
        .layer(crate::ratelimiting::protected_route_layer(crate::ratelimit::policy(
            crate::ratelimit::AppRouteGroup::Administration,
        )))
        .route_layer(middleware::from_fn_with_state(
            Arc::clone(&auth),
            require_platform_admin,
        ));
    Router::new()
        .route("/platform/session", get(session))
        .merge(admin)
        .with_state(auth)
}

async fn require_platform_admin(State(auth): State<Arc<AuthService>>, request: Request, next: Next) -> Response {
    let Some(principal) = request.extensions().get::<AuthedPrincipal>() else {
        return failure(StatusCode::UNAUTHORIZED, "Vui lòng đăng nhập.").into_response();
    };
    match profile(&auth, principal).await {
        Ok(Some(administrator)) => {
            let mut request: Request = request;
            request.extensions_mut().insert(administrator);
            next.run(request).await
        }
        Ok(None) => failure(
            StatusCode::FORBIDDEN,
            "Chỉ quản trị viên hệ thống được thực hiện thao tác này.",
        )
        .into_response(),
        Err(error) => error.into_response(),
    }
}

async fn session(
    State(auth): State<Arc<AuthService>>,
    Extension(principal): Extension<AuthedPrincipal>,
) -> Result<Json<PlatformSession>, ApiError> {
    Ok(Json(PlatformSession {
        administrator: profile(&auth, &principal).await?,
    }))
}

async fn create_tenant(
    Extension(bootstrap): Extension<Arc<TenantBootstrapEndpoint>>,
    Extension(principal): Extension<AuthedPrincipal>,
    Extension(admin): Extension<PlatformProfile>,
    Json(mut request): Json<TenantBootstrapRequest>,
) -> Result<Json<TenantBootstrapResult>, ApiError> {
    request
        .normalize()
        .map_err(|message: String| failure(StatusCode::UNPROCESSABLE_ENTITY, &message))?;
    bootstrap.create(&principal, &admin, &request).await.map(Json)
}

async fn log_filter() -> Result<Json<ServerLogFilter>, ApiError> {
    let context: tokio::sync::RwLockReadGuard<'_, LogMutationCtx> =
        Debugging::read().await.map_err(|_: String| -> ApiError {
            failure(StatusCode::SERVICE_UNAVAILABLE, "Không thể đọc cấu hình ghi log.")
        })?;
    Ok(Json(ServerLogFilter {
        filter: context.filter().to_owned(),
    }))
}

async fn set_log_level(
    State(auth): State<Arc<AuthService>>,
    Extension(principal): Extension<AuthedPrincipal>,
    Json(request): Json<ServerLogLevelRequest>,
) -> Result<Json<ServerLogFilter>, ApiError> {
    let mut context: tokio::sync::RwLockWriteGuard<'_, LogMutationCtx> =
        Debugging::write().await.map_err(|_: String| -> ApiError {
            failure(StatusCode::SERVICE_UNAVAILABLE, "Không thể đọc cấu hình ghi log.")
        })?;
    let guard: sqlx::Transaction<'static, sqlx::Postgres> =
        database::lock_administrator(auth.db.pool(), &principal.issuer, &principal.subject)
            .await
            .map_err(|_| failure(StatusCode::FORBIDDEN, "Không thể xác nhận quyền quản trị."))?;
    let before: String = context.filter().to_owned();
    let result: Result<(), sqlx::Error> = database::audit_log_request(
        auth.db.pool(),
        &principal.issuer,
        &principal.subject,
        &before,
        request.level.as_str(),
    )
    .await;
    result.map_err(|error: sqlx::Error| {
        tracing::error!(reason = %error, "Platform logging audit failed");
        failure(StatusCode::SERVICE_UNAVAILABLE, "Không thể lưu lịch sử thay đổi log.")
    })?;
    let filter: String = context
        .set_level(request.level.as_str())
        .map_err(|_err: String| failure(StatusCode::SERVICE_UNAVAILABLE, "Không thể cập nhật mức log."))?;
    tracing::warn!(actor = %principal.subject, level = request.level.as_str(), "Server log level changed without restart");
    guard
        .commit()
        .await
        .map_err(|_err: sqlx::Error| failure(StatusCode::SERVICE_UNAVAILABLE, "Không thể hoàn tất cập nhật log."))?;
    Ok(Json(ServerLogFilter { filter }))
}
