#![cfg_attr(debug_assertions, allow(unused))]

use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, warn};

const TELEGRAM_API_ROOT: &str = "https://api.telegram.org";
const ZALO_MESSAGE_URL: &str = "https://openapi.zalo.me/v3.0/oa/message/cs";
const DEFAULT_PROVIDER_HTTP_TIMEOUT_SECS: u64 = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationChannel {
    Telegram,
    Zalo,
}

impl NotificationChannel {
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "telegram" => Some(Self::Telegram),
            "zalo" => Some(Self::Zalo),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryReceipt {
    pub provider_message_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum NotificationError {
    #[error("notification channel is not configured")]
    NotConfigured,
    #[error("notification provider request failed")]
    Transport,
    #[error("notification provider request timed out")]
    TimedOut,
    #[error("notification provider is temporarily unavailable")]
    Unavailable,
    #[error("notification provider rejected the request: {0}")]
    Rejected(String),
    #[error("notification provider returned an invalid response")]
    InvalidResponse,
}

impl NotificationError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Transport | Self::TimedOut | Self::Unavailable | Self::InvalidResponse
        )
    }
}

#[derive(Clone)]
pub struct Notifier {
    client: Client,
    telegram_bot_token: Option<String>,
    zalo_access_token: Option<String>,
}

impl Notifier {
    pub fn from_env() -> Self {
        let request_timeout_secs: u64 = positive_env_u64(
            "NOTIFICATION_PROVIDER_HTTP_TIMEOUT_SECS",
            DEFAULT_PROVIDER_HTTP_TIMEOUT_SECS,
        );
        debug!(request_timeout_secs, "Resolved notification provider HTTP timeout");
        let client = Client::builder()
            .timeout(Duration::from_secs(request_timeout_secs))
            .build()
            .unwrap_or_else(|error| panic!("failed to construct notification HTTP client: {error}"));
        Self {
            client,
            telegram_bot_token: non_empty_env("TELEGRAM_BOT_TOKEN"),
            zalo_access_token: non_empty_env("ZALO_OA_ACCESS_TOKEN"),
        }
    }

    pub async fn send(
        &self,
        channel: NotificationChannel,
        destination: &str,
        message: &str,
    ) -> Result<DeliveryReceipt, NotificationError> {
        match channel {
            NotificationChannel::Telegram => self.send_telegram(destination, message).await,
            NotificationChannel::Zalo => self.send_zalo(destination, message).await,
        }
    }

    async fn send_telegram(&self, destination: &str, message: &str) -> Result<DeliveryReceipt, NotificationError> {
        let token = self
            .telegram_bot_token
            .as_deref()
            .ok_or(NotificationError::NotConfigured)?;
        let url = format!("{TELEGRAM_API_ROOT}/bot{token}/sendMessage");
        let response = self
            .client
            .post(url)
            .json(&TelegramRequest {
                chat_id: destination,
                text: message,
            })
            .send()
            .await
            .map_err(|_| NotificationError::Transport)?;
        let status = response.status();
        if status.is_server_error() || status.as_u16() == 429 {
            return Err(NotificationError::Unavailable);
        }
        let body: TelegramResponse = response.json().await.map_err(|_| NotificationError::InvalidResponse)?;
        if !status.is_success() || !body.ok {
            return Err(NotificationError::Rejected(
                body.description.unwrap_or_else(|| format!("HTTP {status}")),
            ));
        }
        Ok(DeliveryReceipt {
            provider_message_id: body.result.map(|result| result.message_id.to_string()),
        })
    }

    async fn send_zalo(&self, destination: &str, message: &str) -> Result<DeliveryReceipt, NotificationError> {
        let token = self
            .zalo_access_token
            .as_deref()
            .ok_or(NotificationError::NotConfigured)?;
        let response = self
            .client
            .post(ZALO_MESSAGE_URL)
            .header("access_token", token)
            .json(&ZaloRequest {
                recipient: ZaloRecipient { user_id: destination },
                message: ZaloMessage { text: message },
            })
            .send()
            .await
            .map_err(|_| NotificationError::Transport)?;
        let status = response.status();
        if status.is_server_error() || status.as_u16() == 429 {
            return Err(NotificationError::Unavailable);
        }
        let body: ZaloResponse = response.json().await.map_err(|_| NotificationError::InvalidResponse)?;
        if !status.is_success() || body.error != 0 {
            return Err(NotificationError::Rejected(
                body.message
                    .unwrap_or_else(|| format!("HTTP {status}, provider error {}", body.error)),
            ));
        }
        Ok(DeliveryReceipt {
            provider_message_id: body.data.and_then(|data| data.message_id),
        })
    }
}

impl Default for Notifier {
    fn default() -> Self {
        Self::from_env()
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn positive_env_u64(name: &str, default: u64) -> u64 {
    let raw_value: String = match std::env::var(name) {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return default,
        Err(std::env::VarError::NotUnicode(_value)) => {
            warn!(
                configuration = name,
                default, "Configuration is not valid Unicode; using default"
            );
            return default;
        }
    };
    match raw_value.parse::<u64>() {
        Ok(value) if value > 0 => value,
        Ok(_zero) => {
            warn!(
                configuration = name,
                default, "Configuration must be greater than zero; using default"
            );
            default
        }
        Err(error) => {
            warn!(
                configuration = name,
                default,
                error = %error,
                "Configuration is not an unsigned integer; using default"
            );
            default
        }
    }
}

#[derive(Serialize)]
struct TelegramRequest<'a> {
    chat_id: &'a str,
    text: &'a str,
}

#[derive(Deserialize)]
struct TelegramResponse {
    ok: bool,
    description: Option<String>,
    result: Option<TelegramMessage>,
}

#[derive(Deserialize)]
struct TelegramMessage {
    message_id: i64,
}

#[derive(Serialize)]
struct ZaloRequest<'a> {
    recipient: ZaloRecipient<'a>,
    message: ZaloMessage<'a>,
}

#[derive(Serialize)]
struct ZaloRecipient<'a> {
    user_id: &'a str,
}

#[derive(Serialize)]
struct ZaloMessage<'a> {
    text: &'a str,
}

#[derive(Deserialize)]
struct ZaloResponse {
    error: i64,
    message: Option<String>,
    data: Option<ZaloResponseData>,
}

#[derive(Deserialize)]
struct ZaloResponseData {
    message_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{NotificationChannel, NotificationError};

    #[test]
    fn parses_supported_channels() {
        assert_eq!(
            NotificationChannel::from_code("telegram"),
            Some(NotificationChannel::Telegram)
        );
        assert_eq!(NotificationChannel::from_code("zalo"), Some(NotificationChannel::Zalo));
        assert_eq!(NotificationChannel::from_code("email"), None);
    }

    #[test]
    fn only_transport_failures_are_retryable() {
        assert!(NotificationError::Transport.is_retryable());
        assert!(NotificationError::TimedOut.is_retryable());
        assert!(NotificationError::Unavailable.is_retryable());
        assert!(NotificationError::InvalidResponse.is_retryable());
        assert!(!NotificationError::NotConfigured.is_retryable());
        assert!(!NotificationError::Rejected("invalid destination".to_owned()).is_retryable());
    }
}
