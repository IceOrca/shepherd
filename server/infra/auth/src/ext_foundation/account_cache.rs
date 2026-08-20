use std::{sync::Arc, time::Duration};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use infra_redis::RedisAdapter;
use redis::{AsyncCommands, RedisError};
use sha2::{Digest, Sha256};
use tracing::{debug, error, info, trace, warn};

use super::{AuthenticatedPrincipal, account::AuthenticatedUser};

const CACHE_KEY_PREFIX: &str = "auth:application-user:v1";
const DEFAULT_CACHE_TTL_SECS: u64 = 60;
const MAX_CACHE_TTL_SECS: u64 = 3_600;

#[derive(Debug, thiserror::Error)]
pub enum AuthenticatedUserCacheConfigError {
    #[error("AUTH_ACCOUNT_CACHE_TTL_SECS must be an integer between 1 and {MAX_CACHE_TTL_SECS}")]
    InvalidTtl,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum AuthenticatedUserCacheError {
    #[error("authenticated-user Redis operation failed")]
    Redis(#[from] RedisError),
    #[error("authenticated-user cache serialization failed")]
    Serialization(#[from] serde_json::Error),
}

pub(crate) struct AuthenticatedUserCache {
    redis: Arc<RedisAdapter>,
    ttl: Duration,
}

impl AuthenticatedUserCache {
    pub(crate) fn from_env(redis: Arc<RedisAdapter>) -> Result<Arc<Self>, AuthenticatedUserCacheConfigError> {
        let ttl_secs: u64 =
            std::env::var("AUTH_ACCOUNT_CACHE_TTL_SECS").map_or(Ok(DEFAULT_CACHE_TTL_SECS), |raw_value: String| {
                raw_value
                    .parse::<u64>()
                    .ok()
                    .filter(|value: &u64| (1..=MAX_CACHE_TTL_SECS).contains(value))
                    .ok_or(AuthenticatedUserCacheConfigError::InvalidTtl)
            })?;
        info!(
            operation = "configure_authenticated_user_cache",
            ttl_secs,
            key_prefix = CACHE_KEY_PREFIX,
            "Configured bounded Redis authenticated-user cache"
        );
        Ok(Arc::new(Self {
            redis,
            ttl: Duration::from_secs(ttl_secs),
        }))
    }

    pub(crate) async fn get(
        &self,
        principal: &AuthenticatedPrincipal,
    ) -> Result<Option<AuthenticatedUser>, AuthenticatedUserCacheError> {
        let cache_key: String = cache_key(&principal.issuer, &principal.subject);
        trace!(
            operation = "get_authenticated_user_cache",
            cache_key = %cache_key,
            "Reading authenticated-user cache entry"
        );
        let mut connection: redis::aio::MultiplexedConnection = self.redis.connection().await?;
        let serialized_user: Option<String> = connection.get(&cache_key).await.map_err(|cache_error: RedisError| {
            error!(
                operation = "get_authenticated_user_cache",
                cache_key = %cache_key,
                reason = %cache_error,
                "Authenticated-user Redis read failed"
            );
            cache_error
        })?;
        let Some(serialized_user) = serialized_user else {
            debug!(
                operation = "get_authenticated_user_cache",
                cache_key = %cache_key,
                "Authenticated-user cache miss"
            );
            return Ok(None);
        };
        let user: AuthenticatedUser = match serde_json::from_str(&serialized_user) {
            Ok(user) => user,
            Err(serialization_error) => {
                warn!(
                    operation = "get_authenticated_user_cache",
                    cache_key = %cache_key,
                    reason = %serialization_error,
                    "Discarding malformed authenticated-user cache entry"
                );
                let delete_result: Result<usize, RedisError> = connection.del(&cache_key).await;
                if let Err(cache_error) = delete_result {
                    error!(
                        operation = "delete_malformed_authenticated_user_cache",
                        cache_key = %cache_key,
                        reason = %cache_error,
                        "Malformed authenticated-user cache deletion failed"
                    );
                }
                return Ok(None);
            }
        };
        debug!(
            operation = "get_authenticated_user_cache",
            cache_key = %cache_key,
            tenant_id = %user.tenant_id,
            account_id = %user.account_id,
            "Authenticated-user cache hit"
        );
        Ok(Some(user))
    }

    pub(crate) async fn put(
        &self,
        principal: &AuthenticatedPrincipal,
        user: &AuthenticatedUser,
    ) -> Result<(), AuthenticatedUserCacheError> {
        let cache_key: String = cache_key(&principal.issuer, &principal.subject);
        let serialized_user: String =
            serde_json::to_string(user).map_err(|serialization_error: serde_json::Error| {
                error!(
                    operation = "serialize_authenticated_user_cache",
                    tenant_id = %user.tenant_id,
                    account_id = %user.account_id,
                    reason = %serialization_error,
                    "Authenticated-user cache serialization failed"
                );
                AuthenticatedUserCacheError::Serialization(serialization_error)
            })?;
        let mut connection: redis::aio::MultiplexedConnection = self.redis.connection().await?;
        let ttl_secs: u64 = self.ttl.as_secs();
        let result: Result<(), RedisError> = connection.set_ex(&cache_key, serialized_user, ttl_secs).await;
        result.map_err(|cache_error: RedisError| {
            error!(
                operation = "put_authenticated_user_cache",
                cache_key = %cache_key,
                tenant_id = %user.tenant_id,
                account_id = %user.account_id,
                ttl_secs,
                reason = %cache_error,
                "Authenticated-user Redis write failed"
            );
            cache_error
        })?;
        debug!(
            operation = "put_authenticated_user_cache",
            cache_key = %cache_key,
            tenant_id = %user.tenant_id,
            account_id = %user.account_id,
            ttl_secs,
            "Authenticated-user cache entry stored with mandatory expiry"
        );
        Ok(())
    }

    pub(crate) async fn invalidate(&self, issuer: &str, subject: &str) -> Result<(), AuthenticatedUserCacheError> {
        let cache_key: String = cache_key(issuer, subject);
        let mut connection: redis::aio::MultiplexedConnection = self.redis.connection().await?;
        let deleted_count: usize = connection.del(&cache_key).await.map_err(|cache_error: RedisError| {
            error!(
                operation = "invalidate_authenticated_user_cache",
                cache_key = %cache_key,
                reason = %cache_error,
                "Authenticated-user Redis invalidation failed"
            );
            cache_error
        })?;
        info!(
            operation = "invalidate_authenticated_user_cache",
            cache_key = %cache_key,
            deleted_count,
            "Authenticated-user cache invalidation completed"
        );
        Ok(())
    }
}

fn cache_key(issuer: &str, subject: &str) -> String {
    let mut hasher: Sha256 = Sha256::new();
    hasher.update(issuer.as_bytes());
    hasher.update([0_u8]);
    hasher.update(subject.as_bytes());
    let identity_hash: String = URL_SAFE_NO_PAD.encode(hasher.finalize());
    format!("{CACHE_KEY_PREFIX}:{identity_hash}")
}

#[cfg(test)]
mod tests {
    use super::cache_key;

    #[test]
    fn cache_key_is_deterministic_and_separates_issuer_from_subject() {
        let first_key: String = cache_key("https://auth.example.test", "subject-1");
        let repeated_key: String = cache_key("https://auth.example.test", "subject-1");
        let other_subject_key: String = cache_key("https://auth.example.test", "subject-2");
        let ambiguous_parts_key: String = cache_key("https://auth.example.testsubject-", "1");

        assert_eq!(first_key, repeated_key);
        assert_ne!(first_key, other_subject_key);
        assert_ne!(first_key, ambiguous_parts_key);
        assert!(first_key.starts_with("auth:application-user:v1:"));
    }
}
