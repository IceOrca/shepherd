use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use crate::features::working_schedule::core::{EmployeeScheduleAssignment, WorkingSchedule};
use uuid::Uuid;

use crate::{AppContext, auth::AuthenticatedUser};

use super::dto::{EmployeeScheduleAssignmentCreateRequest, WorkingScheduleUpsertRequest};
use super::dto::EmployeeScheduleAssignmentView;
use crate::features::people::host::handler::{hr_status, require_permission};

#[utoipa::path(
    get,
    path = "/hr/working-schedules",
    tag = "hr",
    security(("bearer_auth" = [])),
    responses((status = 200, body = [WorkingSchedule]), (status = 403), (status = 503))
)]
pub async fn list_working_schedules(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<WorkingSchedule>>, StatusCode> {
    require_permission(&user, "hr.working_schedules.read")?;
    host.core
        .working_schedules
        .list(user.tenant_id)
        .await
        .map(Json)
        .map_err(|error| hr_status("list working schedules", &user, error))
}

#[utoipa::path(
    post,
    path = "/hr/working-schedules",
    tag = "hr",
    security(("bearer_auth" = [])),
    request_body = WorkingScheduleUpsertRequest,
    responses((status = 201, body = WorkingSchedule), (status = 400), (status = 403), (status = 409), (status = 503))
)]
pub async fn create_working_schedule(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<WorkingScheduleUpsertRequest>,
) -> Result<(StatusCode, Json<WorkingSchedule>), StatusCode> {
    require_permission(&user, "hr.working_schedules.manage")?;
    let schedule: WorkingSchedule = host
        .core
        .working_schedules
        .create(user.tenant_id, payload.into(), user.account_id)
        .await
        .map_err(|error| hr_status("create working schedule", &user, error))?;
    Ok((StatusCode::CREATED, Json(schedule)))
}

#[utoipa::path(
    get,
    path = "/hr/working-schedules/{schedule_id}",
    tag = "hr",
    security(("bearer_auth" = [])),
    params(("schedule_id" = Uuid, Path)),
    responses((status = 200, body = WorkingSchedule), (status = 403), (status = 404), (status = 503))
)]
pub async fn get_working_schedule(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(schedule_id): Path<Uuid>,
) -> Result<Json<WorkingSchedule>, StatusCode> {
    require_permission(&user, "hr.working_schedules.read")?;
    host.core
        .working_schedules
        .find(user.tenant_id, schedule_id)
        .await
        .map_err(|error| hr_status("find working schedule", &user, error))?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[utoipa::path(
    put,
    path = "/hr/working-schedules/{schedule_id}",
    tag = "hr",
    security(("bearer_auth" = [])),
    params(("schedule_id" = Uuid, Path)),
    request_body = WorkingScheduleUpsertRequest,
    responses((status = 200, body = WorkingSchedule), (status = 400), (status = 403), (status = 404), (status = 409), (status = 503))
)]
pub async fn update_working_schedule(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(schedule_id): Path<Uuid>,
    Json(payload): Json<WorkingScheduleUpsertRequest>,
) -> Result<Json<WorkingSchedule>, StatusCode> {
    require_permission(&user, "hr.working_schedules.manage")?;
    host.core
        .working_schedules
        .update(user.tenant_id, schedule_id, payload.into(), user.account_id)
        .await
        .map(Json)
        .map_err(|error| hr_status("update working schedule", &user, error))
}

#[utoipa::path(
    get,
    path = "/hr/employees/{employee_id}/working-schedule-assignments",
    tag = "hr",
    security(("bearer_auth" = [])),
    params(("employee_id" = Uuid, Path)),
    responses((status = 200, body = [EmployeeScheduleAssignmentView]), (status = 403), (status = 404), (status = 503))
)]
pub async fn list_employee_schedule_assignments(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(employee_id): Path<Uuid>,
) -> Result<Json<Vec<EmployeeScheduleAssignmentView>>, StatusCode> {
    require_permission(&user, "hr.working_schedules.read")?;
    let assignments: Vec<EmployeeScheduleAssignment> = host
        .core
        .working_schedules
        .list_employee_assignments(user.tenant_id, employee_id)
        .await
        .map_err(|error| hr_status("list employee working schedule assignments", &user, error))?;
    load_assignment_views(&host, &user, assignments).await.map(Json)
}

#[utoipa::path(
    get,
    path = "/hr/employees/me/working-schedule-assignments",
    tag = "hr",
    security(("bearer_auth" = [])),
    responses((status = 200, body = [EmployeeScheduleAssignmentView]), (status = 403), (status = 404), (status = 503))
)]
pub async fn list_own_schedule_assignments(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<EmployeeScheduleAssignmentView>>, StatusCode> {
    require_permission(&user, "hr.working_schedules.self.read")?;
    let employee = host
        .core
        .people
        .find_employee_by_account(user.tenant_id, user.account_id)
        .await
        .map_err(|error| hr_status("find employee for own working schedules", &user, error))?
        .ok_or(StatusCode::NOT_FOUND)?;
    let assignments: Vec<EmployeeScheduleAssignment> = host
        .core
        .working_schedules
        .list_employee_assignments(user.tenant_id, employee.id)
        .await
        .map_err(|error| hr_status("list own working schedule assignments", &user, error))?;
    load_assignment_views(&host, &user, assignments).await.map(Json)
}

#[utoipa::path(
    post,
    path = "/hr/employees/{employee_id}/working-schedule-assignments",
    tag = "hr",
    security(("bearer_auth" = [])),
    params(("employee_id" = Uuid, Path)),
    request_body = EmployeeScheduleAssignmentCreateRequest,
    responses((status = 201, body = EmployeeScheduleAssignment), (status = 400), (status = 403), (status = 404), (status = 409), (status = 503))
)]
pub async fn create_employee_schedule_assignment(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(employee_id): Path<Uuid>,
    Json(payload): Json<EmployeeScheduleAssignmentCreateRequest>,
) -> Result<(StatusCode, Json<EmployeeScheduleAssignment>), StatusCode> {
    require_permission(&user, "hr.working_schedules.manage")?;
    let assignment: EmployeeScheduleAssignment = host
        .core
        .working_schedules
        .assign_employee(user.tenant_id, employee_id, payload.into(), user.account_id)
        .await
        .map_err(|error| hr_status("assign employee working schedule", &user, error))?;
    Ok((StatusCode::CREATED, Json(assignment)))
}

async fn load_assignment_views(
    host: &AppContext,
    user: &AuthenticatedUser,
    assignments: Vec<EmployeeScheduleAssignment>,
) -> Result<Vec<EmployeeScheduleAssignmentView>, StatusCode> {
    let mut views: Vec<EmployeeScheduleAssignmentView> = Vec::with_capacity(assignments.len());
    for assignment in assignments {
        let schedule: WorkingSchedule = host
            .core
            .working_schedules
            .find(user.tenant_id, assignment.schedule_id)
            .await
            .map_err(|error| hr_status("load assigned working schedule", user, error))?
            .ok_or_else(|| {
                hr_status(
                    "load assigned working schedule",
                    user,
                    crate::features::people::core::HrError::BackendUnavailable,
                )
            })?;
        views.push(EmployeeScheduleAssignmentView { assignment, schedule });
    }
    Ok(views)
}
