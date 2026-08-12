pub mod staffing;

use std::sync::Arc;

use axum::Router;

use crate::{AppContext, features};

pub fn routes() -> Router<Arc<AppContext>> {
    Router::new()
        .merge(features::organization::host::routes())
        .merge(staffing::host::routes())
        .merge(staffing::work_session::host::routes())
}
