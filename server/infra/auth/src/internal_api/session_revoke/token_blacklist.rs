use std::future::Future;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use infra_redis::RedisAdapter;
use tracing::{error, warn, info, debug, trace};

pub const DEFAULT_TOKEN_BLACKLIST_CHANNEL: &str = "infra:auth:blacklist:access_jti";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessTokenBlacklistReason {
    Logout,
    LogoutAll,
    RefreshRotated,
    SessionLimitKicked,
    AdminRevoked,
    Compromised,
    Other,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccessTokenBlacklistedEvent {
    pub jti: String,
    pub expires_at: u64,
    pub tenant_id: Option<Uuid>,
    pub account_id: Option<Uuid>,
    pub reason: AccessTokenBlacklistReason,
    pub published_at: u64,
}

impl AccessTokenBlacklistedEvent {
    pub fn new(
        jti: String,
        expires_at: u64,
        tenant_id: Option<Uuid>,
        account_id: Option<Uuid>,
        reason: AccessTokenBlacklistReason,
    ) -> Self {
        Self {
            jti,
            expires_at,
            tenant_id,
            account_id,
            reason,
            published_at: unix_now(),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at <= unix_now()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TokenBlacklistPubSubError {
    #[error("redis pub/sub unavailable")]
    RedisUnavailable,
    #[error("token blacklist event serialization failed")]
    SerializationFailed,
    #[error("token blacklist event deserialization failed")]
    DeserializationFailed,
}

pub struct RedisTokenBlacklistPubSub {
    redis: Arc<RedisAdapter>,
    channel: String,
}

impl RedisTokenBlacklistPubSub {
    pub fn new_arc(redis: Arc<RedisAdapter>) -> Arc<Self> {
        let channel: String = std::env::var("AUTH_TOKEN_BLACKLIST_CHANNEL")
            .unwrap_or_else(|_| DEFAULT_TOKEN_BLACKLIST_CHANNEL.to_string());

        info!("RedisTokenBlacklistPubSub initialized: channel={}", channel);
        Arc::new(Self { redis, channel })
    }

    pub fn channel(&self) -> &str {
        &self.channel
    }

    pub async fn publish_jti_blacklisted(
        &self,
        event: &AccessTokenBlacklistedEvent,
    ) -> Result<usize, TokenBlacklistPubSubError> {
        if event.jti.is_empty() {
            warn!("Skipped publishing empty access-token blacklist jti");
            return Ok(0);
        }
        if event.is_expired() {
            trace!(
                "Skipped publishing expired access-token blacklist event: jti={} expires_at={}",
                event.jti, event.expires_at
            );
            return Ok(0);
        }

        let payload: String = serde_json::to_string(event).map_err(|err: serde_json::Error| {
            error!(
                "Failed to serialize access-token blacklist event: jti={} error={}",
                event.jti, err
            );
            TokenBlacklistPubSubError::SerializationFailed
        })?;

        let mut connection = self.redis.connection().await.map_err(|err: redis::RedisError| {
            error!(
                "Failed to connect Redis before publishing access-token blacklist event: jti={} error={}",
                event.jti, err
            );
            TokenBlacklistPubSubError::RedisUnavailable
        })?;

        let subscriber_count: usize =
            connection
                .publish(&self.channel, payload)
                .await
                .map_err(|err: redis::RedisError| {
                    error!(
                        "Failed to publish access-token blacklist event: channel={} jti={} error={}",
                        self.channel, event.jti, err
                    );
                    TokenBlacklistPubSubError::RedisUnavailable
                })?;

        info!(
            "Published access-token blacklist event: channel={} jti={} tenant_id={:?} account_id={:?} subscribers={}",
            self.channel, event.jti, event.tenant_id, event.account_id, subscriber_count
        );
        Ok(subscriber_count)
    }

    pub async fn subscribe_jti_blacklisted<F, Fut>(&self, mut handler: F) -> Result<(), TokenBlacklistPubSubError>
    where
        F: FnMut(AccessTokenBlacklistedEvent) -> Fut + Send,
        Fut: Future<Output = ()> + Send,
    {
        let mut pubsub = self
            .redis
            .client()
            .get_async_pubsub()
            .await
            .map_err(|err: redis::RedisError| {
                error!(
                    "Failed to connect Redis pub/sub for access-token blacklist events: channel={} error={}",
                    self.channel, err
                );
                TokenBlacklistPubSubError::RedisUnavailable
            })?;

        pubsub
            .subscribe(&self.channel)
            .await
            .map_err(|err: redis::RedisError| {
                error!(
                    "Failed to subscribe access-token blacklist channel: channel={} error={}",
                    self.channel, err
                );
                TokenBlacklistPubSubError::RedisUnavailable
            })?;

        info!("Subscribed to access-token blacklist events: channel={}", self.channel);

        let mut message_stream = pubsub.on_message();
        while let Some(message) = message_stream.next().await {
            let payload: String = message.get_payload().map_err(|err: redis::RedisError| {
                warn!(
                    "Ignored malformed access-token blacklist pub/sub payload: channel={} error={}",
                    self.channel, err
                );
                TokenBlacklistPubSubError::DeserializationFailed
            })?;

            let event: AccessTokenBlacklistedEvent =
                serde_json::from_str(&payload).map_err(|err: serde_json::Error| {
                    warn!(
                        "Ignored invalid access-token blacklist event JSON: channel={} error={}",
                        self.channel, err
                    );
                    TokenBlacklistPubSubError::DeserializationFailed
                })?;

            if event.is_expired() {
                trace!(
                    "Ignored expired access-token blacklist event from pub/sub: jti={} expires_at={}",
                    event.jti, event.expires_at
                );
                continue;
            }

            handler(event).await;
        }

        warn!(
            "Redis access-token blacklist pub/sub message stream ended: channel={}",
            self.channel
        );
        Err(TokenBlacklistPubSubError::RedisUnavailable)
    }
}

fn unix_now() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(err) => {
            error!(
                "System time error while computing token blacklist event unix time: {}",
                err
            );
            0
        }
    }
}
