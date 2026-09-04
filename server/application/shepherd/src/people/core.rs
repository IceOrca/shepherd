use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use tracing::{debug, error, info, trace, warn};
use super::database::PeopleRepo;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum EmployeeStatus {
    Active,
    OnLeave,
    Terminated,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Gender {
    Female,
    Male,
    Other,
    Unspecified,
}

impl Gender {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Female => "female",
            Self::Male => "male",
            Self::Other => "other",
            Self::Unspecified => "unspecified",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "female" => Some(Self::Female),
            "male" => Some(Self::Male),
            "other" => Some(Self::Other),
            "unspecified" => Some(Self::Unspecified),
            _ => None,
        }
    }
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

#[derive(Clone, Debug, Serialize, TS)]
pub struct Employee {
    pub id: Uuid,
    pub branch_id: Uuid,
    pub account_id: Option<Uuid>,
    pub employee_code: String,
    pub display_name: String,
    pub legal_first_name: Option<String>,
    pub legal_middle_name: Option<String>,
    pub legal_last_name: Option<String>,
    pub personal_phone_e164: Option<String>,
    pub gender: Option<Gender>,
    pub citizen_id_country_code: Option<String>,
    pub citizen_id_last4: Option<String>,
    pub profile_complete: bool,
    pub status: EmployeeStatus,
    pub hire_date: NaiveDate,
    pub termination_date: Option<NaiveDate>,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct EmployeeSensitiveProfile {
    pub employee_id: Uuid,
    pub citizen_id_country_code: Option<String>,
    pub citizen_id: Option<String>,
    pub version: i64,
}

/// One contiguous employee work session. A workday can contain multiple
/// completed sessions, for example before and after an unpaid break.
#[derive(Clone, Debug, Serialize, TS)]
pub struct AttendanceSession {
    pub id: Uuid,
    pub employee_id: Uuid,
    pub branch_id: Uuid,
    pub check_in_at: DateTime<Utc>,
    pub check_out_at: Option<DateTime<Utc>>,
    pub worked_seconds: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EmployeeCursor {
    pub normalized_display_name: String,
    pub employee_code: String,
    pub employee_id: Uuid,
}

#[derive(Clone, Debug)]
pub struct EmployeePage {
    pub items: Vec<Employee>,
    pub next_cursor: Option<EmployeeCursor>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AttendanceCursor {
    pub check_in_at: DateTime<Utc>,
    pub attendance_session_id: Uuid,
}

#[derive(Clone, Debug)]
pub struct AttendancePage {
    pub items: Vec<AttendanceSession>,
    pub next_cursor: Option<AttendanceCursor>,
}

#[derive(Clone, Debug)]
pub struct EmployeeInput {
    pub account_id: Option<Uuid>,
    pub employee_code: String,
    pub display_name: String,
    pub legal_first_name: Option<String>,
    pub legal_middle_name: Option<String>,
    pub legal_last_name: Option<String>,
    pub personal_phone_e164: Option<String>,
    pub gender: Option<Gender>,
    pub status: EmployeeStatus,
    pub hire_date: NaiveDate,
    pub termination_date: Option<NaiveDate>,
    pub expected_version: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct EmployeeCitizenIdInput {
    pub citizen_id_country_code: Option<String>,
    pub citizen_id: Option<String>,
    pub expected_version: i64,
}

#[derive(Debug)]
pub enum PeopleOpsErr {
    NotFound,
    Conflict,
    InvalidInput(&'static str),
    BackendUnavailable,
}

pub struct PeopleService {
    repo: Arc<PeopleRepo>,
}

impl PeopleService {
    pub fn new_arc(repo: Arc<PeopleRepo>) -> Arc<Self> {
        Arc::new(Self { repo })
    }

    pub async fn list_employees(
        &self,
        tenant_id: Uuid,
        search: Option<String>,
        limit: i64,
        cursor: Option<EmployeeCursor>,
    ) -> Result<EmployeePage, PeopleOpsErr> {
        if limit <= 0 {
            return Err(PeopleOpsErr::InvalidInput("employee page size must be positive"));
        }
        self.repo
            .list_employees(tenant_id, search.as_deref(), limit, cursor.as_ref())
            .await
    }

    pub async fn find_employee(&self, tenant_id: Uuid, employee_id: Uuid) -> Result<Option<Employee>, PeopleOpsErr> {
        self.repo.find_employee(tenant_id, employee_id).await
    }

    pub async fn find_employee_by_account(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
    ) -> Result<Option<Employee>, PeopleOpsErr> {
        self.repo.find_employee_by_account(tenant_id, account_id).await
    }

    pub async fn create_employee(
        &self,
        tenant_id: Uuid,
        branch_id: Uuid,
        input: EmployeeInput,
        audit_account_id: Uuid,
    ) -> Result<Employee, PeopleOpsErr> {
        validate_employee(&input)?;
        self.repo
            .create_employee(tenant_id, branch_id, Uuid::new_v4(), &input, audit_account_id)
            .await
    }

    pub async fn update_employee(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        input: EmployeeInput,
        audit_account_id: Uuid,
    ) -> Result<Employee, PeopleOpsErr> {
        validate_employee(&input)?;
        if input.expected_version.is_none_or(|version: i64| version < 1) {
            return Err(PeopleOpsErr::InvalidInput(
                "employee update requires a positive expected version",
            ));
        }
        self.repo
            .update_employee(tenant_id, employee_id, &input, audit_account_id)
            .await
    }

    pub async fn find_employee_sensitive_profile(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Option<EmployeeSensitiveProfile>, PeopleOpsErr> {
        self.repo.find_employee_sensitive_profile(tenant_id, employee_id).await
    }

    pub async fn find_employee_sensitive_profile_by_account(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
    ) -> Result<Option<EmployeeSensitiveProfile>, PeopleOpsErr> {
        self.repo
            .find_employee_sensitive_profile_by_account(tenant_id, account_id)
            .await
    }

    pub async fn update_employee_citizen_id(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        input: EmployeeCitizenIdInput,
        audit_account_id: Uuid,
    ) -> Result<EmployeeSensitiveProfile, PeopleOpsErr> {
        validate_citizen_id_input(&input)?;
        self.repo
            .update_employee_citizen_id(tenant_id, employee_id, &input, audit_account_id)
            .await
    }

    pub async fn list_attendance_sessions(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        limit: i64,
        cursor: Option<AttendanceCursor>,
    ) -> Result<AttendancePage, PeopleOpsErr> {
        if limit <= 0 {
            return Err(PeopleOpsErr::InvalidInput("attendance page size must be positive"));
        }
        self.repo
            .list_attendance_sessions(tenant_id, employee_id, limit, cursor.as_ref())
            .await
    }

    pub async fn check_in(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        account_id: Uuid,
        branch_id: Uuid,
    ) -> Result<AttendanceSession, PeopleOpsErr> {
        self.repo
            .check_in(tenant_id, Uuid::new_v4(), employee_id, account_id, branch_id)
            .await
    }

    pub async fn check_out(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        account_id: Uuid,
    ) -> Result<AttendanceSession, PeopleOpsErr> {
        self.repo.check_out(tenant_id, employee_id, account_id).await
    }
}

fn validate_employee(input: &EmployeeInput) -> Result<(), PeopleOpsErr> {
    validate_code_and_name(&input.employee_code, &input.display_name)?;
    match (
        &input.legal_first_name,
        &input.legal_middle_name,
        &input.legal_last_name,
    ) {
        (None, None, None) => {}
        (Some(first_name), middle_name, Some(last_name))
            if valid_person_name(first_name)
                && valid_person_name(last_name)
                && middle_name.as_ref().is_none_or(|name: &String| valid_person_name(name)) => {}
        _ => {
            return Err(PeopleOpsErr::InvalidInput(
                "legal first and last names must be supplied together",
            ));
        }
    }
    if input.personal_phone_e164.as_ref().is_some_and(|phone: &String| {
        !(8..=16).contains(&phone.len())
            || !phone.starts_with('+')
            || !phone.bytes().skip(1).all(|byte: u8| byte.is_ascii_digit())
            || phone.as_bytes().get(1) == Some(&b'0')
    }) {
        return Err(PeopleOpsErr::InvalidInput("personal phone must use E.164 format"));
    }
    match input.status {
        EmployeeStatus::Terminated
            if input
                .termination_date
                .is_none_or(|termination_date| termination_date < input.hire_date) =>
        {
            Err(PeopleOpsErr::InvalidInput(
                "terminated employees require a termination date on or after the hire date",
            ))
        }
        EmployeeStatus::Active | EmployeeStatus::OnLeave if input.termination_date.is_some() => Err(
            PeopleOpsErr::InvalidInput("only terminated employees may have a termination date"),
        ),
        _ => Ok(()),
    }
}

fn validate_citizen_id_input(input: &EmployeeCitizenIdInput) -> Result<(), PeopleOpsErr> {
    if input.expected_version < 1 {
        return Err(PeopleOpsErr::InvalidInput(
            "citizen ID update requires a positive expected version",
        ));
    }
    match (&input.citizen_id_country_code, &input.citizen_id) {
        (None, None) => Ok(()),
        (Some(country_code), Some(citizen_id))
            if country_code.len() == 2
                && country_code.bytes().all(|byte: u8| byte.is_ascii_uppercase())
                && valid_citizen_id(country_code, citizen_id) =>
        {
            Ok(())
        }
        _ => Err(PeopleOpsErr::InvalidInput(
            "citizen ID and two-letter country code must be supplied or cleared together",
        )),
    }
}

fn valid_citizen_id(country_code: &str, citizen_id: &str) -> bool {
    if country_code == "VN" {
        citizen_id.len() == 12 && citizen_id.bytes().all(|byte: u8| byte.is_ascii_digit())
    } else {
        (4..=32).contains(&citizen_id.len())
            && citizen_id
                .bytes()
                .all(|byte: u8| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    }
}

fn valid_person_name(value: &str) -> bool {
    (1..=100).contains(&value.chars().count()) && value == value.trim()
}

pub(crate) fn validate_code_and_name(code: &str, name: &str) -> Result<(), PeopleOpsErr> {
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
        return Err(PeopleOpsErr::InvalidInput("code format is invalid"));
    }
    if !(1..=200).contains(&name.len()) || name != name.trim() {
        return Err(PeopleOpsErr::InvalidInput("name format is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{
        EmployeeCitizenIdInput, EmployeeInput, EmployeeStatus, Gender, PeopleOpsErr, validate_citizen_id_input,
        validate_employee,
    };

    fn active_employee() -> EmployeeInput {
        EmployeeInput {
            account_id: None,
            employee_code: "emp-001".to_owned(),
            display_name: "Nguyễn Văn An".to_owned(),
            legal_first_name: Some("An".to_owned()),
            legal_middle_name: Some("Văn".to_owned()),
            legal_last_name: Some("Nguyễn".to_owned()),
            personal_phone_e164: Some("+84901234567".to_owned()),
            gender: Some(Gender::Male),
            status: EmployeeStatus::Active,
            hire_date: NaiveDate::from_ymd_opt(2026, 1, 2).expect("valid test date"),
            termination_date: None,
            expected_version: None,
        }
    }

    #[test]
    fn terminated_employee_requires_a_valid_termination_date() {
        let mut input: EmployeeInput = active_employee();
        input.status = EmployeeStatus::Terminated;

        assert!(matches!(validate_employee(&input), Err(PeopleOpsErr::InvalidInput(_))));
    }

    #[test]
    fn active_employee_cannot_have_a_termination_date() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 2).expect("valid test date");
        let mut input: EmployeeInput = active_employee();
        input.termination_date = Some(date);

        assert!(matches!(validate_employee(&input), Err(PeopleOpsErr::InvalidInput(_))));
    }

    #[test]
    fn legal_names_are_all_absent_or_have_first_and_last_components() {
        let mut input: EmployeeInput = active_employee();
        assert!(validate_employee(&input).is_ok());
        input.legal_last_name = None;
        assert!(matches!(validate_employee(&input), Err(PeopleOpsErr::InvalidInput(_))));
    }

    #[test]
    fn vietnamese_citizen_id_requires_twelve_digits() {
        let valid = EmployeeCitizenIdInput {
            citizen_id_country_code: Some("VN".to_owned()),
            citizen_id: Some("012345678901".to_owned()),
            expected_version: 1,
        };
        assert!(validate_citizen_id_input(&valid).is_ok());
        let invalid = EmployeeCitizenIdInput {
            citizen_id: Some("0123-456-789".to_owned()),
            ..valid
        };
        assert!(matches!(
            validate_citizen_id_input(&invalid),
            Err(PeopleOpsErr::InvalidInput(_))
        ));
    }
}
