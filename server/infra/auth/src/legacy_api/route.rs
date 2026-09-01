use std::sync::Arc;

use axum::{
    Router,
    middleware::{from_fn, from_fn_with_state},
    routing::{get, post, put},
};
use crate::AppRoutes;
use crate::ratelimiting::{RateLimitPolicy, RateLimiter};
use crate::{LegacyAuthService, bruteforce, handler, jwks, middleware as auth_middleware};

/// Complete standalone authentication router.
///
/// A host may instead consume the individual route groups to apply its own
/// rate-limit policy while keeping the auth crate independent from the host.
pub fn routes(state: &Arc<LegacyAuthService>) -> Router {
    Router::new()
        .merge(public_routes(state))
        .merge(authenticated_routes(protected_app_routes(state), state))
        .merge(authenticated_routes(admin_routes(state), state))
}

pub async fn init(
    auth_admin: Arc<dyn infra_auth::ext_service::auth_admin::ExtAuthAdmin>,
) -> (Arc<HostContext>, Router) {
    info!("Starting infra host initialization");
    let host_ctx: Arc<HostContext> = HostContext::new_arc(auth_admin).await;
    debug!("Infra host context initialized; building host routes");
    let host_router: Router = routes(Arc::clone(&host_ctx));
    let host_router: Router = apply_layers(host_router, Arc::clone(&host_ctx));
    info!("Infra host initialization completed");
    (host_ctx, host_router)
}

pub fn mount_app_routes(router: Router, routes: AppRoutes, host: Arc<HostContext>) -> Router {
    info!("Mounting public, protected, and admin application route groups");
    let public: Router = RateLimiter::public_layer(routes.public);
    let protected: Router = routes
        .protected
        .layer(RateLimiter::protected_route_layer(RateLimitPolicy::generic_protected()))
        .route_layer(from_fn_with_state(
            Arc::clone(&host.auth),
            infra_auth::ext_service::middleware::require_authenticated,
        ));
    let admin: Router = routes
        .admin
        .layer(RateLimiter::protected_route_layer(RateLimitPolicy::generic_protected()))
        .route_layer(from_fn_with_state(
            Arc::clone(&host.auth),
            infra_auth::ext_service::middleware::require_authenticated,
        ));
    let merged: Router = router.merge(public).merge(protected).merge(admin);
    debug!("Mounted public, protected, and admin application route groups");
    merged
}

pub fn public_routes(state: &Arc<LegacyAuthService>) -> Router {
    let login: Router = Router::new()
        .route("/login", post(handler::login))
        .route_layer(from_fn_with_state(
            Arc::clone(state),
            bruteforce::brute_force_guard_layer,
        ))
        .with_state(Arc::clone(state));

    let jwks: Router = Router::new()
        .route("/.well-known/jwks.json", get(jwks::jwks_handler))
        .with_state(Arc::clone(state));

    let refresh: Router = Router::new()
        .route("/refresh", post(handler::refresh_session))
        .with_state(Arc::clone(state));

    Router::new().merge(login).merge(jwks).merge(refresh)
}

/// Routes that require a valid account but no host-level administrator role.
/// Authentication middleware is applied by `routes` or by the composing host.
pub fn protected_app_routes(state: &Arc<LegacyAuthService>) -> Router {
    Router::new()
        .route("/profile", get(handler::get_profile))
        .route("/logout", post(handler::logout))
        .route("/logout-all", post(handler::logout_all))
        .route("/password", put(handler::change_own_password))
        .route("/ping", get(handler::test_ping))
        .with_state(Arc::clone(state))
}

/// Tenant account-administration routes.
///
/// Handlers retain their fine-grained permission checks. Authentication is
/// applied by `routes` or by the composing host.
pub fn admin_routes(state: &Arc<LegacyAuthService>) -> Router {
    let registration: Router = Router::new()
        .route("/register", post(handler::register_new_user))
        .with_state(Arc::clone(state))
        .route_layer(from_fn(auth_middleware::require_account_creator));

    let account_management: Router = Router::new()
        .route("/accounts", get(handler::list_accounts))
        .route("/roles", get(handler::get_authorization_catalog))
        .route("/accounts/{account_id}/status", put(handler::update_account_status))
        .route("/accounts/{account_id}/password", put(handler::reset_account_password))
        .route("/accounts/{account_id}/roles", put(handler::update_account_roles))
        .route(
            "/accounts/{account_id}/permissions",
            put(handler::update_account_permissions),
        )
        .with_state(Arc::clone(state));

    Router::new().merge(registration).merge(account_management)
}

fn authenticated_routes(router: Router, state: &Arc<LegacyAuthService>) -> Router {
    router.route_layer(from_fn_with_state(
        Arc::clone(state),
        auth_middleware::require_authenticated,
    ))
}
