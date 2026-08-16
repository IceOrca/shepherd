use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use moka::Expiry;
use moka::future::Cache;

use infra_kernel::debug::*;

#[path = "session_revoke/token_blacklist.rs"]
pub mod token_blacklist;

struct RevokedJtiExpiry;

impl Expiry<String, u64> for RevokedJtiExpiry {
    fn expire_after_create(&self, _key: &String, expires_at: &u64, _created_at: Instant) -> Option<Duration> {
        Some(Duration::from_secs(expires_at.saturating_sub(unix_now())))
    }

    fn expire_after_update(
        &self,
        _key: &String,
        expires_at: &u64,
        _updated_at: Instant,
        _duration: Option<Duration>,
    ) -> Option<Duration> {
        Some(Duration::from_secs(expires_at.saturating_sub(unix_now())))
    }
}

pub struct AccessRevocationCache {
    revoked_jtis: Cache<String, u64>,
}

impl AccessRevocationCache {
    pub fn new_arc() -> Arc<Self> {
        // Revocation is an authorization decision: an unexpired JTI must not be
        // evicted due to cache pressure and become usable again.
        log_info!("AccessRevocationCache initialized: eviction=expiration_only");
        Arc::new(Self {
            revoked_jtis: Cache::builder().expire_after(RevokedJtiExpiry).build(),
        })
    }

    pub async fn revoke_jti(&self, jti: &str, expires_at: u64) {
        let now: u64 = unix_now();
        if jti.is_empty() || expires_at <= now {
            log_debug!(
                "Ignored revoked access jti because it is empty or expired: jti={} expires_at={} now={}",
                jti,
                expires_at,
                now
            );
            return;
        }

        self.revoked_jtis.insert(jti.to_string(), expires_at).await;
        log_debug!(
            "Access jti revoked locally: jti={} expires_at={} ttl={}s",
            jti,
            expires_at,
            expires_at.saturating_sub(now)
        );
    }

    pub async fn is_revoked(&self, jti: &str) -> bool {
        let Some(expires_at) = self.revoked_jtis.get(jti).await else {
            return false;
        };

        let now: u64 = unix_now();
        if expires_at <= now {
            self.revoked_jtis.invalidate(jti).await;
            log_trace!(
                "Expired revoked access jti removed during lookup: jti={} expires_at={} now={}",
                jti,
                expires_at,
                now
            );
            return false;
        }

        true
    }
}

fn unix_now() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(err) => {
            log_error!("System time error while computing access revocation unix time: {}", err);
            0
        }
    }
}
