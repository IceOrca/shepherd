use std::{env, sync::Arc};

use axum::{
    Router,
    http::{
        HeaderName, HeaderValue, Method, StatusCode,
        header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
    },
    response::IntoResponse,
    routing::{MethodRouter, get},
};
#[cfg(feature = "auth")]
use axum::middleware::from_fn_with_state;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::timeout::TimeoutLayer;
use tracing::{error, warn, info, debug, trace};

use crate::HostContext;
use crate::ip_extract;
use crate::logging;

const DEFAULT_CORS_ALLOWED_ORIGINS: &str = "http://localhost:5173,http://localhost:5174";
const DEFAULT_HTTP_REQUEST_TIMEOUT_SECS: u64 = 20;

pub fn routes(host_ctx: Arc<HostContext>) -> Router {
    let host_router: Router = make_route_with_state("/", get(get_root), Arc::clone(&host_ctx));
    info!("Infra host routes initialized");
    host_router
}

pub fn apply_layers(router: Router, host_ctx: Arc<HostContext>) -> Router {
    info!("Applying host request tracing, client identification, timeout, and CORS layers");
    let router: Router = logging::layer(router);
    let router: Router = ip_extract::layer(router, host_ctx);
    router
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            http_request_timeout(),
        ))
        .layer(cors_layer())
}

fn http_request_timeout() -> std::time::Duration {
    let timeout_secs: u64 = env::var("HTTP_REQUEST_TIMEOUT_SECS")
        .ok()
        .and_then(|value: String| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_HTTP_REQUEST_TIMEOUT_SECS);
    debug!(timeout_secs, "Resolved HTTP request timeout");
    std::time::Duration::from_secs(timeout_secs)
}

fn cors_layer() -> CorsLayer {
    let origins: Vec<HeaderValue> = env::var("CORS_ALLOWED_ORIGINS")
        .unwrap_or_else(|_| DEFAULT_CORS_ALLOWED_ORIGINS.to_owned())
        .split(',')
        .filter_map(|origin: &str| {
            let origin: &str = origin.trim();
            if origin.is_empty() {
                return None;
            }
            match origin.parse::<HeaderValue>() {
                Ok(header_value) => Some(header_value),
                Err(error) => {
                    warn!(origin, reason = %error, "Ignoring invalid CORS origin");
                    None
                }
            }
        })
        .collect();
    info!(origin_count = origins.len(), "Configured allowed CORS origins");
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([
            ACCEPT,
            AUTHORIZATION,
            CONTENT_TYPE,
            HeaderName::from_static("x-tenant-id"),
            HeaderName::from_static("x-branch-id"),
            HeaderName::from_static("idempotency-key"),
        ])
}

pub async fn get_root() -> impl IntoResponse {
    info!("Received root endpoint request");
    let response: (StatusCode, axum::Json<&'static str>) = (StatusCode::OK, axum::Json("Welcome to Shepherd Server"));
    debug!(status = %response.0, "Completed root endpoint request");
    response
}

pub fn add_route<S>(router: Router<S>, path: &str, handler: MethodRouter<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    debug!(path, "Registering host route");
    router.route(path, handler)
}

pub fn make_route<S>(path: &str, handler: MethodRouter<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    debug!(path, "Creating standalone host route");
    Router::new().route(path, handler)
}

pub fn make_route_with_state<R, S>(path: &str, handler: MethodRouter<S>, state_ctx: S) -> Router<R>
where
    R: Clone + Send + Sync + 'static,
    S: Clone + Send + Sync + 'static,
{
    debug!(path, "Creating host route with application state");
    let router: Router<S> = Router::new().route(path, handler);
    let router: Router<R> = router.with_state(state_ctx);
    router
}
