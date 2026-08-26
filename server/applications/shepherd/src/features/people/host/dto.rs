use chrono::NaiveDate;
use serde::Deserialize;
use crate::features::people::core::{
    DepartmentInput, EmployeeAssignmentInput, EmployeeCitizenIdInput, EmployeeInput, EmployeeStatus, Gender,
    HrRecordStatus, JobPositionInput,
};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Deserialize, TS)]
pub struct AttendanceCheckInRequest {
    pub branch_id: Uuid,
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct EmployeeUpsertRequest {
    pub account_id: Option<Uuid>,
    pub employee_code: String,
    pub display_name: String,
    pub legal_first_name: Option<String>,
    pub legal_middle_name: Option<String>,
    pub legal_last_name: Option<String>,
    pub work_email: Option<String>,
    pub work_phone: Option<String>,
    pub personal_phone_e164: Option<String>,
    pub gender: Option<Gender>,
    pub badge_id: Option<String>,
    pub status: EmployeeStatus,
    pub hire_date: NaiveDate,
    pub termination_date: Option<NaiveDate>,
    pub expected_version: Option<i64>,
}

impl From<EmployeeUpsertRequest> for EmployeeInput {
    fn from(value: EmployeeUpsertRequest) -> Self {
        Self {
            account_id: value.account_id,
            employee_code: value.employee_code.trim().to_ascii_lowercase(),
            display_name: value.display_name.trim().to_owned(),
            legal_first_name: normalize_optional(value.legal_first_name),
            legal_middle_name: normalize_optional(value.legal_middle_name),
            legal_last_name: normalize_optional(value.legal_last_name),
            work_email: normalize_optional(value.work_email),
            work_phone: normalize_optional(value.work_phone),
            personal_phone_e164: normalize_optional(value.personal_phone_e164),
            gender: value.gender,
            badge_id: normalize_optional(value.badge_id),
            status: value.status,
            hire_date: value.hire_date,
            termination_date: value.termination_date,
            expected_version: value.expected_version,
        }
    }
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct EmployeeCitizenIdUpdateRequest {
    pub citizen_id_country_code: Option<String>,
    pub citizen_id: Option<String>,
    pub expected_version: i64,
}

impl From<EmployeeCitizenIdUpdateRequest> for EmployeeCitizenIdInput {
    fn from(value: EmployeeCitizenIdUpdateRequest) -> Self {
        Self {
            citizen_id_country_code: normalize_optional(value.citizen_id_country_code)
                .map(|country_code: String| country_code.to_ascii_uppercase()),
            citizen_id: normalize_optional(value.citizen_id).map(|citizen_id: String| {
                citizen_id
                    .chars()
                    .filter(|character: &char| !character.is_ascii_whitespace() && *character != '-')
                    .flat_map(char::to_uppercase)
                    .collect()
            }),
            expected_version: value.expected_version,
        }
    }
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
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

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
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

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct EmployeeAssignmentCreateRequest {
    pub branch_id: Uuid,
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
