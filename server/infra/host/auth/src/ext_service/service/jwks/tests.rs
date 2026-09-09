use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use axum::{Json, Router, extract::State, routing::get};
use jsonwebtoken::{Algorithm, jwk::JwkSet};
use reqwest::Client;
use tokio::{net::TcpListener, sync::Notify, task::JoinHandle, time::timeout};

use super::{CachedJwks, JwksCache};
use crate::ext_service::{AccessTokenErr, OidcJwksVerifierCfg};

const TEST_TIMEOUT: Duration = Duration::from_secs(3);

struct ProviderState {
    requests: AtomicUsize,
    entered: Notify,
    release: Notify,
    set: JwkSet,
}

struct Provider {
    state: Arc<ProviderState>,
    task: JoinHandle<()>,
    config: OidcJwksVerifierCfg,
}

async fn keys(State(state): State<Arc<ProviderState>>) -> Json<JwkSet> {
    state.requests.fetch_add(1, Ordering::SeqCst);
    state.entered.notify_one();
    state.release.notified().await;
    Json(state.set.clone())
}

impl Provider {
    async fn start(set: JwkSet) -> Self {
        let listener: TcpListener = TcpListener::bind("127.0.0.1:0").await.expect("test listener");
        let address: SocketAddr = listener.local_addr().expect("test address");
        let state: Arc<ProviderState> = Arc::new(ProviderState {
            requests: AtomicUsize::new(0),
            entered: Notify::new(),
            release: Notify::new(),
            set,
        });
        let router: Router = Router::new().route("/jwks", get(keys)).with_state(Arc::clone(&state));
        let task: JoinHandle<()> = tokio::spawn(async move {
            axum::serve(listener, router).await.expect("test provider");
        });
        let config: OidcJwksVerifierCfg = OidcJwksVerifierCfg::new(
            "https://identity.example".to_owned(),
            "authenticated".to_owned(),
            format!("http://{address}/jwks"),
            vec![Algorithm::EdDSA],
            Duration::from_secs(300),
            TEST_TIMEOUT,
            Duration::ZERO,
        )
        .expect("test config");
        Self { state, task, config }
    }
}

impl Drop for Provider {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn client() -> Client {
    Client::builder().timeout(TEST_TIMEOUT).build().expect("test client")
}

fn key_set() -> JwkSet {
    serde_json::from_str(
        r#"{"keys":[{"kty":"OKP","crv":"Ed25519","x":"11qYAYKxCrfVS_7TyW12F8t8S1m56lBiAhdkrwoCzX4","kid":"test-key","alg":"EdDSA","use":"sig"}]}"#
    ).expect("public test JWK")
}

fn aged_cache() -> JwksCache {
    JwksCache::from_snapshot(
        client(),
        CachedJwks {
            set: key_set(),
            fetched_at: Some(Instant::now() - Duration::from_secs(20)),
        },
    )
}

#[tokio::test]
async fn concurrent_refreshes_fetch_and_publish_once() {
    let provider: Provider = Provider::start(key_set()).await;
    let cache: JwksCache = JwksCache::new(client());
    let (first, second, ()) = timeout(TEST_TIMEOUT, async {
        tokio::join!(
            cache.refresh(&provider.config, false),
            cache.refresh(&provider.config, false),
            async {
                provider.state.entered.notified().await;
                assert!(
                    cache
                        .lookup("test-key", provider.config.jwks_refresh_interval)
                        .0
                        .is_none()
                );
                provider.state.release.notify_one();
            },
        )
    })
    .await
    .expect("bounded concurrent refresh");
    first.expect("first refresh");
    second.expect("coalesced refresh");
    assert_eq!(provider.state.requests.load(Ordering::SeqCst), 1);
    assert!(
        cache
            .lookup("test-key", provider.config.jwks_refresh_interval)
            .0
            .is_some()
    );
    cache
        .refresh(&provider.config, true)
        .await
        .expect("unknown-KID cooldown");
    assert_eq!(provider.state.requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failed_refresh_preserves_readable_keys_and_releases_the_context() {
    let provider: Provider = Provider::start(JwkSet { keys: Vec::new() }).await;
    let cache: JwksCache = aged_cache();
    let (result, ()) = timeout(TEST_TIMEOUT, async {
        tokio::join!(cache.refresh(&provider.config, true), async {
            provider.state.entered.notified().await;
            // Snapshot reads remain available while the mutation guard owns HTTP.
            let (key, fresh) = cache.lookup("test-key", provider.config.jwks_refresh_interval);
            assert!(key.is_some() && fresh);
            provider.state.release.notify_one();
        },)
    })
    .await
    .expect("bounded failed refresh");
    assert!(matches!(result, Err(AccessTokenErr::EmptyJwks)));
    assert!(
        cache
            .lookup("test-key", provider.config.jwks_refresh_interval)
            .0
            .is_some()
    );
    assert!(cache.mutation.try_lock().is_ok());
}

#[tokio::test]
async fn cancelled_refresh_releases_the_context_without_publishing() {
    let provider: Provider = Provider::start(key_set()).await;
    let cache: Arc<JwksCache> = Arc::new(aged_cache());
    let writer: Arc<JwksCache> = Arc::clone(&cache);
    let config: OidcJwksVerifierCfg = provider.config.clone();
    let task: JoinHandle<Result<(), AccessTokenErr>> = tokio::spawn(async move { writer.refresh(&config, true).await });
    timeout(TEST_TIMEOUT, provider.state.entered.notified())
        .await
        .expect("refresh started");
    assert!(cache.mutation.try_lock().is_err());
    task.abort();
    assert!(task.await.expect_err("cancelled refresh").is_cancelled());
    assert!(cache.mutation.try_lock().is_ok());
    assert!(
        cache
            .lookup("test-key", provider.config.jwks_refresh_interval)
            .0
            .is_some()
    );
    provider.state.release.notify_one();
}
