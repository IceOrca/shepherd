use std::sync::Arc;

use infra_redis::RedisAdapter;

use crate::ext_service::{
    AccessTokenError, AuthenticatedUserCacheConfigError, OidcJwksVerifier, account_cache::AuthenticatedUserCache,
    auth_admin::ExternalIdentityAdmin,
};

#[derive(Debug, thiserror::Error)]
pub enum AuthServiceError {
    #[error("failed to configure access-token validation: {0}")]
    AccessToken(#[from] AccessTokenError),
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
    pub token_verifier: Arc<OidcJwksVerifier>,
    pub identity_admin: Arc<dyn ExternalIdentityAdmin>,
}

impl AuthService {
    pub async fn new(
        db: Arc<infra_postgres::DatabaseAdapter>,
        redis: Arc<RedisAdapter>,
        identity_admin: Arc<dyn ExternalIdentityAdmin>,
    ) -> Result<Arc<Self>, AuthServiceError> {
        Ok(Arc::new(Self {
            db,
            account_cache: AuthenticatedUserCache::from_env(redis)?,
            token_verifier: OidcJwksVerifier::from_env().await?,
            identity_admin,
        }))
    }
}
