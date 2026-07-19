use std::{env, sync::Arc};

use axum::{
    Json, Router,
    http::{
        HeaderValue, Method, StatusCode,
        header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE},
    },
    response::IntoResponse,
    routing::{MethodRouter, get},
};
#[cfg(feature = "auth")]
use axum::middleware::{from_fn, from_fn_with_state};
use foundation_kernel::debug::*;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::timeout::TimeoutLayer;

use crate::logging;
#[cfg(feature = "auth")]
use crate::AppRoutes;
use crate::HostContext;
use crate::ip_extract;
#[cfg(feature = "auth")]
use crate::ratelimiting::RateLimitHandle;

const DEFAULT_CORS_ALLOWED_ORIGINS: &str = "http://localhost:5173,http://localhost:5174"; // Vite dev proxy
const DEFAULT_HTTP_REQUEST_TIMEOUT_SECS: u64 = 20;

pub async fn init() -> (Arc<HostContext>, Router) {
    let host_iface: Arc<HostContext> = HostContext::new_arc().await;
    let host_router: Router = routes(Arc::clone(&host_iface));
    let host_router: Router = apply_layers(host_router, Arc::clone(&host_iface));
    (host_iface, host_router)
}

pub fn routes(host_iface: Arc<HostContext>) -> Router {
    let host_router: Router = make_route_with_state("/", get(get_root), Arc::clone(&host_iface));
    #[cfg(feature = "auth")]
    let host_router: Router = host_router.nest("/auth", auth_routes(&host_iface.auth));
    log_notice!("Foundation host routes initialized");
    host_router
}

/// Mount finalized application route groups under the host's shared policy.
///
/// This API is available when the host's default `auth` feature is enabled.
/// Applications keep ownership of handlers and state; the host owns common
/// authentication, tenant-owner, and rate-limit layers.
#[cfg(feature = "auth")]
pub fn mount_app_routes(router: Router, routes: AppRoutes, host: Arc<HostContext>) -> Router {
    let public: Router = RateLimitHandle::public_layer(routes.public);
    let protected: Router = routes
        .protected
        .layer(RateLimitHandle::protected_route_layer())
        .route_layer(from_fn_with_state(
            Arc::clone(&host.auth),
            foundation_auth::middleware::require_authenticated,
        ));
    let admin: Router = routes
        .admin
        .layer(RateLimitHandle::protected_route_layer())
        .route_layer(from_fn(foundation_auth::middleware::require_tenant_owner))
        .route_layer(from_fn_with_state(
            Arc::clone(&host.auth),
            foundation_auth::middleware::require_authenticated,
        ));

    router.merge(public).merge(protected).merge(admin)
}

#[cfg(feature = "auth")]
fn auth_routes(auth: &Arc<foundation_auth::AuthService>) -> Router {
    let public: Router = foundation_auth::route::public_routes(auth).layer(RateLimitHandle::public_route_layer());
    let protected: Router = foundation_auth::route::protected_routes(auth)
        .layer(RateLimitHandle::protected_route_layer())
        .route_layer(from_fn_with_state(
            Arc::clone(auth),
            foundation_auth::middleware::require_authenticated,
        ));
    let admin: Router = foundation_auth::route::admin_routes(auth)
        .layer(RateLimitHandle::protected_route_layer())
        .route_layer(from_fn_with_state(
            Arc::clone(auth),
            foundation_auth::middleware::require_authenticated,
        ));

    Router::new().merge(public).merge(protected).merge(admin)
}

pub fn apply_layers(router: Router, host_iface: Arc<HostContext>) -> Router {
    let router: Router = logging::layer(router);
    let router: Router = ip_extract::layer(router, host_iface);
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

    std::time::Duration::from_secs(timeout_secs)
}

fn cors_layer() -> CorsLayer {
    let origins: Vec<HeaderValue> = env::var("CORS_ALLOWED_ORIGINS")
        .unwrap_or_else(|_| DEFAULT_CORS_ALLOWED_ORIGINS.to_string())
        .split(',')
        .filter_map(|origin: &str| {
            let origin: &str = origin.trim();
            if origin.is_empty() {
                return None;
            }

            match origin.parse::<HeaderValue>() {
                Ok(origin) => Some(origin),
                Err(err) => {
                    log_warn!("Ignoring invalid CORS origin '{}': {}", origin, err);
                    None
                }
            }
        })
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([ACCEPT, AUTHORIZATION, CONTENT_TYPE])
}

pub async fn get_root() -> impl IntoResponse {
    log_info!("Received request for root endpoint");

    (StatusCode::OK, Json("Welcome to Shepherd Server"))
}

pub fn add_route<S>(router: Router<S>, path: &str, handler: MethodRouter<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router.route(path, handler)
}

pub fn make_route<S>(path: &str, handler: MethodRouter<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new().route(path, handler)
}

pub fn make_route_with_state<R, S>(path: &str, handler: MethodRouter<S>, state_ctx: S) -> Router<R>
where
    R: Clone + Send + Sync + 'static,
    S: Clone + Send + Sync + 'static,
{
    let router: Router<S> = Router::new().route(path, handler);
    let router: Router<R> = router.with_state(state_ctx);
    router
}
