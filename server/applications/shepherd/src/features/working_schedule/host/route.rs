use std::sync::Arc;

use axum::{Router, routing::get};

use crate::AppContext;

use super::handler;

pub fn routes() -> Router<Arc<AppContext>> {
    Router::new()
        .route(
            "/working-schedules",
            get(handler::list_working_schedules).post(handler::create_working_schedule),
        )
        .route(
            "/working-schedules/{schedule_id}",
            get(handler::get_working_schedule).put(handler::update_working_schedule),
        )
        .route(
            "/employees/me/working-schedule-assignments",
            get(handler::list_own_schedule_assignments),
        )
        .route(
            "/employees/{employee_id}/working-schedule-assignments",
            get(handler::list_employee_schedule_assignments).post(handler::create_employee_schedule_assignment),
        )
}
