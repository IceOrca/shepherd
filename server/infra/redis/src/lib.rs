#![cfg_attr(debug_assertions, allow(unused))]

use redis::{aio::MultiplexedConnection, RedisResult};
use std::sync::Arc;
use tracing::{debug, error, info, trace, warn};

pub struct RedisAdapter {
    cache: Arc<RedisCache>,
    // queue worker are not used in the current implementation,
    // but we may use it in the future
    // queue: Arc<RedisQueue>,
}

pub struct RedisCache {
    client: redis::Client,
    url: String,
}

impl RedisAdapter {
    pub fn new_arc() -> Arc<Self> {
        let configured_url: Result<String, std::env::VarError> = std::env::var("REDIS_CACHE_URL");
        let url_is_configured: bool = configured_url.is_ok();
        debug!(url_is_configured, "Redis adapter configuration loaded");
        let url: String = configured_url.unwrap_or_else(|_error: std::env::VarError| {
            warn!("REDIS_CACHE_URL is not configured; using the local development default");
            "redis://127.0.0.1/".to_owned()
        });

        Self::connect(url).unwrap_or_else(|error: redis::RedisError| {
            error!(error = %error, "Redis client initialization failed");
            panic!("Invalid REDIS_CACHE_URL configuration")
        })
    }

    pub fn connect(url: impl Into<String>) -> RedisResult<Arc<Self>> {
        let url: String = url.into();
        trace!("Constructing Redis client");
        let client: redis::Client = redis::Client::open(url.clone()).map_err(|error: redis::RedisError| {
            error!(error = %error, "Redis connection URL validation failed");
            error
        })?;
        let cache: Arc<RedisCache> = Arc::new(RedisCache { client, url });
        info!("Redis provider initialized");
        Ok(Arc::new(Self { cache }))
    }

    pub fn client(&self) -> redis::Client {
        self.cache.client.clone()
    }

    pub async fn connection(&self) -> RedisResult<MultiplexedConnection> {
        trace!("Opening multiplexed Redis connection");
        let connection: MultiplexedConnection =
            self.cache
                .client
                .get_multiplexed_async_connection()
                .await
                .map_err(|error: redis::RedisError| {
                    error!(error = %error, "Opening multiplexed Redis connection failed");
                    error
                })?;
        debug!("Multiplexed Redis connection opened");
        Ok(connection)
    }

    pub fn url(&self) -> &str {
        &self.cache.url
    }
}
