use std::sync::Arc;

use async_trait::async_trait;
use axum::http::StatusCode;
use infra_auth::ext_service::{
    AuthedPrincipal,
    auth_admin::{CreateExternalIdentityRequest, ExtAdminErr, ExtAuthAdmin, ExternalIdentity},
};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use tokio::sync::MutexGuard;
use uuid::Uuid;

use super::{TenantBootstrapEndpoint, TenantBootstrapMutationCtx};
use crate::platform::core::{PlatformProfile, TenantBootstrapRequest};

struct UnusedProvider;

#[async_trait]
impl ExtAuthAdmin for UnusedProvider {
    async fn get_identity(&self, _: &str) -> Result<Option<ExternalIdentity>, ExtAdminErr> {
        panic!("admission test must not call the provider")
    }

    async fn find_identity_by_email(&self, _: &str) -> Result<Option<ExternalIdentity>, ExtAdminErr> {
        panic!("admission test must not call the provider")
    }

    async fn find_provisioned_identity(
        &self,
        _: &str,
        _: Uuid,
        _: Uuid,
    ) -> Result<Option<ExternalIdentity>, ExtAdminErr> {
        panic!("admission test must not call the provider")
    }

    async fn create_identity(&self, _: &CreateExternalIdentityRequest) -> Result<ExternalIdentity, ExtAdminErr> {
        panic!("admission test must not call the provider")
    }
}

#[tokio::test]
async fn bootstrap_admission_precedes_database_access_and_releases_after_failure() {
    // A closed lazy pool guarantees this test never connects or mutates data.
    let pool: PgPool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
    pool.close().await;
    let endpoint: TenantBootstrapEndpoint = TenantBootstrapEndpoint::new(pool, Arc::new(UnusedProvider));
    let principal: AuthedPrincipal = AuthedPrincipal {
        issuer: "https://identity.example".to_owned(),
        subject: "test-operator".to_owned(),
        audience: vec!["authenticated".to_owned()],
        username: None,
        email: None,
        email_verified: false,
        scopes: Vec::new(),
        session_id: None,
        token_id: None,
        issued_at: None,
        expires_at: u64::MAX,
        tenant_id: None,
    };
    let admin: PlatformProfile = PlatformProfile {
        username: "test-operator".to_owned(),
        email: "operator@example.test".to_owned(),
    };
    let request: TenantBootstrapRequest = TenantBootstrapRequest {
        tenant_id: Uuid::new_v4(),
        tenant_slug: "test-company".to_owned(),
        tenant_display_name: "Test company".to_owned(),
        idempotency_key: Uuid::new_v4(),
        owners: Vec::new(),
    };
    let guard: MutexGuard<'_, TenantBootstrapMutationCtx> = endpoint.mutation.lock().await;
    let (status, _) = endpoint
        .create(&principal, &admin, &request)
        .await
        .err()
        .expect("busy admission");
    assert_eq!(status, StatusCode::CONFLICT);
    drop(guard);
    let (status, _) = endpoint
        .create(&principal, &admin, &request)
        .await
        .err()
        .expect("closed test pool");
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(endpoint.mutation.try_lock().is_ok(), "early errors must drop the guard");
}
