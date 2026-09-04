pub mod finance;
pub mod staffing;

use std::sync::Arc;

use axum::Router;

use crate::{AppContext, branch};

pub fn routes() -> Router<Arc<AppContext>> {
    let routes: Router<Arc<AppContext>> = Router::new()
        .merge(branch::host::routes())
        .merge(finance::host::routes())
        .merge(staffing::host::routes())
        .merge(finance::reporting::host::routes())
        .merge(staffing::urgent_work::host::routes());

    #[cfg(feature = "planned-staffing")]
    let routes: Router<Arc<AppContext>> = routes
        .merge(staffing::planned_work::host::routes())
        .merge(staffing::planned_work::work_session::host::routes());

    routes
}

pub fn export_routes() -> Router<Arc<AppContext>> {
    finance::reporting::host::export_routes()
}
