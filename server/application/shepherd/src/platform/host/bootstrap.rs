//! Web bootstrap admission owns the resources used in its critical section.
use std::sync::Arc;

use axum::http::StatusCode;
use infra_auth::ext_service::{AuthedPrincipal, auth_admin::ExtAuthAdmin};
use sqlx::PgPool;
use tokio::sync::{Mutex, MutexGuard};

use super::{ApiError, failure};
use crate::platform::{
    bootstrap, database,
    core::{PlatformProfile, TenantBootstrapRequest, TenantBootstrapResult},
};

pub(super) struct TenantBootstrapEndpoint {
    mutation: Mutex<TenantBootstrapMutationCtx>,
}

struct TenantBootstrapMutationCtx {
    pool: PgPool,
    auth_admin: Arc<dyn ExtAuthAdmin>,
}

impl TenantBootstrapEndpoint {
    pub(super) fn new(pool: PgPool, auth_admin: Arc<dyn ExtAuthAdmin>) -> Self {
        Self {
            mutation: Mutex::new(TenantBootstrapMutationCtx { pool, auth_admin }),
        }
    }

    pub(super) async fn create(
        &self,
        principal: &AuthedPrincipal,
        admin: &PlatformProfile,
        request: &TenantBootstrapRequest,
    ) -> Result<TenantBootstrapResult, ApiError> {
        // Reject concurrent onboarding before acquiring any pooled connections.
        let mut mutation: MutexGuard<'_, TenantBootstrapMutationCtx> =
            self.mutation
                .try_lock()
                .map_err(|_: tokio::sync::TryLockError| -> ApiError {
                    failure(
                        StatusCode::CONFLICT,
                        "Một doanh nghiệp khác đang được khởi tạo. Vui lòng thử lại.",
                    )
                })?;
        mutation.create(principal, admin, request).await
    }
}

impl TenantBootstrapMutationCtx {
    async fn create(
        &mut self,
        principal: &AuthedPrincipal,
        admin: &PlatformProfile,
        request: &TenantBootstrapRequest,
    ) -> Result<TenantBootstrapResult, ApiError> {
        let guard: sqlx::Transaction<'static, sqlx::Postgres> =
            database::lock_administrator(&self.pool, &principal.issuer, &principal.subject)
                .await
                .map_err(|error: sqlx::Error| -> ApiError {
                    if matches!(error, sqlx::Error::RowNotFound) {
                        failure(StatusCode::FORBIDDEN, "Quyền quản trị đã bị thu hồi.")
                    } else {
                        failure(StatusCode::SERVICE_UNAVAILABLE, "Không thể kiểm tra quyền quản trị.")
                    }
                })?;
        tracing::info!(tenant_id = %request.tenant_id, actor = %principal.subject, "Platform tenant bootstrap requested");
        let result: TenantBootstrapResult = bootstrap::bootstrap(
            &self.pool,
            self.auth_admin.as_ref(),
            &principal.issuer,
            request,
            &principal.subject,
            &admin.email,
        )
        .await
        .map_err(|error: Box<dyn std::error::Error + Send + Sync>| -> ApiError {
            tracing::warn!(tenant_id = %request.tenant_id, reason = %error, "Platform tenant bootstrap rejected");
            failure(StatusCode::CONFLICT, "Không thể tạo doanh nghiệp. Kiểm tra mã doanh nghiệp và email chủ sở hữu; giữ nguyên yêu cầu để thử lại nếu dịch vụ bị gián đoạn.")
        })?;
        guard.commit().await.map_err(|_: sqlx::Error| -> ApiError {
            failure(
                StatusCode::SERVICE_UNAVAILABLE,
                "Hãy gửi lại cùng yêu cầu để kiểm tra kết quả.",
            )
        })?;
        Ok(result)
    }
}

#[cfg(test)]
#[path = "bootstrap/tests.rs"]
mod tests;
