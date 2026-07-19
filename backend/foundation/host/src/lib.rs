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
pub use foundation_auth as auth;
#[cfg(feature = "auth")]
use foundation_auth::AuthService;
use foundation_kernel::debug::*;
use foundation_postgres::DatabaseAdapter;
use foundation_redis::RedisAdapter;
use ratelimiting::RateLimitHandle;

/// Runtime context owned by the reusable HTTP host foundation.
///
/// Application state may hold this context, but the foundation never holds an
/// application domain service.
#[derive(Clone)]
pub struct HostContext {
    pub database: Arc<DatabaseAdapter>,
    pub redis: Arc<RedisAdapter>,
    #[cfg(feature = "auth")]
    pub auth: Arc<AuthService>,
    pub ip: String,
    pub port: u16,
    pub ratelimiter: Arc<RateLimitHandle>,
}

impl HostContext {
    pub async fn new_arc() -> Arc<Self> {
        let database: Arc<DatabaseAdapter> = DatabaseAdapter::new_arc().await;
        let redis: Arc<RedisAdapter> = RedisAdapter::new_arc();
        #[cfg(feature = "auth")]
        let auth: Arc<AuthService> = AuthService::from_adapters(Arc::clone(&database), Arc::clone(&redis)).await;

        Arc::new(Self {
            database,
            redis,
            #[cfg(feature = "auth")]
            auth,
            ip: std::env::var("HOST_IP").unwrap_or_else(|_| {
                log_warn!("HOST_IP not set, defaulting to 127.0.0.1");
                "127.0.0.1".to_owned()
            }),
            port: std::env::var("HOST_PORT")
                .unwrap_or_else(|_| panic!("HOST_PORT not set"))
                .parse()
                .unwrap_or_else(|_| panic!("HOST_PORT is not a valid number")),
            ratelimiter: RateLimitHandle::new_arc(),
        })
    }
}
