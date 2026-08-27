use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use tracing::{error, warn, info, debug, trace};
use crate::features::people::core::{AttendanceSession, Employee, EmployeeSensitiveProfile, HrError};
use uuid::Uuid;

use crate::{AppContext, auth::AuthenticatedUser};

use super::dto::{AttendanceCheckInRequest, EmployeeCitizenIdUpdateRequest, EmployeeUpsertRequest};

pub async fn list_employees(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<Employee>>, StatusCode> {
    require_permission(&user, "hr.employees.read")?;
    host.core
        .people
        .list_employees(user.tenant_id)
        .await
        .map(Json)
        .map_err(|error| hr_status("list employees", &user, error))
}

pub async fn create_employee(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<EmployeeUpsertRequest>,
) -> Result<(StatusCode, Json<Employee>), StatusCode> {
    require_permission(&user, "hr.employees.manage")?;
    let branch_id: Uuid = user.active_branch_id.ok_or(StatusCode::BAD_REQUEST)?;
    let employee: Employee = host
        .core
        .people
        .create_employee(user.tenant_id, branch_id, payload.into(), user.account_id)
        .await
        .map_err(|error| hr_status("create employee", &user, error))?;
    Ok((StatusCode::CREATED, Json(employee)))
}

pub async fn get_own_employee_citizen_id(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<EmployeeSensitiveProfile>, StatusCode> {
    require_permission(&user, "hr.employees.self.sensitive.read")?;
    host.core
        .people
        .find_employee_sensitive_profile_by_account(user.tenant_id, user.account_id)
        .await
        .map_err(|error| hr_status("find own employee citizen ID", &user, error))?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

pub async fn get_own_employee(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Employee>, StatusCode> {
    require_permission(&user, "hr.employees.self.read")?;
    host.core
        .people
        .find_employee_by_account(user.tenant_id, user.account_id)
        .await
        .map_err(|error| hr_status("find own employee", &user, error))?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

pub async fn list_own_attendance_sessions(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<AttendanceSession>>, StatusCode> {
    require_permission(&user, "hr.attendance.self.read")?;
    let employee: Employee = host
        .core
        .people
        .find_employee_by_account(user.tenant_id, user.account_id)
        .await
        .map_err(|error| hr_status("find employee for own attendance", &user, error))?
        .ok_or(StatusCode::NOT_FOUND)?;
    host.core
        .people
        .list_attendance_sessions(user.tenant_id, employee.id)
        .await
        .map(Json)
        .map_err(|error| hr_status("list own attendance sessions", &user, error))
}

pub async fn check_in(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(request): Json<AttendanceCheckInRequest>,
) -> Result<(StatusCode, Json<AttendanceSession>), StatusCode> {
    require_permission(&user, "hr.attendance.self.manage")?;
    let employee: Employee = host
        .core
        .people
        .find_employee_by_account(user.tenant_id, user.account_id)
        .await
        .map_err(|error| hr_status("find employee for check in", &user, error))?
        .ok_or(StatusCode::NOT_FOUND)?;
    let session: AttendanceSession = host
        .core
        .people
        .check_in(user.tenant_id, employee.id, user.account_id, request.branch_id)
        .await
        .map_err(|error| hr_status("check in", &user, error))?;
    Ok((StatusCode::CREATED, Json(session)))
}

pub async fn check_out(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<AttendanceSession>, StatusCode> {
    require_permission(&user, "hr.attendance.self.manage")?;
    let employee: Employee = host
        .core
        .people
        .find_employee_by_account(user.tenant_id, user.account_id)
        .await
        .map_err(|error| hr_status("find employee for check out", &user, error))?
        .ok_or(StatusCode::NOT_FOUND)?;
    host.core
        .people
        .check_out(user.tenant_id, employee.id, user.account_id)
        .await
        .map(Json)
        .map_err(|error| hr_status("check out", &user, error))
}

pub async fn get_employee(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(employee_id): Path<Uuid>,
) -> Result<Json<Employee>, StatusCode> {
    require_permission(&user, "hr.employees.read")?;
    host.core
        .people
        .find_employee(user.tenant_id, employee_id)
        .await
        .map_err(|error| hr_status("find employee", &user, error))?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

pub async fn list_employee_attendance_sessions(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(employee_id): Path<Uuid>,
) -> Result<Json<Vec<AttendanceSession>>, StatusCode> {
    require_permission(&user, "hr.attendance.read")?;
    host.core
        .people
        .list_attendance_sessions(user.tenant_id, employee_id)
        .await
        .map(Json)
        .map_err(|error| hr_status("list employee attendance sessions", &user, error))
}

pub async fn update_employee(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(employee_id): Path<Uuid>,
    Json(payload): Json<EmployeeUpsertRequest>,
) -> Result<Json<Employee>, StatusCode> {
    require_permission(&user, "hr.employees.manage")?;
    host.core
        .people
        .update_employee(user.tenant_id, employee_id, payload.into(), user.account_id)
        .await
        .map(Json)
        .map_err(|error| hr_status("update employee", &user, error))
}

pub async fn get_employee_citizen_id(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(employee_id): Path<Uuid>,
) -> Result<Json<EmployeeSensitiveProfile>, StatusCode> {
    require_permission(&user, "hr.employees.sensitive.read")?;
    host.core
        .people
        .find_employee_sensitive_profile(user.tenant_id, employee_id)
        .await
        .map_err(|error| hr_status("find employee citizen ID", &user, error))?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

pub async fn update_employee_citizen_id(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(employee_id): Path<Uuid>,
    Json(payload): Json<EmployeeCitizenIdUpdateRequest>,
) -> Result<Json<EmployeeSensitiveProfile>, StatusCode> {
    require_permission(&user, "hr.employees.sensitive.manage")?;
    host.core
        .people
        .update_employee_citizen_id(user.tenant_id, employee_id, payload.into(), user.account_id)
        .await
        .map(Json)
        .map_err(|error| hr_status("update employee citizen ID", &user, error))
}

pub(crate) fn require_permission(user: &AuthenticatedUser, permission: &str) -> Result<(), StatusCode> {
    if user.has_permission(permission) {
        Ok(())
    } else {
        info!(
            "HR request denied: tenant_id={} account_id={} required_permission={}",
            user.tenant_id, user.account_id, permission
        );
        Err(StatusCode::FORBIDDEN)
    }
}

pub(crate) fn hr_status(operation: &str, user: &AuthenticatedUser, error: HrError) -> StatusCode {
    let status = match error {
        HrError::NotFound => StatusCode::NOT_FOUND,
        HrError::Conflict => StatusCode::CONFLICT,
        HrError::InvalidInput(reason) => {
            info!(
                "HR input rejected: operation={} tenant_id={} account_id={} reason={}",
                operation, user.tenant_id, user.account_id, reason
            );
            StatusCode::BAD_REQUEST
        }
        HrError::BackendUnavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    if status == StatusCode::SERVICE_UNAVAILABLE {
        error!(
            "HR request failed: operation={} tenant_id={} account_id={} status={}",
            operation, user.tenant_id, user.account_id, status
        );
    }
    status
}
