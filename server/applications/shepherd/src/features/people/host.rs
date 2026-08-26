pub mod dto;
pub mod handler;

use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post, put},
};

use crate::AppContext;

pub fn routes() -> Router<Arc<AppContext>> {
    Router::new()
        .route(
            "/employees",
            get(handler::list_employees).post(handler::create_employee),
        )
        .route("/employees/me", get(handler::get_own_employee))
        .route("/employees/me/citizen-id", get(handler::get_own_employee_citizen_id))
        .route("/attendance/me", get(handler::list_own_attendance_sessions))
        .route("/attendance/check-in", post(handler::check_in))
        .route("/attendance/check-out", post(handler::check_out))
        .route(
            "/employees/{employee_id}",
            get(handler::get_employee).put(handler::update_employee),
        )
        .route(
            "/employees/{employee_id}/citizen-id",
            get(handler::get_employee_citizen_id).put(handler::update_employee_citizen_id),
        )
        .route(
            "/employees/{employee_id}/attendance",
            get(handler::list_employee_attendance_sessions),
        )
        .route(
            "/employees/{employee_id}/assignments",
            get(handler::list_employee_assignments).post(handler::create_employee_assignment),
        )
        .route(
            "/departments",
            get(handler::list_departments).post(handler::create_department),
        )
        .route("/departments/{department_id}", put(handler::update_department))
        .route("/jobs", get(handler::list_jobs).post(handler::create_job))
        .route("/jobs/{job_id}", put(handler::update_job))
}
