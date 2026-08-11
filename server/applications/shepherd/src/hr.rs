use std::sync::Arc;

use axum::Router;

use crate::{AppContext, features};

pub fn routes() -> Router<Arc<AppContext>> {
    Router::new()
        .merge(features::people::host::routes())
        .merge(features::working_schedule::host::routes())
        .nest("/payroll", features::payroll::host::routes())
}
