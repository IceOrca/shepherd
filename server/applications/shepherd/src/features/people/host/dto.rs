use chrono::NaiveDate;
use serde::Deserialize;
use crate::features::people::core::{
    DepartmentInput, EmployeeAssignmentInput, EmployeeInput, EmployeeStatus, HrRecordStatus, JobPositionInput,
};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Deserialize, ToSchema)]
pub struct AttendanceCheckInRequest {
    pub facility_id: Uuid,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EmployeeUpsertRequest {
    pub account_id: Option<Uuid>,
    pub employee_code: String,
    pub display_name: String,
    pub work_email: Option<String>,
    pub work_phone: Option<String>,
    pub badge_id: Option<String>,
    pub status: EmployeeStatus,
    pub hire_date: NaiveDate,
    pub termination_date: Option<NaiveDate>,
}

impl From<EmployeeUpsertRequest> for EmployeeInput {
    fn from(value: EmployeeUpsertRequest) -> Self {
        Self {
            account_id: value.account_id,
            employee_code: value.employee_code.trim().to_ascii_lowercase(),
            display_name: value.display_name.trim().to_owned(),
            work_email: normalize_optional(value.work_email),
            work_phone: normalize_optional(value.work_phone),
            badge_id: normalize_optional(value.badge_id),
            status: value.status,
            hire_date: value.hire_date,
            termination_date: value.termination_date,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DepartmentUpsertRequest {
    pub code: String,
    pub name: String,
    pub parent_department_id: Option<Uuid>,
    pub manager_employee_id: Option<Uuid>,
    pub status: HrRecordStatus,
}

impl From<DepartmentUpsertRequest> for DepartmentInput {
    fn from(value: DepartmentUpsertRequest) -> Self {
        Self {
            code: value.code.trim().to_ascii_lowercase(),
            name: value.name.trim().to_owned(),
            parent_department_id: value.parent_department_id,
            manager_employee_id: value.manager_employee_id,
            status: value.status,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct JobPositionUpsertRequest {
    pub code: String,
    pub name: String,
    pub department_id: Option<Uuid>,
    pub status: HrRecordStatus,
}

impl From<JobPositionUpsertRequest> for JobPositionInput {
    fn from(value: JobPositionUpsertRequest) -> Self {
        Self {
            code: value.code.trim().to_ascii_lowercase(),
            name: value.name.trim().to_owned(),
            department_id: value.department_id,
            status: value.status,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EmployeeAssignmentCreateRequest {
    pub branch_id: Uuid,
    pub facility_id: Option<Uuid>,
    pub department_id: Option<Uuid>,
    pub job_id: Option<Uuid>,
    pub manager_employee_id: Option<Uuid>,
    pub date_start: NaiveDate,
    pub date_end: Option<NaiveDate>,
    pub is_primary: bool,
}

impl From<EmployeeAssignmentCreateRequest> for EmployeeAssignmentInput {
    fn from(value: EmployeeAssignmentCreateRequest) -> Self {
        Self {
            branch_id: value.branch_id,
            facility_id: value.facility_id,
            department_id: value.department_id,
            job_id: value.job_id,
            manager_employee_id: value.manager_employee_id,
            date_start: value.date_start,
            date_end: value.date_end,
            is_primary: value.is_primary,
        }
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value: String| {
        let normalized: String = value.trim().to_owned();
        (!normalized.is_empty()).then_some(normalized)
    })
}
