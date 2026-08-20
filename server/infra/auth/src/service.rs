use std::sync::Arc;

use infra_redis::RedisAdapter;

use crate::ext_foundation::{
    AccessTokenError, AuthenticatedUserCacheConfigError, ExtProvider,
    account_cache::AuthenticatedUserCache,
    auth_admin::{AuthAdminConfigError, AuthAdminService},
};

#[derive(Debug, thiserror::Error)]
pub enum AuthServiceError {
    #[error("failed to configure access-token validation: {0}")]
    AccessToken(#[from] AccessTokenError),
    #[error("failed to configure identity administration: {0}")]
    Administration(#[from] AuthAdminConfigError),
    #[error("failed to configure authenticated-user cache: {0}")]
    AccountCache(#[from] AuthenticatedUserCacheConfigError),
}

/// Authentication capability exposed by the HTTP host.
///
/// The external provider owns credentials and sessions. This service only
/// exposes verified bearer-token identities to the host and applications.
pub struct AuthService {
    pub db: Arc<infra_postgres::DatabaseAdapter>,
    pub(crate) account_cache: Arc<AuthenticatedUserCache>,
    pub provider: Arc<ExtProvider>,
    pub admin: Arc<AuthAdminService>,
}

impl AuthService {
    pub async fn new(
        db: Arc<infra_postgres::DatabaseAdapter>,
        redis: Arc<RedisAdapter>,
    ) -> Result<Arc<Self>, AuthServiceError> {
        Ok(Arc::new(Self {
            db,
            account_cache: AuthenticatedUserCache::from_env(redis)?,
            provider: ExtProvider::from_env().await?,
            admin: AuthAdminService::from_env()?,
        }))
    }
}
