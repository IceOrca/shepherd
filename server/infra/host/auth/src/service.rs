use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD};
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
    #[error("failed to configure account-provisioning fingerprinting: {0}")]
    ProvisioningFingerprintKey(String),
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
    pub(crate) provisioning_fingerprint_key: Arc<[u8]>,
}

impl AuthService {
    pub async fn new(
        db: Arc<infra_postgres::DatabaseAdapter>,
        redis: Arc<RedisAdapter>,
        auth_admin: Arc<dyn ExtAuthAdmin>,
    ) -> Result<Arc<Self>, AuthSvcErr> {
        let encoded_key: String = std::env::var("AUTH_PROVISIONING_FINGERPRINT_KEY_BASE64")
            .ok()
            .map(|value: String| value.trim().to_owned())
            .filter(|value: &String| !value.is_empty())
            .ok_or_else(|| {
                AuthSvcErr::ProvisioningFingerprintKey(
                    "AUTH_PROVISIONING_FINGERPRINT_KEY_BASE64 is required".to_owned(),
                )
            })?;
        let key: Vec<u8> = STANDARD.decode(encoded_key).map_err(|_| {
            AuthSvcErr::ProvisioningFingerprintKey(
                "AUTH_PROVISIONING_FINGERPRINT_KEY_BASE64 must be valid standard base64".to_owned(),
            )
        })?;
        if key.len() < 32 {
            return Err(AuthSvcErr::ProvisioningFingerprintKey(
                "AUTH_PROVISIONING_FINGERPRINT_KEY_BASE64 must decode to at least 32 bytes".to_owned(),
            ));
        }
        Ok(Arc::new(Self {
            db,
            acct_cache: AuthedUserCache::from_env(redis)?,
            token_verifier: OidcJwksVerifier::from_env().await?,
            auth_admin,
            provisioning_fingerprint_key: Arc::from(key),
        }))
    }
}
