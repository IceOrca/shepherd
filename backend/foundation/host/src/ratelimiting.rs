use std::net::IpAddr;

// use governor::{RateLimiter};
// use governor::clock::{QuantaInstant, DefaultClock};
use governor::middleware::StateInformationMiddleware;
use governor::clock::Clock;
use governor::state::keyed::DefaultKeyedStateStore;
use tower_governor::key_extractor::{KeyExtractor, PeerIpKeyExtractor};
use std::net::SocketAddr;
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use tower_governor::governor::GovernorConfig;
use std::num::NonZeroU32;
use std::sync::Arc;
use axum::{middleware::Next, response::Response};
use axum::{
    Json, Router,
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{MethodRouter, get, post},
    body::Body,
};

use axum::{
    extract::{ConnectInfo, Request},
    http::HeaderMap,
};
use axum::{
    extract::{Extension, State},
    http::header::{COOKIE, SET_COOKIE, USER_AGENT, ACCEPT_LANGUAGE, ACCEPT_ENCODING, RETRY_AFTER},
};
use axum::http::header::{HeaderValue, InvalidHeaderValue};

use foundation_kernel::debug::*;
pub use foundation_kernel::request::PrincipalRateLimitKey;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use validator::Validate;

use crate::ip_extract::{self, OriginatorIp};

use tower_governor::{errors::GovernorError};
use governor::Quota;

/// Key extractor for protected requests without coupling the host to a feature.
#[derive(Clone, Debug)]
pub struct PrincipalKeyExtractor;

impl KeyExtractor for PrincipalKeyExtractor {
    type Key = String;

    fn extract<B>(&self, req: &Request<B>) -> Result<Self::Key, GovernorError> {
        req.extensions()
            .get::<PrincipalRateLimitKey>()
            .map(|key: &PrincipalRateLimitKey| key.as_str().to_owned())
            .ok_or(GovernorError::UnableToExtractKey)
    }
}

/// KeyExtractor for IP
#[derive(Clone, Debug)]
pub struct OriginatorIpExtractor;

impl KeyExtractor for OriginatorIpExtractor {
    type Key = String;

    fn extract<B>(&self, req: &Request<B>) -> Result<Self::Key, GovernorError> {
        req.extensions()
            .get::<OriginatorIp>()
            .map(|ip: &OriginatorIp| ip.ip().to_string())
            .ok_or(GovernorError::UnableToExtractKey)
    }
}

pub struct RateLimitHandle;

impl RateLimitHandle {
    pub fn new_arc() -> Arc<Self> {
        Arc::new(Self)
    }

    fn response_error_handler(err: GovernorError) -> Response {
        match err {
            GovernorError::TooManyRequests { wait_time, headers } => {
                let wait_seconds: u64 = wait_time;
                let retry_after: u64 = headers
                    .as_ref()
                    .and_then(|h: &HeaderMap| h.get(RETRY_AFTER))
                    .and_then(|v: &HeaderValue| v.to_str().ok())
                    .and_then(|s: &str| s.parse::<u64>().ok())
                    .unwrap_or_else(|| {
                        log_debug!("error_handler caller does not set RETRY_AFTER header");
                        wait_seconds
                    });
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(serde_json::json!({
                        "error": "Too Many Requests",
                        "retry_after": retry_after
                    })),
                )
                    .into_response()
            }

            GovernorError::UnableToExtractKey => {
                // Can not extract Key - middleware did not inject
                log_error!("Rate limit key extraction failed");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }

            _ => {
                // Handle other GovernorError variants (like Key Extraction failures)
                (StatusCode::INTERNAL_SERVER_ERROR, "Rate limit error").into_response()
            }
        }
    }

    fn public_route_config() -> Arc<GovernorConfig<OriginatorIpExtractor, StateInformationMiddleware>> {
        let config: Arc<GovernorConfig<OriginatorIpExtractor, StateInformationMiddleware>> = Arc::new(
            GovernorConfigBuilder::default()
                .period(Duration::from_secs(10)) // replenished 10 s per burst shot
                .burst_size(5)
                .key_extractor(OriginatorIpExtractor)
                .use_headers()
                .finish()
                .expect("Failed to build rate limiter config"),
        );

        config
    }

    pub fn public_route_layer() -> GovernorLayer<OriginatorIpExtractor, StateInformationMiddleware, Body> {
        let config: Arc<GovernorConfig<OriginatorIpExtractor, StateInformationMiddleware>> =
            Self::public_route_config();
        let layer: GovernorLayer<OriginatorIpExtractor, StateInformationMiddleware, Body> =
            GovernorLayer::new(config).error_handler(Self::response_error_handler);

        layer
    }

    pub fn public_layer(router: Router) -> Router {
        let config: Arc<GovernorConfig<OriginatorIpExtractor, StateInformationMiddleware>> =
            Self::public_route_config();
        let layer: GovernorLayer<OriginatorIpExtractor, StateInformationMiddleware, Body> =
            GovernorLayer::new(config).error_handler(Self::response_error_handler);

        router.layer(layer)
    }

    fn protected_route_config() -> Arc<GovernorConfig<PrincipalKeyExtractor, StateInformationMiddleware>> {
        let config: Arc<GovernorConfig<PrincipalKeyExtractor, StateInformationMiddleware>> = Arc::new(
            GovernorConfigBuilder::default()
                .period(Duration::from_secs(5)) // replenished 5 s per burst shot
                .burst_size(20) // burst 20
                .key_extractor(PrincipalKeyExtractor)
                .use_headers()
                .finish()
                .expect("Failed to build rate limiter config"),
        );

        config
    }

    pub fn protected_route_layer() -> GovernorLayer<PrincipalKeyExtractor, StateInformationMiddleware, Body> {
        let config: Arc<GovernorConfig<PrincipalKeyExtractor, StateInformationMiddleware>> =
            Self::protected_route_config();
        let layer: GovernorLayer<PrincipalKeyExtractor, StateInformationMiddleware, Body> =
            GovernorLayer::new(config).error_handler(Self::response_error_handler);
        layer
    }

    pub fn protected_layer(router: Router) -> Router {
        let config: Arc<GovernorConfig<PrincipalKeyExtractor, StateInformationMiddleware>> =
            Self::protected_route_config();
        let layer: GovernorLayer<PrincipalKeyExtractor, StateInformationMiddleware, Body> =
            GovernorLayer::new(config).error_handler(Self::response_error_handler);

        router.layer(layer)
    }

    fn public_route_strict_config() -> Arc<GovernorConfig<OriginatorIpExtractor, StateInformationMiddleware>> {
        let config: Arc<GovernorConfig<OriginatorIpExtractor, StateInformationMiddleware>> = Arc::new(
            GovernorConfigBuilder::default()
                .period(Duration::from_secs(12)) // replenished 12 s per burst shot
                .burst_size(5)
                .key_extractor(OriginatorIpExtractor)
                .use_headers()
                .finish()
                .expect("Failed to build rate limiter config"),
        );

        config
    }

    pub fn public_route_strict_layer() -> GovernorLayer<OriginatorIpExtractor, StateInformationMiddleware, Body> {
        let config: Arc<GovernorConfig<OriginatorIpExtractor, StateInformationMiddleware>> =
            Self::public_route_strict_config();
        let layer: GovernorLayer<OriginatorIpExtractor, StateInformationMiddleware, Body> =
            GovernorLayer::new(config).error_handler(Self::response_error_handler);
        layer
    }

    pub fn public_strict_layer(router: Router) -> Router {
        let config: Arc<GovernorConfig<OriginatorIpExtractor, StateInformationMiddleware>> =
            Self::public_route_strict_config();
        let layer: GovernorLayer<OriginatorIpExtractor, StateInformationMiddleware, Body> =
            GovernorLayer::new(config).error_handler(Self::response_error_handler);

        router.layer(layer)
    }
}
