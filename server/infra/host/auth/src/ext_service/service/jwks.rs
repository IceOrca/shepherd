//! Only the mutex-owned context can fetch and publish a new key set.
//! Verifiers retain a read-only snapshot so cache hits do not wait on HTTP.
use std::time::{Duration, Instant};

use jsonwebtoken::jwk::{Jwk, JwkSet};
use reqwest::Client;
use tokio::sync::{Mutex, MutexGuard, watch};
use tracing::{debug, error, info, trace};

use crate::ext_service::{AccessTokenErr, OidcJwksVerifierCfg};

const UNKNOWN_KID_REFRESH_COOLDOWN_SECS: u64 = 10;

#[derive(Default)]
struct CachedJwks {
    set: JwkSet,
    fetched_at: Option<Instant>,
}

pub(super) struct JwksCache {
    snapshot: watch::Receiver<CachedJwks>,
    mutation: Mutex<JwksMutationCtx>,
}

struct JwksMutationCtx {
    client: Client,
    publisher: watch::Sender<CachedJwks>,
}

impl JwksCache {
    pub(super) fn new(client: Client) -> Self {
        Self::from_snapshot(client, CachedJwks::default())
    }

    fn from_snapshot(client: Client, snapshot: CachedJwks) -> Self {
        let (publisher, snapshot): (watch::Sender<CachedJwks>, watch::Receiver<CachedJwks>) = watch::channel(snapshot);
        Self {
            snapshot,
            mutation: Mutex::new(JwksMutationCtx { client, publisher }),
        }
    }

    pub(super) fn lookup(&self, kid: &str, refresh_interval: Duration) -> (Option<Jwk>, bool) {
        let cache: watch::Ref<'_, CachedJwks> = self.snapshot.borrow();
        let fresh: bool = cache
            .fetched_at
            .is_some_and(|fetched_at: Instant| -> bool { fetched_at.elapsed() < refresh_interval });
        (cache.set.find(kid).cloned(), fresh)
    }

    pub(super) async fn refresh(&self, config: &OidcJwksVerifierCfg, force: bool) -> Result<(), AccessTokenErr> {
        let mut mutation: MutexGuard<'_, JwksMutationCtx> = self.mutation.lock().await;
        mutation.refresh(config, force).await
    }

    #[cfg(test)]
    pub(super) fn with_keys(client: Client, set: JwkSet) -> Self {
        Self::from_snapshot(
            client,
            CachedJwks {
                set,
                fetched_at: Some(Instant::now()),
            },
        )
    }
}

impl JwksMutationCtx {
    async fn refresh(&mut self, config: &OidcJwksVerifierCfg, force: bool) -> Result<(), AccessTokenErr> {
        let cache_age: Option<Duration> = self
            .publisher
            .borrow()
            .fetched_at
            .map(|fetched_at: Instant| -> Duration { fetched_at.elapsed() });
        let cache_is_fresh: bool =
            cache_age.is_some_and(|age: Duration| -> bool { age < config.jwks_refresh_interval });
        let unknown_kid_refresh_is_throttled: bool = force
            && cache_age.is_some_and(|age: Duration| -> bool { age.as_secs() < UNKNOWN_KID_REFRESH_COOLDOWN_SECS });
        if (cache_is_fresh && !force) || unknown_kid_refresh_is_throttled {
            trace!(
                force,
                cache_is_fresh, unknown_kid_refresh_is_throttled, "External signing-key refresh skipped"
            );
            return Ok(());
        }

        debug!(force, "Fetching external signing keys");
        let set: JwkSet = self
            .client
            .get(&config.jwks_url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(AccessTokenErr::JwksUnavailable)?
            .json::<JwkSet>()
            .await
            .map_err(AccessTokenErr::JwksUnavailable)?;
        if set.keys.is_empty() {
            error!("External identity provider returned an empty signing-key set");
            return Err(AccessTokenErr::EmptyJwks);
        }
        let key_count: usize = set.keys.len();
        self.publisher.send_replace(CachedJwks {
            set,
            fetched_at: Some(Instant::now()),
        });
        info!(key_count, force, "External signing-key cache refreshed");
        Ok(())
    }
}

#[cfg(test)]
#[path = "jwks/tests.rs"]
mod tests;
