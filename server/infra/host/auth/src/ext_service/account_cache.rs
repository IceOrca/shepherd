use std::{sync::Arc, time::Duration};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use infra_redis::RedisAdapter;
use redis::{AsyncCommands, RedisError};
use sha2::{Digest, Sha256};
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;
use redis::aio::MultiplexedConnection;

use super::{AuthedPrincipal, account::AuthedUser};

const CACHE_KEY_PREFIX: &str = "auth:application-user:v2";
const DEFAULT_CACHE_TTL_SECS: u64 = 60;
const MAX_CACHE_TTL_SECS: u64 = 3_600;

#[derive(Debug, thiserror::Error)]
pub enum AuthedCacheCfgErr {
    #[error("AUTH_ACCOUNT_CACHE_TTL_SECS must be an integer between 1 and {MAX_CACHE_TTL_SECS}")]
    InvalidTtl,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum AuthedCacheErr {
    #[error("authenticated-user Redis operation failed")]
    Redis(#[from] RedisError),
    #[error("authenticated-user cache serialization failed")]
    Serialization(#[from] serde_json::Error),
}

pub(crate) struct AuthedUserCache {
    redis: Arc<RedisAdapter>,
    ttl: Duration,
}

impl AuthedUserCache {
    pub(crate) fn from_env(redis: Arc<RedisAdapter>) -> Result<Arc<Self>, AuthedCacheCfgErr> {
        let ttl_secs: u64 =
            std::env::var("AUTH_ACCOUNT_CACHE_TTL_SECS").map_or(Ok(DEFAULT_CACHE_TTL_SECS), |raw_value: String| {
                raw_value
                    .parse::<u64>()
                    .ok()
                    .filter(|value: &u64| (1..=MAX_CACHE_TTL_SECS).contains(value))
                    .ok_or(AuthedCacheCfgErr::InvalidTtl)
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
        principal: &AuthedPrincipal,
        tenant_id: Uuid,
    ) -> Result<Option<AuthedUser>, AuthedCacheErr> {
        let cache_key: String = cache_key(&principal.issuer, &principal.subject, tenant_id);
        trace!(
            operation = "get_authenticated_user_cache",
            cache_key = %cache_key,
            "Reading authenticated-user cache entry"
        );
        let mut conn: MultiplexedConnection = self.redis.connection().await?;
        let serz_user: Option<String> = conn.get(&cache_key).await.map_err(|err: RedisError| {
            error!(
                operation = "get_authenticated_user_cache",
                cache_key = %cache_key,
                reason = %err,
                "Authed-user Redis read failed"
            );
            err
        })?;
        let Some(serz_user) = serz_user else {
            debug!(
                operation = "get_authenticated_user_cache",
                cache_key = %cache_key,
                "Authed-user cache miss"
            );
            return Ok(None);
        };
        let user: AuthedUser = match serde_json::from_str(&serz_user) {
            Ok(user) => user,
            Err(err) => {
                warn!(
                    operation = "get_authenticated_user_cache",
                    cache_key = %cache_key,
                    reason = %err,
                    "Discarding malformed authenticated-user cache entry"
                );
                let delete_result: Result<usize, RedisError> = conn.del(&cache_key).await;
                if let Err(err) = delete_result {
                    error!(
                        operation = "delete_malformed_authenticated_user_cache",
                        cache_key = %cache_key,
                        reason = %err,
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
            "Authed-user cache hit"
        );
        Ok(Some(user))
    }

    pub(crate) async fn put(&self, principal: &AuthedPrincipal, user: &AuthedUser) -> Result<(), AuthedCacheErr> {
        let cache_key: String = cache_key(&principal.issuer, &principal.subject, user.tenant_id);
        let serz_user: String = serde_json::to_string(user).map_err(|err: serde_json::Error| {
            error!(
                operation = "serialize_authenticated_user_cache",
                tenant_id = %user.tenant_id,
                account_id = %user.account_id,
                reason = %err,
                "Authed-user cache serialization failed"
            );
            AuthedCacheErr::Serialization(err)
        })?;
        let mut conn: MultiplexedConnection = self.redis.connection().await?;
        let ttl_secs: u64 = self.ttl.as_secs();
        let result: Result<(), RedisError> = conn.set_ex(&cache_key, serz_user, ttl_secs).await;
        result.map_err(|err: RedisError| {
            error!(
                operation = "put_authenticated_user_cache",
                cache_key = %cache_key,
                tenant_id = %user.tenant_id,
                account_id = %user.account_id,
                ttl_secs,
                reason = %err,
                "Authed-user Redis write failed"
            );
            err
        })?;
        debug!(
            operation = "put_authenticated_user_cache",
            cache_key = %cache_key,
            tenant_id = %user.tenant_id,
            account_id = %user.account_id,
            ttl_secs,
            "Authed-user cache entry stored with mandatory expiry"
        );
        Ok(())
    }

    pub(crate) async fn invalidate(&self, issuer: &str, subject: &str, tenant_id: Uuid) -> Result<(), AuthedCacheErr> {
        let cache_key: String = cache_key(issuer, subject, tenant_id);
        let mut conn: MultiplexedConnection = self.redis.connection().await?;
        let deleted_count: usize = conn.del(&cache_key).await.map_err(|err: RedisError| {
            error!(
                operation = "invalidate_authenticated_user_cache",
                cache_key = %cache_key,
                reason = %err,
                "Authed-user Redis invalidation failed"
            );
            err
        })?;
        info!(
            operation = "invalidate_authenticated_user_cache",
            cache_key = %cache_key,
            deleted_count,
            "Authed-user cache invalidation completed"
        );
        Ok(())
    }
}

fn cache_key(issuer: &str, subject: &str, tenant_id: Uuid) -> String {
    let mut hasher: Sha256 = Sha256::new();
    hasher.update(issuer.as_bytes());
    hasher.update([0_u8]);
    hasher.update(subject.as_bytes());
    hasher.update([0_u8]);
    hasher.update(tenant_id.as_bytes());
    let identity_hash: String = URL_SAFE_NO_PAD.encode(hasher.finalize());
    format!("{CACHE_KEY_PREFIX}:{identity_hash}")
}

#[cfg(test)]
mod tests {
    use super::cache_key;
    use uuid::Uuid;

    #[test]
    fn cache_key_is_deterministic_and_separates_identity_and_tenant() {
        let first_tenant_id: Uuid = Uuid::from_u128(1);
        let second_tenant_id: Uuid = Uuid::from_u128(2);
        let first_key: String = cache_key("https://auth.example.test", "subject-1", first_tenant_id);
        let repeated_key: String = cache_key("https://auth.example.test", "subject-1", first_tenant_id);
        let other_subject_key: String = cache_key("https://auth.example.test", "subject-2", first_tenant_id);
        let other_tenant_key: String = cache_key("https://auth.example.test", "subject-1", second_tenant_id);
        let ambiguous_parts_key: String = cache_key("https://auth.example.testsubject-", "1", first_tenant_id);

        assert_eq!(first_key, repeated_key);
        assert_ne!(first_key, other_subject_key);
        assert_ne!(first_key, other_tenant_key);
        assert_ne!(first_key, ambiguous_parts_key);
        assert!(first_key.starts_with("auth:application-user:v2:"));
    }
}
