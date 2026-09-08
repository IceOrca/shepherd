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
use infra_kernel::debug::Debugging;
use serde_json::json;
use super::{bootstrap, database, core::*};

// Tenant onboarding is rare; serialize it before acquiring pooled connections.
static BOOTSTRAP_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static LOGGING_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

type ApiError = (StatusCode, Json<serde_json::Value>);
fn failure(status: StatusCode, message: &str) -> ApiError {
    (
        status,
        Json(json!({"message": message, "code": "platform_request_failed"})),
    )
}

async fn profile(auth: &AuthService, principal: &AuthedPrincipal) -> Result<Option<PlatformProfile>, ApiError> {
    database::administrator(auth.db.global_pool(), &principal.issuer, &principal.subject)
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
    let admin: Router<Arc<AuthService>> = Router::new()
        .route("/platform/tenants", post(create_tenant))
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
    State(auth): State<Arc<AuthService>>,
    Extension(principal): Extension<AuthedPrincipal>,
    Extension(admin): Extension<PlatformProfile>,
    Json(mut request): Json<TenantBootstrapRequest>,
) -> Result<Json<TenantBootstrapResult>, ApiError> {
    request
        .normalize()
        .map_err(|message: String| failure(StatusCode::UNPROCESSABLE_ENTITY, &message))?;
    let _permit: tokio::sync::MutexGuard<'_, ()> = BOOTSTRAP_GATE.try_lock().map_err(|_| {
        failure(
            StatusCode::CONFLICT,
            "Một doanh nghiệp khác đang được khởi tạo. Vui lòng thử lại.",
        )
    })?;
    let guard: sqlx::Transaction<'static, sqlx::Postgres> =
        database::lock_administrator(auth.db.global_pool(), &principal.issuer, &principal.subject)
            .await
            .map_err(|error: sqlx::Error| {
                if matches!(error, sqlx::Error::RowNotFound) {
                    failure(StatusCode::FORBIDDEN, "Quyền quản trị đã bị thu hồi.")
                } else {
                    failure(StatusCode::SERVICE_UNAVAILABLE, "Không thể kiểm tra quyền quản trị.")
                }
            })?;
    tracing::info!(tenant_id = %request.tenant_id, actor = %principal.subject, "Platform tenant bootstrap requested");
    let result: TenantBootstrapResult = bootstrap::bootstrap(auth.db.global_pool(), auth.auth_admin.as_ref(), &principal.issuer, &request, &principal.subject, &admin.email)
        .await.map_err(|error| {
            tracing::warn!(tenant_id = %request.tenant_id, reason = %error, "Platform tenant bootstrap rejected");
            failure(StatusCode::CONFLICT, "Không thể tạo doanh nghiệp. Kiểm tra mã doanh nghiệp và email chủ sở hữu; giữ nguyên yêu cầu để thử lại nếu dịch vụ bị gián đoạn.")
        })?;
    guard.commit().await.map_err(|_| {
        failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "Hãy gửi lại cùng yêu cầu để kiểm tra kết quả.",
        )
    })?;
    Ok(Json(result))
}

async fn log_filter() -> Result<Json<ServerLogFilter>, ApiError> {
    Ok(Json(ServerLogFilter {
        filter: Debugging::filter()
            .map_err(|_| failure(StatusCode::SERVICE_UNAVAILABLE, "Không thể đọc cấu hình ghi log."))?,
    }))
}

async fn set_log_level(
    State(auth): State<Arc<AuthService>>,
    Extension(principal): Extension<AuthedPrincipal>,
    Json(request): Json<ServerLogLevelRequest>,
) -> Result<Json<ServerLogFilter>, ApiError> {
    let _permit: tokio::sync::MutexGuard<'_, ()> = LOGGING_GATE.lock().await;
    let guard: sqlx::Transaction<'static, sqlx::Postgres> =
        database::lock_administrator(auth.db.global_pool(), &principal.issuer, &principal.subject)
            .await
            .map_err(|_| failure(StatusCode::FORBIDDEN, "Không thể xác nhận quyền quản trị."))?;
    let before: String =
        Debugging::filter().map_err(|_| failure(StatusCode::SERVICE_UNAVAILABLE, "Không thể đọc cấu hình ghi log."))?;
    let result: Result<(), sqlx::Error> = database::audit_log_request(
        auth.db.global_pool(),
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
    let filter: String = Debugging::set_level(request.level.as_str())
        .map_err(|_| failure(StatusCode::SERVICE_UNAVAILABLE, "Không thể cập nhật mức log."))?;
    tracing::warn!(actor = %principal.subject, level = request.level.as_str(), "Server log level changed without restart");
    guard
        .commit()
        .await
        .map_err(|_| failure(StatusCode::SERVICE_UNAVAILABLE, "Không thể hoàn tất cập nhật log."))?;
    Ok(Json(ServerLogFilter { filter }))
}
