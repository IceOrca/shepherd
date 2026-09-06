pub mod dto;
pub mod handler;

use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppContext, auth::PermissionRouteExt};

pub fn routes() -> Router<Arc<AppContext>> {
    let router = Router::new()
        .route(
            "/employees",
            get(handler::list_employees).require_one("hr.employees.read"),
        )
        .route(
            "/employees/{employee_id}",
            get(handler::get_employee).require_one("hr.employees.read"),
        )
        .route(
            "/employees",
            post(handler::create_employee).require_one("hr.employees.manage"),
        )
        .route(
            "/employees/{employee_id}",
            axum::routing::put(handler::update_employee).require_one("hr.employees.manage"),
        )
        .route(
            "/employees/me",
            get(handler::get_own_employee).require_one("hr.employees.self.read"),
        )
        .route(
            "/employees/me/citizen-id",
            get(handler::get_own_employee_citizen_id).require_one("hr.employees.self.sensitive.read"),
        )
        .route(
            "/employees/{employee_id}/citizen-id",
            get(handler::get_employee_citizen_id).require_one("hr.employees.sensitive.read"),
        )
        .route(
            "/employees/{employee_id}/citizen-id",
            axum::routing::put(handler::update_employee_citizen_id).require_one("hr.employees.sensitive.manage"),
        );

    #[cfg(feature = "hrm-attendance")]
    let router: Router<Arc<AppContext>> = router
        .route(
            "/attendance/me",
            get(handler::list_own_attendance_sessions).require_one("hr.attendance.self.read"),
        )
        .route(
            "/attendance/check-in",
            post(handler::check_in).require_one("hr.attendance.self.manage"),
        )
        .route(
            "/attendance/check-out",
            post(handler::check_out).require_one("hr.attendance.self.manage"),
        )
        .route(
            "/employees/{employee_id}/attendance",
            get(handler::list_employee_attendance_sessions).require_one("hr.attendance.read"),
        );
    router
}
