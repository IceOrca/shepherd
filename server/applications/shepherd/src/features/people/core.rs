use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EmployeeStatus {
    Active,
    OnLeave,
    Terminated,
}

impl EmployeeStatus {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::OnLeave => "on_leave",
            Self::Terminated => "terminated",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "active" => Some(Self::Active),
            "on_leave" => Some(Self::OnLeave),
            "terminated" => Some(Self::Terminated),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HrRecordStatus {
    Active,
    Archived,
}

impl HrRecordStatus {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "active" => Some(Self::Active),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Employee {
    pub id: Uuid,
    pub account_id: Option<Uuid>,
    pub employee_code: String,
    pub display_name: String,
    pub work_email: Option<String>,
    pub work_phone: Option<String>,
    pub badge_id: Option<String>,
    pub status: EmployeeStatus,
    pub hire_date: NaiveDate,
    pub termination_date: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Department {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub parent_department_id: Option<Uuid>,
    pub manager_employee_id: Option<Uuid>,
    pub status: HrRecordStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct JobPosition {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub department_id: Option<Uuid>,
    pub status: HrRecordStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct EmployeeAssignment {
    pub id: Uuid,
    pub employee_id: Uuid,
    pub branch_id: Uuid,
    pub facility_id: Option<Uuid>,
    pub department_id: Option<Uuid>,
    pub job_id: Option<Uuid>,
    pub manager_employee_id: Option<Uuid>,
    pub date_start: NaiveDate,
    pub date_end: Option<NaiveDate>,
    pub is_primary: bool,
    pub created_at: DateTime<Utc>,
}

/// One contiguous employee work session. A workday can contain multiple
/// completed sessions, for example before and after an unpaid break.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct AttendanceSession {
    pub id: Uuid,
    pub employee_id: Uuid,
    pub facility_id: Uuid,
    pub check_in_at: DateTime<Utc>,
    pub check_out_at: Option<DateTime<Utc>>,
    pub worked_seconds: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct EmployeeInput {
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

#[derive(Clone, Debug)]
pub struct DepartmentInput {
    pub code: String,
    pub name: String,
    pub parent_department_id: Option<Uuid>,
    pub manager_employee_id: Option<Uuid>,
    pub status: HrRecordStatus,
}

#[derive(Clone, Debug)]
pub struct JobPositionInput {
    pub code: String,
    pub name: String,
    pub department_id: Option<Uuid>,
    pub status: HrRecordStatus,
}

#[derive(Clone, Debug)]
pub struct EmployeeAssignmentInput {
    pub branch_id: Uuid,
    pub facility_id: Option<Uuid>,
    pub department_id: Option<Uuid>,
    pub job_id: Option<Uuid>,
    pub manager_employee_id: Option<Uuid>,
    pub date_start: NaiveDate,
    pub date_end: Option<NaiveDate>,
    pub is_primary: bool,
}

#[derive(Debug)]
pub enum HrError {
    NotFound,
    Conflict,
    InvalidInput(&'static str),
    BackendUnavailable,
}

#[async_trait]
pub trait PeopleRepo {
    async fn list_employees(&self, tenant_id: Uuid) -> Result<Vec<Employee>, HrError>;
    async fn find_employee(&self, tenant_id: Uuid, employee_id: Uuid) -> Result<Option<Employee>, HrError>;
    async fn find_employee_by_account(&self, tenant_id: Uuid, account_id: Uuid) -> Result<Option<Employee>, HrError>;
    async fn create_employee(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        input: &EmployeeInput,
        audit_account_id: Uuid,
    ) -> Result<Employee, HrError>;
    async fn update_employee(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        input: &EmployeeInput,
        audit_account_id: Uuid,
    ) -> Result<Employee, HrError>;

    async fn list_departments(&self, tenant_id: Uuid) -> Result<Vec<Department>, HrError>;
    async fn create_department(
        &self,
        tenant_id: Uuid,
        department_id: Uuid,
        input: &DepartmentInput,
        audit_account_id: Uuid,
    ) -> Result<Department, HrError>;
    async fn update_department(
        &self,
        tenant_id: Uuid,
        department_id: Uuid,
        input: &DepartmentInput,
        audit_account_id: Uuid,
    ) -> Result<Department, HrError>;

    async fn list_jobs(&self, tenant_id: Uuid) -> Result<Vec<JobPosition>, HrError>;
    async fn create_job(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        input: &JobPositionInput,
        audit_account_id: Uuid,
    ) -> Result<JobPosition, HrError>;
    async fn update_job(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        input: &JobPositionInput,
        audit_account_id: Uuid,
    ) -> Result<JobPosition, HrError>;

    async fn list_assignments(&self, tenant_id: Uuid, employee_id: Uuid) -> Result<Vec<EmployeeAssignment>, HrError>;
    async fn create_assignment(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        employee_id: Uuid,
        input: &EmployeeAssignmentInput,
        audit_account_id: Uuid,
    ) -> Result<EmployeeAssignment, HrError>;
    async fn list_attendance_sessions(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<AttendanceSession>, HrError>;
    async fn check_in(
        &self,
        tenant_id: Uuid,
        attendance_session_id: Uuid,
        employee_id: Uuid,
        account_id: Uuid,
        facility_id: Uuid,
    ) -> Result<AttendanceSession, HrError>;
    async fn check_out(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        account_id: Uuid,
    ) -> Result<AttendanceSession, HrError>;
}

pub type DynPeopleRepo = Arc<dyn PeopleRepo + Send + Sync>;

pub struct PeopleService {
    repo: DynPeopleRepo,
}

impl PeopleService {
    pub fn new_arc(repo: DynPeopleRepo) -> Arc<Self> {
        Arc::new(Self { repo })
    }

    pub async fn list_employees(&self, tenant_id: Uuid) -> Result<Vec<Employee>, HrError> {
        self.repo.list_employees(tenant_id).await
    }

    pub async fn find_employee(&self, tenant_id: Uuid, employee_id: Uuid) -> Result<Option<Employee>, HrError> {
        self.repo.find_employee(tenant_id, employee_id).await
    }

    pub async fn find_employee_by_account(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
    ) -> Result<Option<Employee>, HrError> {
        self.repo.find_employee_by_account(tenant_id, account_id).await
    }

    pub async fn create_employee(
        &self,
        tenant_id: Uuid,
        input: EmployeeInput,
        audit_account_id: Uuid,
    ) -> Result<Employee, HrError> {
        validate_employee(&input)?;
        self.repo
            .create_employee(tenant_id, Uuid::new_v4(), &input, audit_account_id)
            .await
    }

    pub async fn update_employee(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        input: EmployeeInput,
        audit_account_id: Uuid,
    ) -> Result<Employee, HrError> {
        validate_employee(&input)?;
        self.repo
            .update_employee(tenant_id, employee_id, &input, audit_account_id)
            .await
    }

    pub async fn list_departments(&self, tenant_id: Uuid) -> Result<Vec<Department>, HrError> {
        self.repo.list_departments(tenant_id).await
    }

    pub async fn create_department(
        &self,
        tenant_id: Uuid,
        input: DepartmentInput,
        audit_account_id: Uuid,
    ) -> Result<Department, HrError> {
        validate_code_and_name(&input.code, &input.name)?;
        self.repo
            .create_department(tenant_id, Uuid::new_v4(), &input, audit_account_id)
            .await
    }

    pub async fn update_department(
        &self,
        tenant_id: Uuid,
        department_id: Uuid,
        input: DepartmentInput,
        audit_account_id: Uuid,
    ) -> Result<Department, HrError> {
        validate_code_and_name(&input.code, &input.name)?;
        if input.parent_department_id == Some(department_id) {
            return Err(HrError::InvalidInput("a department cannot be its own parent"));
        }
        self.repo
            .update_department(tenant_id, department_id, &input, audit_account_id)
            .await
    }

    pub async fn list_jobs(&self, tenant_id: Uuid) -> Result<Vec<JobPosition>, HrError> {
        self.repo.list_jobs(tenant_id).await
    }

    pub async fn create_job(
        &self,
        tenant_id: Uuid,
        input: JobPositionInput,
        audit_account_id: Uuid,
    ) -> Result<JobPosition, HrError> {
        validate_code_and_name(&input.code, &input.name)?;
        self.repo
            .create_job(tenant_id, Uuid::new_v4(), &input, audit_account_id)
            .await
    }

    pub async fn update_job(
        &self,
        tenant_id: Uuid,
        job_id: Uuid,
        input: JobPositionInput,
        audit_account_id: Uuid,
    ) -> Result<JobPosition, HrError> {
        validate_code_and_name(&input.code, &input.name)?;
        self.repo.update_job(tenant_id, job_id, &input, audit_account_id).await
    }

    pub async fn list_assignments(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<EmployeeAssignment>, HrError> {
        self.repo.list_assignments(tenant_id, employee_id).await
    }

    pub async fn create_assignment(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        input: EmployeeAssignmentInput,
        audit_account_id: Uuid,
    ) -> Result<EmployeeAssignment, HrError> {
        if input.date_end.is_some_and(|date_end| date_end < input.date_start) {
            return Err(HrError::InvalidInput("assignment end date precedes start date"));
        }
        if input.manager_employee_id == Some(employee_id) {
            return Err(HrError::InvalidInput("an employee cannot manage themself"));
        }
        self.repo
            .create_assignment(tenant_id, Uuid::new_v4(), employee_id, &input, audit_account_id)
            .await
    }

    pub async fn list_attendance_sessions(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<AttendanceSession>, HrError> {
        self.repo.list_attendance_sessions(tenant_id, employee_id).await
    }

    pub async fn check_in(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        account_id: Uuid,
        facility_id: Uuid,
    ) -> Result<AttendanceSession, HrError> {
        self.repo
            .check_in(tenant_id, Uuid::new_v4(), employee_id, account_id, facility_id)
            .await
    }

    pub async fn check_out(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        account_id: Uuid,
    ) -> Result<AttendanceSession, HrError> {
        self.repo.check_out(tenant_id, employee_id, account_id).await
    }
}

fn validate_employee(input: &EmployeeInput) -> Result<(), HrError> {
    validate_code_and_name(&input.employee_code, &input.display_name)?;
    match input.status {
        EmployeeStatus::Terminated
            if input
                .termination_date
                .is_none_or(|termination_date| termination_date < input.hire_date) =>
        {
            Err(HrError::InvalidInput(
                "terminated employees require a termination date on or after the hire date",
            ))
        }
        EmployeeStatus::Active | EmployeeStatus::OnLeave if input.termination_date.is_some() => Err(
            HrError::InvalidInput("only terminated employees may have a termination date"),
        ),
        _ => Ok(()),
    }
}

pub(crate) fn validate_code_and_name(code: &str, name: &str) -> Result<(), HrError> {
    let valid_code: bool = (2..=63).contains(&code.len())
        && code == code.trim()
        && code
            .bytes()
            .all(|byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-')
        && code
            .as_bytes()
            .first()
            .is_some_and(|byte: &u8| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && code
            .as_bytes()
            .last()
            .is_some_and(|byte: &u8| byte.is_ascii_lowercase() || byte.is_ascii_digit());
    if !valid_code {
        return Err(HrError::InvalidInput("code format is invalid"));
    }
    if !(1..=200).contains(&name.len()) || name != name.trim() {
        return Err(HrError::InvalidInput("name format is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{EmployeeInput, EmployeeStatus, HrError, validate_employee};

    #[test]
    fn terminated_employee_requires_a_valid_termination_date() {
        let input = EmployeeInput {
            account_id: None,
            employee_code: "emp-001".to_owned(),
            display_name: "Employee One".to_owned(),
            work_email: None,
            work_phone: None,
            badge_id: None,
            status: EmployeeStatus::Terminated,
            hire_date: NaiveDate::from_ymd_opt(2026, 1, 2).expect("valid test date"),
            termination_date: None,
        };

        assert!(matches!(validate_employee(&input), Err(HrError::InvalidInput(_))));
    }

    #[test]
    fn active_employee_cannot_have_a_termination_date() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 2).expect("valid test date");
        let input = EmployeeInput {
            account_id: None,
            employee_code: "emp-001".to_owned(),
            display_name: "Employee One".to_owned(),
            work_email: None,
            work_phone: None,
            badge_id: None,
            status: EmployeeStatus::Active,
            hire_date: date,
            termination_date: Some(date),
        };

        assert!(matches!(validate_employee(&input), Err(HrError::InvalidInput(_))));
    }
}
