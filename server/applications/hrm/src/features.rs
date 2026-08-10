pub mod organization;
pub mod payroll;
pub mod people;
pub mod working_schedule;

use std::sync::Arc;

use axum::{Router, middleware::from_fn_with_state};

use crate::{AppContext, auth::middleware::require_authenticated, ratelimiting};

pub fn routes(context: Arc<AppContext>) -> Router {
    let auth: Arc<infra_auth::AuthService> = Arc::clone(&context.auth);
    let hr_routes = Router::new()
        .merge(people::host::routes())
        .merge(working_schedule::host::routes())
        .nest("/payroll", payroll::host::routes())
        .layer(ratelimiting::RateLimiter::protected_route_layer())
        .route_layer(from_fn_with_state(auth, require_authenticated))
        .with_state(Arc::clone(&context));

    Router::new()
        .nest("/hr", hr_routes)
        .nest("/business", organization::host::routes(context))
}
