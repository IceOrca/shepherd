use std::{env, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    body::Body,
    extract::Request,
    http::{HeaderMap, StatusCode, header::RETRY_AFTER},
    response::{IntoResponse, Response},
};
use governor::middleware::StateInformationMiddleware;
pub use infra_kernel::request::PrincipalRateLimitKey;
use tower_governor::{
    GovernorLayer,
    errors::GovernorError,
    governor::{GovernorConfig, GovernorConfigBuilder},
    key_extractor::KeyExtractor,
};
use tracing::{debug, error, info};

use crate::ip_extract::OriginatorIp;

const PUBLIC_REPLENISH_MILLIS_ENV: &str = "HTTP_RATE_LIMIT_PUBLIC_REPLENISH_MILLIS";
const PUBLIC_BURST_ENV: &str = "HTTP_RATE_LIMIT_PUBLIC_BURST";
const PROTECTED_REPLENISH_MILLIS_ENV: &str = "HTTP_RATE_LIMIT_PROTECTED_REPLENISH_MILLIS";
const PROTECTED_BURST_ENV: &str = "HTTP_RATE_LIMIT_PROTECTED_BURST";
const STRICT_REPLENISH_MILLIS_ENV: &str = "HTTP_RATE_LIMIT_STRICT_REPLENISH_MILLIS";
const STRICT_BURST_ENV: &str = "HTTP_RATE_LIMIT_STRICT_BURST";

const DEFAULT_PUBLIC_REPLENISH_MILLIS: u64 = 500;
const DEFAULT_PUBLIC_BURST: u32 = 30;
const DEFAULT_PROTECTED_REPLENISH_MILLIS: u64 = 200;
const DEFAULT_PROTECTED_BURST: u32 = 80;
const DEFAULT_STRICT_REPLENISH_MILLIS: u64 = 2_000;
const DEFAULT_STRICT_BURST: u32 = 10;
const MAX_REPLENISH_MILLIS: u64 = 60_000;
const MAX_BURST: u32 = 10_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RateLimitPolicy {
    replenish_millis: u64,
    burst: u32,
}

impl RateLimitPolicy {
    pub fn from_env(
        policy_name: &'static str,
        replenish_env: &'static str,
        burst_env: &'static str,
        default_replenish_millis: u64,
        default_burst: u32,
    ) -> Self {
        let replenish_millis: u64 = configured_integer(replenish_env, default_replenish_millis, MAX_REPLENISH_MILLIS);
        let burst: u32 = u32::try_from(configured_integer(
            burst_env,
            u64::from(default_burst),
            u64::from(MAX_BURST),
        ))
        .expect("validated rate-limit burst must fit u32");
        info!(
            policy = policy_name,
            replenish_millis, burst, "Resolved HTTP rate-limit policy"
        );
        Self {
            replenish_millis,
            burst,
        }
    }

    pub fn generic_protected() -> Self {
        Self::from_env(
            "generic-protected",
            PROTECTED_REPLENISH_MILLIS_ENV,
            PROTECTED_BURST_ENV,
            DEFAULT_PROTECTED_REPLENISH_MILLIS,
            DEFAULT_PROTECTED_BURST,
        )
    }

    fn period(self) -> Duration {
        Duration::from_millis(self.replenish_millis)
    }
}

fn configured_integer(name: &'static str, default: u64, maximum: u64) -> u64 {
    let raw: Option<String> = env::var(name).ok();
    let value: u64 = raw
        .as_deref()
        .map(str::parse::<u64>)
        .transpose()
        .unwrap_or_else(|_| panic!("{name} must be an integer"))
        .unwrap_or(default);
    assert!((1..=maximum).contains(&value), "{name} must be between 1 and {maximum}");
    value
}

/// Key extractor for protected requests without coupling the host to a feature.
#[derive(Clone, Debug)]
pub struct PrincipalKeyExtractor;

impl KeyExtractor for PrincipalKeyExtractor {
    type Key = String;

    fn extract<B>(&self, request: &Request<B>) -> Result<Self::Key, GovernorError> {
        request
            .extensions()
            .get::<PrincipalRateLimitKey>()
            .map(|key: &PrincipalRateLimitKey| key.as_str().to_owned())
            .ok_or(GovernorError::UnableToExtractKey)
    }
}

/// Key extractor for unauthenticated requests after trusted-proxy IP resolution.
#[derive(Clone, Debug)]
pub struct OriginatorIpExtractor;

impl KeyExtractor for OriginatorIpExtractor {
    type Key = String;

    fn extract<B>(&self, request: &Request<B>) -> Result<Self::Key, GovernorError> {
        request
            .extensions()
            .get::<OriginatorIp>()
            .map(|ip: &OriginatorIp| ip.ip().to_string())
            .ok_or(GovernorError::UnableToExtractKey)
    }
}

pub struct RateLimiter;

impl RateLimiter {
    pub fn new_arc() -> Arc<Self> {
        Arc::new(Self)
    }

    fn response_error_handler(error: GovernorError) -> Response {
        match error {
            GovernorError::TooManyRequests { wait_time, headers } => {
                let retry_after: u64 = headers
                    .as_ref()
                    .and_then(|values: &HeaderMap| values.get(RETRY_AFTER))
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value: &str| value.parse::<u64>().ok())
                    .unwrap_or_else(|| {
                        debug!(wait_time, "Rate limiter did not provide a Retry-After header");
                        wait_time
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
                error!("Rate-limit key extraction failed");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            _ => {
                error!("Unexpected rate-limit middleware failure");
                (StatusCode::INTERNAL_SERVER_ERROR, "Rate limit error").into_response()
            }
        }
    }

    fn public_route_config() -> Arc<GovernorConfig<OriginatorIpExtractor, StateInformationMiddleware>> {
        let policy: RateLimitPolicy = RateLimitPolicy::from_env(
            "public",
            PUBLIC_REPLENISH_MILLIS_ENV,
            PUBLIC_BURST_ENV,
            DEFAULT_PUBLIC_REPLENISH_MILLIS,
            DEFAULT_PUBLIC_BURST,
        );
        Arc::new(
            GovernorConfigBuilder::default()
                .period(policy.period())
                .burst_size(policy.burst)
                .key_extractor(OriginatorIpExtractor)
                .use_headers()
                .finish()
                .expect("failed to build public rate limiter config"),
        )
    }

    pub fn public_route_layer() -> GovernorLayer<OriginatorIpExtractor, StateInformationMiddleware, Body> {
        GovernorLayer::new(Self::public_route_config()).error_handler(Self::response_error_handler)
    }

    pub fn public_layer(router: Router) -> Router {
        router.layer(Self::public_route_layer())
    }

    fn protected_route_config(
        policy: RateLimitPolicy,
    ) -> Arc<GovernorConfig<PrincipalKeyExtractor, StateInformationMiddleware>> {
        Arc::new(
            GovernorConfigBuilder::default()
                .period(policy.period())
                .burst_size(policy.burst)
                .key_extractor(PrincipalKeyExtractor)
                .use_headers()
                .finish()
                .expect("failed to build protected rate limiter config"),
        )
    }

    pub fn protected_route_layer(
        policy: RateLimitPolicy,
    ) -> GovernorLayer<PrincipalKeyExtractor, StateInformationMiddleware, Body> {
        GovernorLayer::new(Self::protected_route_config(policy)).error_handler(Self::response_error_handler)
    }

    pub fn protected_layer(router: Router, policy: RateLimitPolicy) -> Router {
        router.layer(Self::protected_route_layer(policy))
    }

    fn public_route_strict_config() -> Arc<GovernorConfig<OriginatorIpExtractor, StateInformationMiddleware>> {
        let policy: RateLimitPolicy = RateLimitPolicy::from_env(
            "strict-public",
            STRICT_REPLENISH_MILLIS_ENV,
            STRICT_BURST_ENV,
            DEFAULT_STRICT_REPLENISH_MILLIS,
            DEFAULT_STRICT_BURST,
        );
        Arc::new(
            GovernorConfigBuilder::default()
                .period(policy.period())
                .burst_size(policy.burst)
                .key_extractor(OriginatorIpExtractor)
                .use_headers()
                .finish()
                .expect("failed to build strict public rate limiter config"),
        )
    }

    pub fn public_route_strict_layer() -> GovernorLayer<OriginatorIpExtractor, StateInformationMiddleware, Body> {
        GovernorLayer::new(Self::public_route_strict_config()).error_handler(Self::response_error_handler)
    }

    pub fn public_strict_layer(router: Router) -> Router {
        router.layer(Self::public_route_strict_layer())
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use axum::{
        Router,
        body::Body,
        extract::Request,
        http::{Request as HttpRequest, StatusCode},
        middleware::{self, Next},
        response::Response,
        routing::get,
    };
    use governor::middleware::StateInformationMiddleware;
    use tower::ServiceExt;
    use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};

    use super::{
        MAX_BURST, MAX_REPLENISH_MILLIS, PrincipalKeyExtractor, PrincipalRateLimitKey, RateLimitPolicy, RateLimiter,
    };

    #[test]
    fn protected_defaults_allow_normal_spa_request_bursts() {
        let policy = RateLimitPolicy {
            replenish_millis: 100,
            burst: 120,
        };

        assert_eq!(policy.replenish_millis, 100);
        assert_eq!(policy.burst, 120);
        assert!(policy.replenish_millis <= MAX_REPLENISH_MILLIS);
        assert!(policy.burst <= MAX_BURST);
    }

    async fn identify_principal(mut request: Request, next: Next) -> Response {
        request
            .extensions_mut()
            .insert(PrincipalRateLimitKey::new("test-principal"));
        next.run(request).await
    }

    #[tokio::test]
    async fn protected_limiter_accepts_normal_spa_burst_then_returns_429() {
        let config = Arc::new(
            GovernorConfigBuilder::default()
                .period(Duration::from_secs(60))
                .burst_size(25)
                .key_extractor(PrincipalKeyExtractor)
                .use_headers()
                .finish()
                .expect("test limiter config must be valid"),
        );
        let layer: GovernorLayer<PrincipalKeyExtractor, StateInformationMiddleware, Body> =
            GovernorLayer::new(config).error_handler(RateLimiter::response_error_handler);
        let app: Router = Router::new()
            .route("/", get(|| async { StatusCode::OK }))
            .layer(layer)
            .route_layer(middleware::from_fn(identify_principal));

        for request_number in 1..=25 {
            let response: Response = app
                .clone()
                .oneshot(
                    HttpRequest::get("/")
                        .body(Body::empty())
                        .expect("test request must build"),
                )
                .await
                .expect("test request must complete");
            assert_eq!(response.status(), StatusCode::OK, "request {request_number}");
        }

        let limited: Response = app
            .oneshot(
                HttpRequest::get("/")
                    .body(Body::empty())
                    .expect("test request must build"),
            )
            .await
            .expect("test request must complete");
        assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
