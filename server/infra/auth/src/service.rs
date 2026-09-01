use std::sync::Arc;

use infra_redis::RedisAdapter;

use crate::ext_service::{
    AccessTokenErr, AuthedCacheCfgErr, OidcJwksVerifier, account_cache::AuthedUserCache, auth_admin::ExtAuthAdmin,
};

#[derive(Debug, thiserror::Error)]
pub enum AuthSvcErr {
    #[error("failed to configure access-token validation: {0}")]
    AccessToken(#[from] AccessTokenErr),
    #[error("failed to configure authenticated-user cache: {0}")]
    AccountCache(#[from] AuthedCacheCfgErr),
}

/// Authentication capability exposed by the HTTP host.
///
/// The external provider owns credentials and sessions. This service only
/// exposes verified bearer-token identities to the host and application.
pub struct AuthService {
    pub db: Arc<infra_postgres::DatabaseAdapter>,
    pub(crate) acct_cache: Arc<AuthedUserCache>,
    pub token_verifier: Arc<OidcJwksVerifier>,
    pub auth_admin: Arc<dyn ExtAuthAdmin>,
}

impl AuthService {
    pub async fn new(
        db: Arc<infra_postgres::DatabaseAdapter>,
        redis: Arc<RedisAdapter>,
        auth_admin: Arc<dyn ExtAuthAdmin>,
    ) -> Result<Arc<Self>, AuthSvcErr> {
        Ok(Arc::new(Self {
            db,
            acct_cache: AuthedUserCache::from_env(redis)?,
            token_verifier: OidcJwksVerifier::from_env().await?,
            auth_admin,
        }))
    }
}
