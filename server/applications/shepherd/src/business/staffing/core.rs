use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use tracing::{debug, error, info, trace, warn};
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

#[derive(Clone, Debug, Serialize, TS)]
pub struct StaffingCandidate {
    pub employee_id: Uuid,
    pub employee_code: String,
    pub display_name: String,
    pub suitable: bool,
    pub available: bool,
    pub already_assigned: bool,
    pub conflict_shift_id: Option<Uuid>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationStatus {
    PendingStaff,
    PendingCustomer,
    Matched,
    Discrepancy,
    Reconciled,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct CustomerWorkRecord {
    pub id: Uuid,
    pub assignment_id: Uuid,
    pub confirmed_started_at: DateTime<Utc>,
    pub confirmed_ended_at: DateTime<Utc>,
    pub confirmed_worked_seconds: i64,
    pub customer_reference: Option<String>,
    pub notes: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct StaffingReconciliation {
    pub assignment_id: Uuid,
    pub shift_id: Uuid,
    pub employee_id: Uuid,
    pub employee_code: String,
    pub employee_name: String,
    pub customer_name: String,
    pub customer_facility_name: String,
    pub scheduled_starts_at: DateTime<Utc>,
    pub scheduled_ends_at: DateTime<Utc>,
    pub assignment_status: ShiftAssignmentStatus,
    pub staff_started_at: Option<DateTime<Utc>>,
    pub staff_ended_at: Option<DateTime<Utc>>,
    pub staff_worked_seconds: i64,
    pub customer_record: Option<CustomerWorkRecord>,
    pub final_worked_seconds: Option<i64>,
    pub adjustment_reason: Option<String>,
    pub reconciliation_status: ReconciliationStatus,
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

#[derive(Clone, Debug)]
pub struct CustomerWorkRecordInput {
    pub confirmed_started_at: DateTime<Utc>,
    pub confirmed_ended_at: DateTime<Utc>,
    pub customer_reference: Option<String>,
    pub notes: Option<String>,
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
    async fn list_shift_candidates(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
    ) -> Result<Vec<StaffingCandidate>, StaffingError>;
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
    async fn list_reconciliations(&self, tenant_id: Uuid) -> Result<Vec<StaffingReconciliation>, StaffingError>;
    async fn upsert_customer_work_record(
        &self,
        tenant_id: Uuid,
        record_id: Uuid,
        assignment_id: Uuid,
        input: &CustomerWorkRecordInput,
        audit_account_id: Uuid,
    ) -> Result<CustomerWorkRecord, StaffingError>;
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
        debug!(operation = "list_customers", tenant_id = %tenant_id, "Staffing service operation accepted");
        let result: Result<Vec<Customer>, StaffingError> = self.repo.list_customers(tenant_id).await;
        log_staffing_operation("list_customers", tenant_id, None, None, &result);
        result
    }

    pub async fn create_customer(
        &self,
        tenant_id: Uuid,
        input: CustomerInput,
        audit_account_id: Uuid,
    ) -> Result<Customer, StaffingError> {
        let customer_id: Uuid = Uuid::new_v4();
        trace!(
            operation = "create_customer",
            tenant_id = %tenant_id,
            audit_account_id = %audit_account_id,
            customer_id = %customer_id,
            "Validating staffing customer creation"
        );
        validate_identity(&input.code, &input.name)?;
        let result: Result<Customer, StaffingError> = self
            .repo
            .create_customer(tenant_id, customer_id, &input, audit_account_id)
            .await;
        log_staffing_operation(
            "create_customer",
            tenant_id,
            Some(audit_account_id),
            Some(customer_id),
            &result,
        );
        result
    }

    pub async fn list_customer_facilities(
        &self,
        tenant_id: Uuid,
        customer_id: Uuid,
    ) -> Result<Vec<CustomerFacility>, StaffingError> {
        debug!(
            operation = "list_customer_facilities",
            tenant_id = %tenant_id,
            customer_id = %customer_id,
            "Staffing service operation accepted"
        );
        let result: Result<Vec<CustomerFacility>, StaffingError> =
            self.repo.list_customer_facilities(tenant_id, customer_id).await;
        log_staffing_operation("list_customer_facilities", tenant_id, None, Some(customer_id), &result);
        result
    }

    pub async fn create_customer_facility(
        &self,
        tenant_id: Uuid,
        customer_id: Uuid,
        input: CustomerFacilityInput,
        audit_account_id: Uuid,
    ) -> Result<CustomerFacility, StaffingError> {
        let facility_id: Uuid = Uuid::new_v4();
        trace!(
            operation = "create_customer_facility",
            tenant_id = %tenant_id,
            audit_account_id = %audit_account_id,
            customer_id = %customer_id,
            facility_id = %facility_id,
            "Validating staffing customer facility creation"
        );
        validate_identity(&input.code, &input.name)?;
        if input.time_zone.is_empty() || input.time_zone.len() > 128 {
            return Err(StaffingError::InvalidInput("customer facility time zone is invalid"));
        }
        let result: Result<CustomerFacility, StaffingError> = self
            .repo
            .create_customer_facility(tenant_id, facility_id, customer_id, &input, audit_account_id)
            .await;
        log_staffing_operation(
            "create_customer_facility",
            tenant_id,
            Some(audit_account_id),
            Some(facility_id),
            &result,
        );
        result
    }

    pub async fn list_rate_agreements(&self, tenant_id: Uuid) -> Result<Vec<StaffingRateAgreement>, StaffingError> {
        debug!(operation = "list_rate_agreements", tenant_id = %tenant_id, "Staffing service operation accepted");
        let result: Result<Vec<StaffingRateAgreement>, StaffingError> = self.repo.list_rate_agreements(tenant_id).await;
        log_staffing_operation("list_rate_agreements", tenant_id, None, None, &result);
        result
    }

    pub async fn create_rate_agreement(
        &self,
        tenant_id: Uuid,
        input: StaffingRateAgreementInput,
        audit_account_id: Uuid,
    ) -> Result<StaffingRateAgreement, StaffingError> {
        let agreement_id: Uuid = Uuid::new_v4();
        trace!(
            operation = "create_rate_agreement",
            tenant_id = %tenant_id,
            audit_account_id = %audit_account_id,
            agreement_id = %agreement_id,
            customer_id = %input.customer_id,
            customer_facility_id = ?input.customer_facility_id,
            employee_id = ?input.employee_id,
            job_id = %input.job_id,
            "Validating staffing rate agreement creation"
        );
        validate_identity(&input.code, &input.name)?;
        validate_currency(&input.currency)?;
        validate_positive_decimal(&input.bill_hourly_rate)?;
        validate_positive_decimal(&input.worker_hourly_rate)?;
        if input
            .effective_to
            .is_some_and(|date: NaiveDate| date < input.effective_from)
        {
            return Err(StaffingError::InvalidInput("rate agreement date range is invalid"));
        }
        let result: Result<StaffingRateAgreement, StaffingError> = self
            .repo
            .create_rate_agreement(tenant_id, agreement_id, &input, audit_account_id)
            .await;
        log_staffing_operation(
            "create_rate_agreement",
            tenant_id,
            Some(audit_account_id),
            Some(agreement_id),
            &result,
        );
        result
    }

    pub async fn list_shifts(&self, tenant_id: Uuid) -> Result<Vec<StaffingShift>, StaffingError> {
        debug!(operation = "list_staffing_shifts", tenant_id = %tenant_id, "Staffing service operation accepted");
        let result: Result<Vec<StaffingShift>, StaffingError> = self.repo.list_shifts(tenant_id).await;
        log_staffing_operation("list_staffing_shifts", tenant_id, None, None, &result);
        result
    }

    pub async fn create_shift(
        &self,
        tenant_id: Uuid,
        input: StaffingShiftInput,
        audit_account_id: Uuid,
    ) -> Result<StaffingShift, StaffingError> {
        let shift_id: Uuid = Uuid::new_v4();
        trace!(
            operation = "create_staffing_shift",
            tenant_id = %tenant_id,
            audit_account_id = %audit_account_id,
            shift_id = %shift_id,
            customer_id = %input.customer_id,
            customer_facility_id = %input.customer_facility_id,
            job_id = %input.job_id,
            required_workers = input.required_workers,
            "Validating staffing shift creation"
        );
        if input.ends_at <= input.starts_at || input.required_workers <= 0 {
            return Err(StaffingError::InvalidInput("staffing shift schedule is invalid"));
        }
        let result: Result<StaffingShift, StaffingError> = self
            .repo
            .create_shift(tenant_id, shift_id, &input, audit_account_id)
            .await;
        log_staffing_operation(
            "create_staffing_shift",
            tenant_id,
            Some(audit_account_id),
            Some(shift_id),
            &result,
        );
        result
    }

    pub async fn list_shift_assignments(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
    ) -> Result<Vec<ShiftAssignment>, StaffingError> {
        debug!(
            operation = "list_shift_assignments",
            tenant_id = %tenant_id,
            shift_id = %shift_id,
            "Staffing service operation accepted"
        );
        let result: Result<Vec<ShiftAssignment>, StaffingError> =
            self.repo.list_shift_assignments(tenant_id, shift_id).await;
        log_staffing_operation("list_shift_assignments", tenant_id, None, Some(shift_id), &result);
        result
    }

    pub async fn list_shift_candidates(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
    ) -> Result<Vec<StaffingCandidate>, StaffingError> {
        debug!(
            operation = "list_shift_candidates",
            tenant_id = %tenant_id,
            shift_id = %shift_id,
            "Staffing service operation accepted"
        );
        let result: Result<Vec<StaffingCandidate>, StaffingError> =
            self.repo.list_shift_candidates(tenant_id, shift_id).await;
        log_staffing_operation("list_shift_candidates", tenant_id, None, Some(shift_id), &result);
        result
    }

    pub async fn create_shift_assignment(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
        input: ShiftAssignmentInput,
        audit_account_id: Uuid,
    ) -> Result<ShiftAssignment, StaffingError> {
        let assignment_id: Uuid = Uuid::new_v4();
        trace!(
            operation = "create_shift_assignment",
            tenant_id = %tenant_id,
            audit_account_id = %audit_account_id,
            shift_id = %shift_id,
            assignment_id = %assignment_id,
            employee_id = %input.employee_id,
            has_manual_rate = input.manual_rate.is_some(),
            "Validating staffing shift assignment"
        );
        if let Some(manual_rate) = &input.manual_rate {
            validate_currency(&manual_rate.currency)?;
            validate_positive_decimal(&manual_rate.bill_hourly_rate)?;
            validate_positive_decimal(&manual_rate.worker_hourly_rate)?;
        }
        let result: Result<ShiftAssignment, StaffingError> = self
            .repo
            .create_shift_assignment(tenant_id, assignment_id, shift_id, &input, audit_account_id)
            .await;
        log_staffing_operation(
            "create_shift_assignment",
            tenant_id,
            Some(audit_account_id),
            Some(assignment_id),
            &result,
        );
        result
    }

    pub async fn approve_shift_assignment(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        worked_seconds: Option<i64>,
        adjustment_reason: Option<String>,
        audit_account_id: Uuid,
    ) -> Result<ShiftAssignment, StaffingError> {
        trace!(
            operation = "approve_shift_assignment",
            tenant_id = %tenant_id,
            audit_account_id = %audit_account_id,
            assignment_id = %assignment_id,
            has_worked_seconds_override = worked_seconds.is_some(),
            has_adjustment_reason = adjustment_reason.is_some(),
            "Validating staffing reconciliation approval"
        );
        validate_approval_input(worked_seconds, adjustment_reason.as_deref())?;
        let result: Result<ShiftAssignment, StaffingError> = self
            .repo
            .approve_shift_assignment(
                tenant_id,
                assignment_id,
                worked_seconds,
                adjustment_reason,
                audit_account_id,
            )
            .await;
        log_staffing_operation(
            "approve_shift_assignment",
            tenant_id,
            Some(audit_account_id),
            Some(assignment_id),
            &result,
        );
        result
    }

    pub async fn list_reconciliations(&self, tenant_id: Uuid) -> Result<Vec<StaffingReconciliation>, StaffingError> {
        debug!(operation = "list_staffing_reconciliations", tenant_id = %tenant_id, "Staffing service operation accepted");
        let result: Result<Vec<StaffingReconciliation>, StaffingError> =
            self.repo.list_reconciliations(tenant_id).await;
        log_staffing_operation("list_staffing_reconciliations", tenant_id, None, None, &result);
        result
    }

    pub async fn upsert_customer_work_record(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        input: CustomerWorkRecordInput,
        audit_account_id: Uuid,
    ) -> Result<CustomerWorkRecord, StaffingError> {
        let record_id: Uuid = Uuid::new_v4();
        trace!(
            operation = "upsert_customer_work_record",
            tenant_id = %tenant_id,
            audit_account_id = %audit_account_id,
            assignment_id = %assignment_id,
            record_id = %record_id,
            has_customer_reference = input.customer_reference.is_some(),
            has_notes = input.notes.is_some(),
            "Validating independent customer staffing evidence"
        );
        if input.confirmed_ended_at <= input.confirmed_started_at {
            return Err(StaffingError::InvalidInput("customer work record schedule is invalid"));
        }
        if input
            .customer_reference
            .as_deref()
            .is_some_and(|value: &str| value.len() > 200)
            || input.notes.as_deref().is_some_and(|value: &str| value.len() > 1000)
        {
            return Err(StaffingError::InvalidInput("customer work record text is invalid"));
        }
        let result: Result<CustomerWorkRecord, StaffingError> = self
            .repo
            .upsert_customer_work_record(tenant_id, record_id, assignment_id, &input, audit_account_id)
            .await;
        log_staffing_operation(
            "upsert_customer_work_record",
            tenant_id,
            Some(audit_account_id),
            Some(assignment_id),
            &result,
        );
        result
    }
}

fn log_staffing_operation<T>(
    operation: &'static str,
    tenant_id: Uuid,
    audit_account_id: Option<Uuid>,
    resource_id: Option<Uuid>,
    result: &Result<T, StaffingError>,
) {
    match result {
        Ok(_) => info!(
            operation,
            tenant_id = %tenant_id,
            audit_account_id = ?audit_account_id,
            resource_id = ?resource_id,
            "Staffing service operation completed"
        ),
        Err(StaffingError::BackendUnavailable) => error!(
            operation,
            tenant_id = %tenant_id,
            audit_account_id = ?audit_account_id,
            resource_id = ?resource_id,
            "Staffing service operation failed because the backend is unavailable"
        ),
        Err(service_error) => warn!(
            operation,
            tenant_id = %tenant_id,
            audit_account_id = ?audit_account_id,
            resource_id = ?resource_id,
            reason = ?service_error,
            "Staffing service operation was rejected"
        ),
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
    let valid_boundary: bool = code
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
    let mut parts: std::str::Split<'_, char> = value.split('.');
    let whole: &str = parts
        .next()
        .ok_or(StaffingError::InvalidInput("hourly rate is invalid"))?;
    let fraction: Option<&str> = parts.next();
    let is_zero: bool = value.chars().all(|character| character == '0' || character == '.');
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
