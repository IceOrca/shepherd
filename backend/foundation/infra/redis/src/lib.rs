use std::sync::Arc;
use redis::{aio::MultiplexedConnection, RedisResult};
use foundation_kernel::debug::*;

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
        log_debug!("REDIS_CACHE_URL = {:?}", std::env::var("REDIS_CACHE_URL"));
        let url: String = std::env::var("REDIS_CACHE_URL").unwrap_or_else(|_| {
            log_warn!("REDIS_CACHE_URL not set, defaulting to redis://127.0.0.1/");
            "redis://127.0.0.1/".to_string()
        });

        Self::connect(url).unwrap_or_else(|_err: redis::RedisError| {
            log_error!("Invalid REDIS_CACHE_URL configuration; Redis initialization aborted");
            panic!("Invalid REDIS_CACHE_URL configuration")
        })
    }

    pub fn connect(url: impl Into<String>) -> RedisResult<Arc<Self>> {
        let url: String = url.into();
        let client: redis::Client = redis::Client::open(url.clone())?;
        log_info!("Redis provider initialized");
        let cache: Arc<RedisCache> = Arc::new(RedisCache { client, url });
        Ok(Arc::new(Self { cache }))
    }

    pub fn client(&self) -> redis::Client {
        self.cache.client.clone()
    }

    pub async fn connection(&self) -> RedisResult<MultiplexedConnection> {
        self.cache.client.get_multiplexed_async_connection().await
    }

    pub fn url(&self) -> &str {
        &self.cache.url
    }
}
