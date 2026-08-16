use std::sync::{Arc, Weak};

use axum::{
    Router,
    middleware::{self, from_fn, from_fn_with_state},
    routing::{get, post},
};

use infra_kernel::debug::*;

use crate::{HostContext};
use crate::ratelimiting::{self, RateLimiter};

use super::{
    handler,
    dto::{CitHeader, CitPayload},
    client_token::{ClientTokenHandle, ClientTokenError, ClientTokenKey},
};
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};
use crate::ip_extract::OriginatorIp;

pub fn routes(state: &Arc<ClientTokenHandle>, host_ctx: Arc<HostContext>) -> Router {
    let init_router: Router = Router::new()
        .route("/client-init", post(handler::client_init))
        .with_state(Arc::clone(state));
    let init_router: Router = RateLimiter::public_strict_layer(init_router);

    let ctks_router: Router = Router::new()
        .route("/.well-known/ctks.json", get(handler::ctks_handler))
        .with_state(Arc::clone(state));
    let ctks_router: Router = RateLimiter::public_strict_layer(ctks_router);

    let public_route: Router = Router::new().merge(init_router).merge(ctks_router);

    Router::new().merge(public_route)
}
