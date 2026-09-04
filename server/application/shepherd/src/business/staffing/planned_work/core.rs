use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use tracing::{debug, error, info, trace, warn};

use super::database::PlannedStaffingRepo;

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
pub enum RateSource {
    Configured,
    Manual,
}

impl RateSource {
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "configured" => Some(Self::Configured),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum StaffingRateKind {
    CustomerBill,
    WorkerPay,
}

impl StaffingRateKind {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::CustomerBill => "customer_bill",
            Self::WorkerPay => "worker_pay",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "customer_bill" => Some(Self::CustomerBill),
            "worker_pay" => Some(Self::WorkerPay),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct Customer {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub address: Option<String>,
    pub time_zone: String,
    pub billing_email: Option<String>,
    pub status: BusinessRecordStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CustomerCursor {
    pub normalized_name: String,
    pub code: String,
    pub customer_id: Uuid,
}

#[derive(Clone, Debug)]
pub struct CustomerPage {
    pub items: Vec<Customer>,
    pub next_cursor: Option<CustomerCursor>,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct StaffingJob {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub status: BusinessRecordStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NameCodeCursor {
    pub normalized_name: String,
    pub code: String,
    pub id: Uuid,
}

#[derive(Clone, Debug)]
pub struct KeysetPage<T, C> {
    pub items: Vec<T>,
    pub next_cursor: Option<C>,
}

pub type StaffingJobPage = KeysetPage<StaffingJob, NameCodeCursor>;

#[derive(Clone, Debug, Serialize, TS)]
pub struct StaffingReconcile {
    pub assignment_id: Uuid,
    pub shift_id: Uuid,
    pub customer_id: Uuid,
    pub job_id: Uuid,
    pub employee_id: Uuid,
    pub employee_code: String,
    pub employee_name: String,
    pub customer_name: String,
    pub confirmed_customer_name: Option<String>,
    pub scheduled_starts_at: DateTime<Utc>,
    pub scheduled_ends_at: DateTime<Utc>,
    pub assignment_status: ShiftAssignmentStatus,
    pub staff_started_at: Option<DateTime<Utc>>,
    pub staff_ended_at: Option<DateTime<Utc>>,
    pub staff_worked_seconds: i64,
    pub customer_record: Option<CustomerWorkRecord>,
    pub final_worked_seconds: Option<i64>,
    pub final_customer_id: Option<Uuid>,
    pub final_job_id: Option<Uuid>,
    pub adjustment_reason: Option<String>,
    pub reconciliation_status: ReconcileStatus,
    pub result_revision_id: Option<Uuid>,
    pub result_revision_number: Option<i32>,
}

#[derive(Clone, Debug)]
pub struct StaffingReconcilePage {
    pub items: Vec<StaffingReconcile>,
    pub next_cursor: Option<StaffingReconcileCursor>,
}

#[derive(Clone, Debug)]
pub struct CustomerInput {
    pub code: String,
    pub name: String,
    pub address: Option<String>,
    pub time_zone: String,
    pub billing_email: Option<String>,
    pub status: BusinessRecordStatus,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct StaffingRate {
    pub id: Uuid,
    pub rate_kind: StaffingRateKind,
    pub code: String,
    pub name: String,
    pub customer_id: Option<Uuid>,
    pub employee_id: Option<Uuid>,
    pub currency: String,
    pub hourly_rate: String,
    pub priority: i16,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct StaffingStaff {
    pub employee_id: Uuid,
    pub employee_code: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StaffingStaffCursor {
    pub normalized_display_name: String,
    pub employee_code: String,
    pub employee_id: Uuid,
}

#[derive(Clone, Debug)]
pub struct StaffingStaffPage {
    pub items: Vec<StaffingStaff>,
    pub next_cursor: Option<StaffingStaffCursor>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StaffingRateCursor {
    pub created_at: DateTime<Utc>,
    pub rate_id: Uuid,
}

#[derive(Clone, Debug)]
pub struct StaffingRatePage {
    pub items: Vec<StaffingRate>,
    pub next_cursor: Option<StaffingRateCursor>,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct StaffingPriceSet {
    pub customer_bill_rate: StaffingRate,
    pub worker_pay_rate: StaffingRate,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct StaffingShift {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub job_id: Uuid,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub required_workers: i32,
    pub status: StaffingShiftStatus,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StaffingShiftCursor {
    pub starts_at: DateTime<Utc>,
    pub shift_id: Uuid,
}

pub type StaffingShiftPage = KeysetPage<StaffingShift, StaffingShiftCursor>;

#[derive(Clone, Debug, Serialize, TS)]
pub struct ShiftAssignment {
    pub id: Uuid,
    pub shift_id: Uuid,
    pub employee_id: Uuid,
    pub customer_bill_rate_id: Option<Uuid>,
    pub worker_pay_rate_id: Option<Uuid>,
    pub rate_source: RateSource,
    pub manual_rate_reason: Option<String>,
    pub currency: String,
    pub bill_hourly_rate_snapshot: String,
    pub worker_hourly_rate_snapshot: String,
    pub eligibility_exception_reason: Option<String>,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ShiftAssignmentCursor {
    pub created_at: DateTime<Utc>,
    pub assignment_id: Uuid,
}

pub type ShiftAssignmentPage = KeysetPage<ShiftAssignment, ShiftAssignmentCursor>;

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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StaffingCandidateCursor {
    pub available: bool,
    pub normalized_name: String,
    pub employee_code: String,
    pub employee_id: Uuid,
}

pub type StaffingCandidatePage = KeysetPage<StaffingCandidate, StaffingCandidateCursor>;

#[derive(Clone, Debug, Serialize, TS)]
pub struct StaffingEligibility {
    pub id: Uuid,
    pub employee_id: Uuid,
    pub job_id: Uuid,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StaffingEligibilityCursor {
    pub effective_from: NaiveDate,
    pub employee_id: Uuid,
    pub job_id: Uuid,
    pub eligibility_id: Uuid,
}

pub type StaffingEligibilityPage = KeysetPage<StaffingEligibility, StaffingEligibilityCursor>;

#[derive(Clone, Debug, Serialize, TS)]
pub struct CustomerWorkRecord {
    pub id: Uuid,
    pub assignment_id: Uuid,
    pub confirmed_customer_id: Uuid,
    pub confirmed_started_at: DateTime<Utc>,
    pub confirmed_ended_at: DateTime<Utc>,
    pub confirmed_worked_seconds: i64,
    pub customer_reference: Option<String>,
    pub notes: Option<String>,
    pub updated_at: DateTime<Utc>,
}

use super::super::{
    StaffingErr, ReconcileStatus, StaffingEligibilityInput, StaffingPriceSetInput, StaffingShiftInput,
    ShiftAssignmentInput, CustomerWorkRecordInput, StaffingReconcileCursor, ReconcileCollection,
};

pub struct PlannedStaffingService {
    repo: Arc<PlannedStaffingRepo>,
}
impl PlannedStaffingService {
    pub fn new_arc(repo: Arc<PlannedStaffingRepo>) -> Arc<Self> {
        Arc::new(Self { repo })
    }

    pub async fn list_shifts(
        &self,
        tenant_id: Uuid,
        limit: i64,
        cursor: Option<StaffingShiftCursor>,
    ) -> Result<StaffingShiftPage, StaffingErr> {
        if limit <= 0 {
            return Err(StaffingErr::InvalidInput("staffing shift page size must be positive"));
        }
        debug!(operation = "list_staffing_shifts", tenant_id = %tenant_id, "Staffing service operation accepted");
        let result = self.repo.list_shifts(tenant_id, limit, cursor.as_ref()).await;
        log_staffing_operation("list_staffing_shifts", tenant_id, None, None, &result);
        result
    }

    pub async fn create_shift(
        &self,
        tenant_id: Uuid,
        input: StaffingShiftInput,
        audit_account_id: Uuid,
    ) -> Result<StaffingShift, StaffingErr> {
        let shift_id: Uuid = Uuid::new_v4();
        trace!(
            operation = "create_staffing_shift",
            tenant_id = %tenant_id,
            audit_account_id = %audit_account_id,
            shift_id = %shift_id,
            customer_id = ?input.customer_id,
            job_id = %input.job_id,
            required_workers = input.required_workers,
            "Validating staffing shift creation"
        );
        if input.ends_at <= input.starts_at || input.required_workers <= 0 {
            return Err(StaffingErr::InvalidInput("staffing shift schedule is invalid"));
        }
        let result: Result<StaffingShift, StaffingErr> = self
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

    pub async fn cancel_shift(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
        reason: String,
        audit_account_id: Uuid,
    ) -> Result<(), StaffingErr> {
        let reason: String = normalize_cancellation_reason(reason)?;
        let result = self
            .repo
            .cancel_shift(tenant_id, shift_id, &reason, audit_account_id)
            .await;
        log_staffing_operation(
            "cancel_staffing_shift",
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
        limit: i64,
        cursor: Option<ShiftAssignmentCursor>,
    ) -> Result<ShiftAssignmentPage, StaffingErr> {
        if limit <= 0 {
            return Err(StaffingErr::InvalidInput("assignment page size must be positive"));
        }
        debug!(
            operation = "list_shift_assignments",
            tenant_id = %tenant_id,
            shift_id = %shift_id,
            "Staffing service operation accepted"
        );
        let result = self
            .repo
            .list_shift_assignments(tenant_id, shift_id, limit, cursor.as_ref())
            .await;
        log_staffing_operation("list_shift_assignments", tenant_id, None, Some(shift_id), &result);
        result
    }

    pub async fn list_shift_candidates(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
        search: Option<String>,
        limit: i64,
        cursor: Option<StaffingCandidateCursor>,
    ) -> Result<StaffingCandidatePage, StaffingErr> {
        if limit <= 0 {
            return Err(StaffingErr::InvalidInput("candidate page size must be positive"));
        }
        debug!(
            operation = "list_shift_candidates",
            tenant_id = %tenant_id,
            shift_id = %shift_id,
            "Staffing service operation accepted"
        );
        let result = self
            .repo
            .list_shift_candidates(tenant_id, shift_id, search.as_deref(), limit, cursor.as_ref())
            .await;
        log_staffing_operation("list_shift_candidates", tenant_id, None, Some(shift_id), &result);
        result
    }

    pub async fn create_shift_assignment(
        &self,
        tenant_id: Uuid,
        shift_id: Uuid,
        mut input: ShiftAssignmentInput,
        audit_account_id: Uuid,
    ) -> Result<ShiftAssignment, StaffingErr> {
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
        if let Some(manual_rate) = input.manual_rate.as_mut() {
            manual_rate.reason = manual_rate.reason.trim().to_owned();
            if !(3..=500).contains(&manual_rate.reason.len()) {
                return Err(StaffingErr::InvalidInput("manual staffing rate reason is invalid"));
            }
            validate_currency(&manual_rate.currency)?;
            validate_positive_decimal(&manual_rate.bill_hourly_rate)?;
            validate_positive_decimal(&manual_rate.worker_hourly_rate)?;
        }
        let result: Result<ShiftAssignment, StaffingErr> = self
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

    pub async fn cancel_shift_assignment(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        reason: String,
        audit_account_id: Uuid,
    ) -> Result<(), StaffingErr> {
        let reason: String = normalize_cancellation_reason(reason)?;
        let result = self
            .repo
            .cancel_shift_assignment(tenant_id, assignment_id, &reason, audit_account_id)
            .await;
        log_staffing_operation(
            "cancel_staffing_shift_assignment",
            tenant_id,
            Some(audit_account_id),
            Some(assignment_id),
            &result,
        );
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn approve_shift_assignment(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        worked_seconds: Option<i64>,
        adjustment_reason: Option<String>,
        final_customer_id: Option<Uuid>,
        final_job_id: Option<Uuid>,
        audit_account_id: Uuid,
    ) -> Result<ShiftAssignment, StaffingErr> {
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
        if final_customer_id.is_some_and(|value| value.is_nil()) || final_job_id.is_some_and(|value| value.is_nil()) {
            return Err(StaffingErr::InvalidInput("final customer or job is invalid"));
        }
        if (final_customer_id.is_some() || final_job_id.is_some()) && adjustment_reason.is_none() {
            return Err(StaffingErr::InvalidInput(
                "a final customer or job override requires an adjustment reason",
            ));
        }
        let result: Result<ShiftAssignment, StaffingErr> = self
            .repo
            .approve_shift_assignment(
                tenant_id,
                assignment_id,
                worked_seconds,
                adjustment_reason,
                final_customer_id,
                final_job_id,
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

    pub async fn accept_staff_work_record(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        audit_account_id: Uuid,
    ) -> Result<ShiftAssignment, StaffingErr> {
        trace!(
            operation = "accept_staff_work_record",
            tenant_id = %tenant_id,
            audit_account_id = %audit_account_id,
            assignment_id = %assignment_id,
            "Finalizing an exact staff and customer evidence match in one transaction"
        );
        let result: Result<ShiftAssignment, StaffingErr> = self
            .repo
            .accept_staff_work_record(tenant_id, assignment_id, audit_account_id)
            .await;
        log_staffing_operation(
            "accept_staff_work_record",
            tenant_id,
            Some(audit_account_id),
            Some(assignment_id),
            &result,
        );
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn list_reconciliations(
        &self,
        tenant_id: Uuid,
        customer_id: Option<Uuid>,
        collection: ReconcileCollection,
        period_start: Option<DateTime<Utc>>,
        period_end: Option<DateTime<Utc>>,
        limit: i64,
        cursor: Option<StaffingReconcileCursor>,
    ) -> Result<StaffingReconcilePage, StaffingErr> {
        if limit <= 0 {
            return Err(StaffingErr::InvalidInput("reconciliation page size must be positive"));
        }
        if collection == ReconcileCollection::Confirmed
            && !matches!((period_start, period_end), (Some(start), Some(end)) if end > start)
        {
            return Err(StaffingErr::InvalidInput("confirmed reconciliation period is invalid"));
        }
        debug!(operation = "list_staffing_reconciliations", tenant_id = %tenant_id, "Staffing service operation accepted");
        let result: Result<StaffingReconcilePage, StaffingErr> = self
            .repo
            .list_reconciliations(
                tenant_id,
                customer_id,
                collection,
                period_start,
                period_end,
                limit,
                cursor.as_ref(),
            )
            .await;
        log_staffing_operation("list_staffing_reconciliations", tenant_id, None, None, &result);
        result
    }

    pub async fn upsert_customer_work_record(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        input: CustomerWorkRecordInput,
        audit_account_id: Uuid,
        allow_terminal_correction: bool,
    ) -> Result<CustomerWorkRecord, StaffingErr> {
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
            return Err(StaffingErr::InvalidInput("customer work record schedule is invalid"));
        }
        if input
            .customer_reference
            .as_deref()
            .is_some_and(|value: &str| value.len() > 200)
            || input.notes.as_deref().is_some_and(|value: &str| value.len() > 1000)
        {
            return Err(StaffingErr::InvalidInput("customer work record text is invalid"));
        }
        let result: Result<CustomerWorkRecord, StaffingErr> = self
            .repo
            .upsert_customer_work_record(
                tenant_id,
                record_id,
                assignment_id,
                &input,
                audit_account_id,
                allow_terminal_correction,
            )
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

fn validate_customer_location(input: &CustomerInput) -> Result<(), StaffingErr> {
    if input.time_zone.is_empty() || input.time_zone.len() > 128 {
        return Err(StaffingErr::InvalidInput("customer time zone is invalid"));
    }
    if input
        .address
        .as_deref()
        .is_some_and(|address: &str| address.len() > 500)
    {
        return Err(StaffingErr::InvalidInput("customer address is invalid"));
    }
    Ok(())
}

fn normalize_cancellation_reason(reason: String) -> Result<String, StaffingErr> {
    let reason: String = reason.trim().to_owned();
    if !(3..=500).contains(&reason.chars().count()) {
        return Err(StaffingErr::InvalidInput("staffing cancellation reason is invalid"));
    }
    Ok(reason)
}

fn log_staffing_operation<T>(
    operation: &'static str,
    tenant_id: Uuid,
    audit_account_id: Option<Uuid>,
    resource_id: Option<Uuid>,
    result: &Result<T, StaffingErr>,
) {
    match result {
        Ok(_) => info!(
            operation,
            tenant_id = %tenant_id,
            audit_account_id = ?audit_account_id,
            resource_id = ?resource_id,
            "Staffing service operation completed"
        ),
        Err(StaffingErr::BackendUnavailable) => error!(
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

fn validate_approval_input(worked_seconds: Option<i64>, adjustment_reason: Option<&str>) -> Result<(), StaffingErr> {
    if worked_seconds.is_some_and(|seconds| seconds <= 0) {
        return Err(StaffingErr::InvalidInput("worked seconds must be positive"));
    }
    if adjustment_reason.is_some_and(|reason| reason.len() < 3 || reason.len() > 500 || reason != reason.trim()) {
        return Err(StaffingErr::InvalidInput("approval adjustment reason is invalid"));
    }
    Ok(())
}

fn validate_identity(code: &str, name: &str) -> Result<(), StaffingErr> {
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
        return Err(StaffingErr::InvalidInput("business code is invalid"));
    }
    if name.is_empty() || name.len() > 200 || name != name.trim() {
        return Err(StaffingErr::InvalidInput("business name is invalid"));
    }
    Ok(())
}

fn validate_currency(currency: &str) -> Result<(), StaffingErr> {
    if currency.len() != 3 || currency.chars().any(|character| !character.is_ascii_uppercase()) {
        return Err(StaffingErr::InvalidInput(
            "currency must be a three-letter uppercase code",
        ));
    }
    Ok(())
}

fn validate_positive_decimal(value: &str) -> Result<(), StaffingErr> {
    let mut parts: std::str::Split<'_, char> = value.split('.');
    let whole: &str = parts
        .next()
        .ok_or(StaffingErr::InvalidInput("hourly rate is invalid"))?;
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
        return Err(StaffingErr::InvalidInput("hourly rate is invalid"));
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
