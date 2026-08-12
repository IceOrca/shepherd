use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum BusinessRecordStatus {
    Active,
    Disabled,
}

impl BusinessRecordStatus {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "active" => Some(Self::Active),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum StaffingShiftStatus {
    Open,
    Filled,
    InProgress,
    Completed,
    Cancelled,
}

impl StaffingShiftStatus {
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "open" => Some(Self::Open),
            "filled" => Some(Self::Filled),
            "in_progress" => Some(Self::InProgress),
            "completed" => Some(Self::Completed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ShiftAssignmentStatus {
    Assigned,
    Approved,
    Cancelled,
}

impl ShiftAssignmentStatus {
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "assigned" => Some(Self::Assigned),
            "approved" => Some(Self::Approved),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum RateSource {
    Agreement,
    Manual,
}

impl RateSource {
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "agreement" => Some(Self::Agreement),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct Customer {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub billing_email: Option<String>,
    pub status: BusinessRecordStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct CustomerFacility {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub code: String,
    pub name: String,
    pub address: Option<String>,
    pub time_zone: String,
    pub status: BusinessRecordStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct StaffingRateAgreement {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub customer_id: Uuid,
    pub customer_facility_id: Option<Uuid>,
    pub employee_id: Option<Uuid>,
    pub job_id: Uuid,
    pub currency: String,
    pub bill_hourly_rate: String,
    pub worker_hourly_rate: String,
    pub priority: i16,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct StaffingShift {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub customer_facility_id: Uuid,
    pub job_id: Uuid,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub required_workers: i32,
    pub status: StaffingShiftStatus,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct ShiftAssignment {
    pub id: Uuid,
    pub shift_id: Uuid,
    pub employee_id: Uuid,
    pub rate_agreement_id: Option<Uuid>,
    pub rate_source: RateSource,
    pub currency: String,
    pub bill_hourly_rate_snapshot: String,
    pub worker_hourly_rate_snapshot: String,
    pub status: ShiftAssignmentStatus,
    pub worked_seconds: Option<i64>,
    pub observed_worked_seconds: Option<i64>,
    pub approval_adjustment_reason: Option<String>,
    pub customer_amount: Option<String>,
    pub worker_amount: Option<String>,
    pub margin_amount: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct CustomerInput {
    pub code: String,
    pub name: String,
    pub billing_email: Option<String>,
    pub status: BusinessRecordStatus,
}

#[derive(Clone, Debug)]
pub struct CustomerFacilityInput {
    pub code: String,
    pub name: String,
    pub address: Option<String>,
    pub time_zone: String,
    pub status: BusinessRecordStatus,
}

#[derive(Clone, Debug)]
pub struct StaffingRateAgreementInput {
    pub code: String,
    pub name: String,
    pub customer_id: Uuid,
    pub customer_facility_id: Option<Uuid>,
    pub employee_id: Option<Uuid>,
    pub job_id: Uuid,
    pub currency: String,
    pub bill_hourly_rate: String,
    pub worker_hourly_rate: String,
    pub priority: i16,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub is_active: bool,
}

#[derive(Clone, Debug)]
pub struct StaffingShiftInput {
    pub customer_id: Uuid,
    pub customer_facility_id: Uuid,
    pub job_id: Uuid,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub required_workers: i32,
    pub notes: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ManualRateOverride {
    pub currency: String,
    pub bill_hourly_rate: String,
    pub worker_hourly_rate: String,
}

#[derive(Clone, Debug)]
pub struct ShiftAssignmentInput {
    pub employee_id: Uuid,
    pub manual_rate: Option<ManualRateOverride>,
}

#[derive(Debug)]
pub enum StaffingError {
    NotFound,
    Conflict,
    InvalidInput(&'static str),
    MissingRateAgreement,
    BackendUnavailable,
}

#[async_trait]
pub trait StaffingRepo {
    async fn list_customers(&self, tenant_id: Uuid) -> Result<Vec<Customer>, StaffingError>;
    async fn create_customer(
        &self,
        tenant_id: Uuid,
        customer_id: Uuid,
        input: &CustomerInput,
        audit_account_id: Uuid,
    ) -> Result<Customer, StaffingError>;
    async fn list_customer_facilities(
        &self,
        tenant_id: Uuid,
        customer_id: Uuid,
    ) -> Result<Vec<CustomerFacility>, StaffingError>;
    async fn create_customer_facility(
        &self,
        tenant_id: Uuid,
        facility_id: Uuid,
        customer_id: Uuid,
        input: &CustomerFacilityInput,
        audit_account_id: Uuid,
    ) -> Result<CustomerFacility, StaffingError>;
    async fn list_rate_agreements(&self, tenant_id: Uuid) -> Result<Vec<StaffingRateAgreement>, StaffingError>;
    async fn create_rate_agreement(
        &self,
        tenant_id: Uuid,
        agreement_id: Uuid,
        input: &StaffingRateAgreementInput,
        audit_account_id: Uuid,
    ) -> Result<StaffingRateAgreement, StaffingError>;
    async fn list_shifts(&self, tenant_id: Uuid) -> Result<Vec<StaffingShift>, StaffingError>;
    async fn create_shift(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
        input: &StaffingShiftInput,
        audit_account_id: Uuid,
    ) -> Result<StaffingShift, StaffingError>;
    async fn list_shift_assignments(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
    ) -> Result<Vec<ShiftAssignment>, StaffingError>;
    async fn create_shift_assignment(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        shift_id: Uuid,
        input: &ShiftAssignmentInput,
        audit_account_id: Uuid,
    ) -> Result<ShiftAssignment, StaffingError>;
    async fn approve_shift_assignment(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        worked_seconds: Option<i64>,
        adjustment_reason: Option<String>,
        audit_account_id: Uuid,
    ) -> Result<ShiftAssignment, StaffingError>;
}

pub type DynStaffingRepo = Arc<dyn StaffingRepo + Send + Sync>;

pub struct StaffingService {
    repo: DynStaffingRepo,
}

impl StaffingService {
    pub fn new_arc(repo: DynStaffingRepo) -> Arc<Self> {
        Arc::new(Self { repo })
    }

    pub async fn list_customers(&self, tenant_id: Uuid) -> Result<Vec<Customer>, StaffingError> {
        self.repo.list_customers(tenant_id).await
    }

    pub async fn create_customer(
        &self,
        tenant_id: Uuid,
        input: CustomerInput,
        audit_account_id: Uuid,
    ) -> Result<Customer, StaffingError> {
        validate_identity(&input.code, &input.name)?;
        self.repo
            .create_customer(tenant_id, Uuid::new_v4(), &input, audit_account_id)
            .await
    }

    pub async fn list_customer_facilities(
        &self,
        tenant_id: Uuid,
        customer_id: Uuid,
    ) -> Result<Vec<CustomerFacility>, StaffingError> {
        self.repo.list_customer_facilities(tenant_id, customer_id).await
    }

    pub async fn create_customer_facility(
        &self,
        tenant_id: Uuid,
        customer_id: Uuid,
        input: CustomerFacilityInput,
        audit_account_id: Uuid,
    ) -> Result<CustomerFacility, StaffingError> {
        validate_identity(&input.code, &input.name)?;
        if input.time_zone.is_empty() || input.time_zone.len() > 128 {
            return Err(StaffingError::InvalidInput("customer facility time zone is invalid"));
        }
        self.repo
            .create_customer_facility(tenant_id, Uuid::new_v4(), customer_id, &input, audit_account_id)
            .await
    }

    pub async fn list_rate_agreements(&self, tenant_id: Uuid) -> Result<Vec<StaffingRateAgreement>, StaffingError> {
        self.repo.list_rate_agreements(tenant_id).await
    }

    pub async fn create_rate_agreement(
        &self,
        tenant_id: Uuid,
        input: StaffingRateAgreementInput,
        audit_account_id: Uuid,
    ) -> Result<StaffingRateAgreement, StaffingError> {
        validate_identity(&input.code, &input.name)?;
        validate_currency(&input.currency)?;
        validate_positive_decimal(&input.bill_hourly_rate)?;
        validate_positive_decimal(&input.worker_hourly_rate)?;
        if input.effective_to.is_some_and(|date| date < input.effective_from) {
            return Err(StaffingError::InvalidInput("rate agreement date range is invalid"));
        }
        self.repo
            .create_rate_agreement(tenant_id, Uuid::new_v4(), &input, audit_account_id)
            .await
    }

    pub async fn list_shifts(&self, tenant_id: Uuid) -> Result<Vec<StaffingShift>, StaffingError> {
        self.repo.list_shifts(tenant_id).await
    }

    pub async fn create_shift(
        &self,
        tenant_id: Uuid,
        input: StaffingShiftInput,
        audit_account_id: Uuid,
    ) -> Result<StaffingShift, StaffingError> {
        if input.ends_at <= input.starts_at || input.required_workers <= 0 {
            return Err(StaffingError::InvalidInput("staffing shift schedule is invalid"));
        }
        self.repo
            .create_shift(tenant_id, Uuid::new_v4(), &input, audit_account_id)
            .await
    }

    pub async fn list_shift_assignments(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
    ) -> Result<Vec<ShiftAssignment>, StaffingError> {
        self.repo.list_shift_assignments(tenant_id, shift_id).await
    }

    pub async fn create_shift_assignment(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
        input: ShiftAssignmentInput,
        audit_account_id: Uuid,
    ) -> Result<ShiftAssignment, StaffingError> {
        if let Some(manual_rate) = &input.manual_rate {
            validate_currency(&manual_rate.currency)?;
            validate_positive_decimal(&manual_rate.bill_hourly_rate)?;
            validate_positive_decimal(&manual_rate.worker_hourly_rate)?;
        }
        self.repo
            .create_shift_assignment(tenant_id, Uuid::new_v4(), shift_id, &input, audit_account_id)
            .await
    }

    pub async fn approve_shift_assignment(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        worked_seconds: Option<i64>,
        adjustment_reason: Option<String>,
        audit_account_id: Uuid,
    ) -> Result<ShiftAssignment, StaffingError> {
        validate_approval_input(worked_seconds, adjustment_reason.as_deref())?;
        self.repo
            .approve_shift_assignment(
                tenant_id,
                assignment_id,
                worked_seconds,
                adjustment_reason,
                audit_account_id,
            )
            .await
    }
}

fn validate_approval_input(worked_seconds: Option<i64>, adjustment_reason: Option<&str>) -> Result<(), StaffingError> {
    if worked_seconds.is_some_and(|seconds| seconds <= 0) {
        return Err(StaffingError::InvalidInput("worked seconds must be positive"));
    }
    if adjustment_reason.is_some_and(|reason| reason.len() < 3 || reason.len() > 500 || reason != reason.trim()) {
        return Err(StaffingError::InvalidInput("approval adjustment reason is invalid"));
    }
    Ok(())
}

fn validate_identity(code: &str, name: &str) -> Result<(), StaffingError> {
    let valid_boundary = code
        .chars()
        .next()
        .zip(code.chars().last())
        .is_some_and(|(first, last)| first.is_ascii_alphanumeric() && last.is_ascii_alphanumeric());
    if code.len() < 2
        || code.len() > 63
        || !valid_boundary
        || code.chars().any(|character| {
            !(character.is_ascii_lowercase() || character.is_ascii_digit() || "_-".contains(character))
        })
    {
        return Err(StaffingError::InvalidInput("business code is invalid"));
    }
    if name.is_empty() || name.len() > 200 || name != name.trim() {
        return Err(StaffingError::InvalidInput("business name is invalid"));
    }
    Ok(())
}

fn validate_currency(currency: &str) -> Result<(), StaffingError> {
    if currency.len() != 3 || currency.chars().any(|character| !character.is_ascii_uppercase()) {
        return Err(StaffingError::InvalidInput(
            "currency must be a three-letter uppercase code",
        ));
    }
    Ok(())
}

fn validate_positive_decimal(value: &str) -> Result<(), StaffingError> {
    let mut parts = value.split('.');
    let whole = parts
        .next()
        .ok_or(StaffingError::InvalidInput("hourly rate is invalid"))?;
    let fraction = parts.next();
    let is_zero = value.chars().all(|character| character == '0' || character == '.');
    if value.is_empty()
        || value != value.trim()
        || value.starts_with('-')
        || parts.next().is_some()
        || whole.is_empty()
        || !whole.chars().all(|character| character.is_ascii_digit())
        || fraction.is_some_and(|part| {
            part.is_empty() || part.len() > 4 || !part.chars().all(|character| character.is_ascii_digit())
        })
        || is_zero
    {
        return Err(StaffingError::InvalidInput("hourly rate is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_approval_input, validate_identity, validate_positive_decimal};

    #[test]
    fn validates_financial_rate_without_using_floating_point() {
        assert!(validate_positive_decimal("180000.0000").is_ok());
        assert!(validate_positive_decimal("0").is_err());
        assert!(validate_positive_decimal("12.12345").is_err());
    }

    #[test]
    fn validates_normalized_business_codes() {
        assert!(validate_identity("karaoke-a", "Karaoke A").is_ok());
        assert!(validate_identity("-karaoke", "Karaoke A").is_err());
    }

    #[test]
    fn approval_accepts_observed_time_or_a_normalized_adjustment() {
        assert!(validate_approval_input(None, None).is_ok());
        assert!(validate_approval_input(Some(3600), None).is_ok());
        assert!(validate_approval_input(Some(3900), Some("Customer confirmed extra setup time")).is_ok());
    }

    #[test]
    fn approval_rejects_non_positive_time() {
        assert!(validate_approval_input(Some(0), None).is_err());
        assert!(validate_approval_input(Some(-1), Some("Correction")).is_err());
    }

    #[test]
    fn approval_rejects_invalid_adjustment_reasons() {
        assert!(validate_approval_input(Some(3900), Some("no")).is_err());
        assert!(validate_approval_input(Some(3900), Some(" padded reason ")).is_err());
        assert!(validate_approval_input(Some(3900), Some(&"x".repeat(501))).is_err());
    }
}
