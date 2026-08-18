#![cfg_attr(debug_assertions, allow(unused))]

pub mod app_routes;
pub mod audit;
pub mod client_identifying;
pub mod common;
pub mod ip_extract;
pub mod logging;
pub mod ratelimiting;
pub mod route;

use std::sync::Arc;

pub use app_routes::AppRoutes;
#[cfg(feature = "auth")]
pub use infra_auth as auth;
#[cfg(feature = "auth")]
use infra_auth::AuthService;
use tracing::{error, warn, info, debug, trace};
use infra_postgres::DatabaseAdapter;
use infra_redis::RedisAdapter;
use ratelimiting::RateLimiter;

/// Runtime context owned by the reusable HTTP host infra.
///
/// Application state may hold this context, but the infra never holds an
/// application domain service.
#[derive(Clone)]
pub struct HostContext {
    pub database: Arc<DatabaseAdapter>,
    pub redis: Arc<RedisAdapter>,
    #[cfg(feature = "auth")]
    pub auth: Arc<AuthService>,
    pub ip: String,
    pub port: u16,
    pub ratelimiter: Arc<RateLimiter>,
}

impl HostContext {
    pub async fn new_arc() -> Arc<Self> {
        let database: Arc<DatabaseAdapter> = DatabaseAdapter::new_arc().await;
        let redis: Arc<RedisAdapter> = RedisAdapter::new_arc();
        #[cfg(feature = "auth")]
        let auth: Arc<AuthService> = AuthService::new(database.clone())
            .await
            .unwrap_or_else(|error| panic!("failed to initialize access-token authentication: {error}"));

        Arc::new(Self {
            database,
            redis,
            #[cfg(feature = "auth")]
            auth,
            ip: std::env::var("HOST_IP").unwrap_or_else(|_| {
                warn!("HOST_IP not set, defaulting to 127.0.0.1");
                "127.0.0.1".to_owned()
            }),
            port: std::env::var("HOST_PORT")
                .unwrap_or_else(|_| panic!("HOST_PORT not set"))
                .parse()
                .unwrap_or_else(|_| panic!("HOST_PORT is not a valid number")),
            ratelimiter: RateLimiter::new_arc(),
        })
    }
}
