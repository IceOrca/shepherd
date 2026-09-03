use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, trace, warn};
use ts_rs::TS;
use uuid::Uuid;

use super::super::core::{ManualRateOverride, ReconcileCollection, ReconcileStatus};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum UrgentWorkStatus {
    Active,
    Completed,
    Reconciled,
    Cancelled,
}

impl UrgentWorkStatus {
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "active" => Some(Self::Active),
            "completed" => Some(Self::Completed),
            "reconciled" => Some(Self::Reconciled),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum UrgentWorkActionSource {
    SelfReported,
    Peer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum UrgentWorkSubmissionKind {
    Live,
    Manual,
}

impl UrgentWorkSubmissionKind {
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "live" => Some(Self::Live),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

impl UrgentWorkActionSource {
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "self" => Some(Self::SelfReported),
            "peer" => Some(Self::Peer),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct UrgentWorkCustomer {
    pub customer_id: Uuid,
    pub customer_name: String,
    pub address: Option<String>,
    pub time_zone: String,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct UrgentWorkEmployee {
    pub employee_id: Uuid,
    pub employee_code: String,
    pub display_name: String,
    pub is_self: bool,
    pub has_open_work: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UrgentCustomerCursor {
    pub name: String,
    pub customer_id: Uuid,
}

#[derive(Clone, Debug)]
pub struct UrgentCustomerPage {
    pub items: Vec<UrgentWorkCustomer>,
    pub next_cursor: Option<UrgentCustomerCursor>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UrgentEmployeeCursor {
    pub is_self: bool,
    pub name: String,
    pub employee_id: Uuid,
}

#[derive(Clone, Debug)]
pub struct UrgentEmployeePage {
    pub items: Vec<UrgentWorkEmployee>,
    pub next_cursor: Option<UrgentEmployeeCursor>,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct UrgentWorkItem {
    pub report_id: Uuid,
    pub branch_id: Uuid,
    pub branch_name: String,
    pub employee_id: Uuid,
    pub employee_code: String,
    pub employee_name: String,
    pub claimed_customer_id: Uuid,
    pub customer_name: String,
    pub submission_kind: UrgentWorkSubmissionKind,
    pub staff_note: Option<String>,
    pub status: UrgentWorkStatus,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub worked_seconds: Option<i64>,
    pub started_by_account_id: Uuid,
    pub started_by_username: String,
    pub start_source: UrgentWorkActionSource,
    pub ended_by_account_id: Option<Uuid>,
    pub ended_by_username: Option<String>,
    pub end_source: Option<UrgentWorkActionSource>,
    pub reconciled_assignment_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct UrgentCustomerWorkRecord {
    pub id: Uuid,
    pub report_id: Uuid,
    pub confirmed_customer_id: Uuid,
    pub confirmed_customer_name: String,
    pub confirmed_started_at: DateTime<Utc>,
    pub confirmed_ended_at: DateTime<Utc>,
    pub confirmed_worked_seconds: i64,
    pub customer_reference: Option<String>,
    pub notes: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct UrgentWorkReconcile {
    pub work: UrgentWorkItem,
    pub customer_record: Option<UrgentCustomerWorkRecord>,
    pub reconciliation_status: ReconcileStatus,
    pub final_customer_id: Option<Uuid>,
    pub final_job_id: Option<Uuid>,
    pub final_worked_seconds: Option<i64>,
    pub adjustment_reason: Option<String>,
    pub eligibility_exception_reason: Option<String>,
    pub result_revision_id: Option<Uuid>,
    pub result_revision_number: Option<i32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UrgentReconcileCursor {
    pub active: bool,
    pub started_at: DateTime<Utc>,
    pub report_id: Uuid,
}

#[derive(Clone, Debug)]
pub struct UrgentReconcilePage {
    pub items: Vec<UrgentWorkReconcile>,
    pub next_cursor: Option<UrgentReconcileCursor>,
}

#[derive(Clone, Debug)]
pub struct UrgentWorkLocationInput {
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub accuracy_meters: Option<f32>,
}

impl UrgentWorkLocationInput {
    pub fn validate(&self) -> Result<(), UrgentWorkError> {
        match (self.latitude, self.longitude) {
            (None, None) if self.accuracy_meters.is_none() => Ok(()),
            (Some(latitude), Some(longitude))
                if latitude.is_finite()
                    && longitude.is_finite()
                    && (-90.0..=90.0).contains(&latitude)
                    && (-180.0..=180.0).contains(&longitude)
                    && self
                        .accuracy_meters
                        .is_none_or(|accuracy: f32| accuracy.is_finite() && accuracy >= 0.0) =>
            {
                Ok(())
            }
            _ => Err(UrgentWorkError::InvalidInput("urgent-work location is invalid")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct UrgentWorkStartInput {
    pub customer_id: Uuid,
    pub employee_ids: Vec<Uuid>,
    pub idempotency_key: Uuid,
    pub location: UrgentWorkLocationInput,
}

#[derive(Clone, Debug)]
pub struct UrgentWorkEndInput {
    pub idempotency_key: Uuid,
    pub location: UrgentWorkLocationInput,
}

#[derive(Clone, Debug)]
pub struct UrgentWorkManualInput {
    pub customer_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub note: Option<String>,
    pub idempotency_key: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UrgentOwnWorkCursor {
    pub active: bool,
    pub started_at: DateTime<Utc>,
    pub report_id: Uuid,
}

#[derive(Clone, Debug)]
pub struct UrgentOwnWorkPage {
    pub items: Vec<UrgentWorkItem>,
    pub next_cursor: Option<UrgentOwnWorkCursor>,
}

pub type UrgentTeamWorkPage = UrgentOwnWorkPage;

#[derive(Clone, Debug)]
pub struct UrgentCustomerWorkRecordInput {
    pub confirmed_customer_id: Uuid,
    pub confirmed_started_at: DateTime<Utc>,
    pub confirmed_ended_at: DateTime<Utc>,
    pub customer_reference: Option<String>,
    pub notes: Option<String>,
}

#[derive(Clone, Debug)]
pub struct UrgentWorkReconcileInput {
    pub final_customer_id: Uuid,
    pub job_id: Uuid,
    pub worked_seconds: i64,
    pub adjustment_reason: Option<String>,
    pub manual_rate: Option<ManualRateOverride>,
}

#[derive(Debug)]
pub enum UrgentWorkError {
    NotFound,
    Forbidden,
    Conflict,
    InvalidInput(&'static str),
    MissingStaffingRate,
    BackendUnavailable,
}

#[async_trait]
pub trait UrgentWorkRepo {
    async fn list_customers(
        &self,
        tenant_id: Uuid,
        search: Option<&str>,
        limit: i64,
        cursor: Option<&UrgentCustomerCursor>,
    ) -> Result<UrgentCustomerPage, UrgentWorkError>;
    async fn list_employees(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        search: Option<&str>,
        limit: i64,
        cursor: Option<&UrgentEmployeeCursor>,
    ) -> Result<UrgentEmployeePage, UrgentWorkError>;
    async fn list_own_work(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        limit: i64,
        cursor: Option<&UrgentOwnWorkCursor>,
    ) -> Result<UrgentOwnWorkPage, UrgentWorkError>;
    async fn list_team_work(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        limit: i64,
        cursor: Option<&UrgentOwnWorkCursor>,
    ) -> Result<UrgentTeamWorkPage, UrgentWorkError>;
    #[allow(clippy::too_many_arguments)]
    async fn start(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        allow_peer: bool,
        batch_id: Uuid,
        report_ids: &[Uuid],
        session_ids: &[Uuid],
        input: &UrgentWorkStartInput,
    ) -> Result<Vec<UrgentWorkItem>, UrgentWorkError>;
    async fn end(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        allow_peer: bool,
        report_id: Uuid,
        input: &UrgentWorkEndInput,
    ) -> Result<UrgentWorkItem, UrgentWorkError>;
    #[allow(clippy::too_many_arguments)]
    async fn submit_manual(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        batch_id: Uuid,
        report_id: Uuid,
        session_id: Uuid,
        input: &UrgentWorkManualInput,
    ) -> Result<UrgentWorkItem, UrgentWorkError>;
    async fn cancel(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        report_id: Uuid,
        reason: &str,
    ) -> Result<(), UrgentWorkError>;
    #[allow(clippy::too_many_arguments)]
    async fn list_reconciliations(
        &self,
        tenant_id: Uuid,
        customer_id: Option<Uuid>,
        collection: ReconcileCollection,
        period_start: Option<DateTime<Utc>>,
        period_end: Option<DateTime<Utc>>,
        limit: i64,
        cursor: Option<&UrgentReconcileCursor>,
    ) -> Result<UrgentReconcilePage, UrgentWorkError>;
    async fn upsert_customer_record(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        record_id: Uuid,
        report_id: Uuid,
        input: &UrgentCustomerWorkRecordInput,
        allow_terminal_correction: bool,
    ) -> Result<UrgentCustomerWorkRecord, UrgentWorkError>;
    async fn reconcile(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        shift_id: Uuid,
        assignment_id: Uuid,
        report_id: Uuid,
        input: &UrgentWorkReconcileInput,
    ) -> Result<UrgentWorkReconcile, UrgentWorkError>;
    #[allow(clippy::too_many_arguments)]
    async fn accept_staff_record(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        shift_id: Uuid,
        assignment_id: Uuid,
        report_id: Uuid,
        job_id: Uuid,
    ) -> Result<UrgentWorkReconcile, UrgentWorkError>;
}

pub type DynUrgentWorkRepo = Arc<dyn UrgentWorkRepo + Send + Sync>;

pub struct UrgentWorkService {
    repo: DynUrgentWorkRepo,
}

impl UrgentWorkService {
    pub fn new_arc(repo: DynUrgentWorkRepo) -> Arc<Self> {
        Arc::new(Self { repo })
    }

    pub async fn list_customers(
        &self,
        tenant_id: Uuid,
        search: Option<String>,
        limit: i64,
        cursor: Option<UrgentCustomerCursor>,
    ) -> Result<UrgentCustomerPage, UrgentWorkError> {
        if limit <= 0 {
            return Err(UrgentWorkError::InvalidInput(
                "urgent customer page size must be positive",
            ));
        }
        debug!(operation = "urgent_work.list_customers", tenant_id = %tenant_id, "Urgent-work operation accepted");
        let result: Result<UrgentCustomerPage, UrgentWorkError> = self
            .repo
            .list_customers(tenant_id, search.as_deref(), limit, cursor.as_ref())
            .await;
        log_result("urgent_work.list_customers", tenant_id, None, None, &result);
        result
    }

    pub async fn list_employees(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        search: Option<String>,
        limit: i64,
        cursor: Option<UrgentEmployeeCursor>,
    ) -> Result<UrgentEmployeePage, UrgentWorkError> {
        if limit <= 0 {
            return Err(UrgentWorkError::InvalidInput(
                "urgent employee page size must be positive",
            ));
        }
        debug!(operation = "urgent_work.list_employees", tenant_id = %tenant_id, actor_account_id = %actor_account_id, "Urgent-work operation accepted");
        let result: Result<UrgentEmployeePage, UrgentWorkError> = self
            .repo
            .list_employees(tenant_id, actor_account_id, search.as_deref(), limit, cursor.as_ref())
            .await;
        log_result(
            "urgent_work.list_employees",
            tenant_id,
            Some(actor_account_id),
            None,
            &result,
        );
        result
    }

    pub async fn list_own_work(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        limit: i64,
        cursor: Option<UrgentOwnWorkCursor>,
    ) -> Result<UrgentOwnWorkPage, UrgentWorkError> {
        if limit <= 0 {
            return Err(UrgentWorkError::InvalidInput(
                "urgent own-work page size must be positive",
            ));
        }
        let result: Result<UrgentOwnWorkPage, UrgentWorkError> = self
            .repo
            .list_own_work(tenant_id, actor_account_id, limit, cursor.as_ref())
            .await;
        log_result("urgent_work.list_own", tenant_id, Some(actor_account_id), None, &result);
        result
    }

    pub async fn list_team_work(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        limit: i64,
        cursor: Option<UrgentOwnWorkCursor>,
    ) -> Result<UrgentTeamWorkPage, UrgentWorkError> {
        if limit <= 0 {
            return Err(UrgentWorkError::InvalidInput(
                "urgent team-work page size must be positive",
            ));
        }
        let result: Result<UrgentTeamWorkPage, UrgentWorkError> = self
            .repo
            .list_team_work(tenant_id, actor_account_id, limit, cursor.as_ref())
            .await;
        log_result(
            "urgent_work.list_team",
            tenant_id,
            Some(actor_account_id),
            None,
            &result,
        );
        result
    }

    pub async fn start(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        allow_peer: bool,
        mut input: UrgentWorkStartInput,
    ) -> Result<Vec<UrgentWorkItem>, UrgentWorkError> {
        input.location.validate()?;
        if input.customer_id.is_nil() || input.employee_ids.is_empty() || input.employee_ids.len() > 50 {
            return Err(UrgentWorkError::InvalidInput("urgent-work start selection is invalid"));
        }
        let employee_ids: BTreeSet<Uuid> = input.employee_ids.into_iter().collect();
        if employee_ids.iter().any(Uuid::is_nil) {
            return Err(UrgentWorkError::InvalidInput(
                "urgent-work employee selection is invalid",
            ));
        }
        input.employee_ids = employee_ids.into_iter().collect();
        let target_count: usize = input.employee_ids.len();
        let batch_id: Uuid = Uuid::new_v4();
        let report_ids: Vec<Uuid> = (0..target_count).map(|_index: usize| Uuid::new_v4()).collect();
        let session_ids: Vec<Uuid> = (0..target_count).map(|_index: usize| Uuid::new_v4()).collect();
        trace!(operation = "urgent_work.start", tenant_id = %tenant_id, actor_account_id = %actor_account_id, customer_id = %input.customer_id, target_count, allow_peer, "Validated urgent-work batch without logging location coordinates");
        let result: Result<Vec<UrgentWorkItem>, UrgentWorkError> = self
            .repo
            .start(
                tenant_id,
                actor_account_id,
                allow_peer,
                batch_id,
                &report_ids,
                &session_ids,
                &input,
            )
            .await;
        log_result(
            "urgent_work.start",
            tenant_id,
            Some(actor_account_id),
            Some(batch_id),
            &result,
        );
        result
    }

    pub async fn end(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        allow_peer: bool,
        report_id: Uuid,
        input: UrgentWorkEndInput,
    ) -> Result<UrgentWorkItem, UrgentWorkError> {
        input.location.validate()?;
        if report_id.is_nil() {
            return Err(UrgentWorkError::InvalidInput("urgent-work report ID is invalid"));
        }
        let result: Result<UrgentWorkItem, UrgentWorkError> = self
            .repo
            .end(tenant_id, actor_account_id, allow_peer, report_id, &input)
            .await;
        log_result(
            "urgent_work.end",
            tenant_id,
            Some(actor_account_id),
            Some(report_id),
            &result,
        );
        result
    }

    pub async fn submit_manual(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        mut input: UrgentWorkManualInput,
    ) -> Result<UrgentWorkItem, UrgentWorkError> {
        if input.customer_id.is_nil()
            || input.idempotency_key.is_nil()
            || input.ended_at <= input.started_at
            || input.ended_at > Utc::now() + chrono::Duration::minutes(5)
        {
            return Err(UrgentWorkError::InvalidInput(
                "manual urgent-work declaration is invalid",
            ));
        }
        input.note = input
            .note
            .take()
            .map(|note: String| note.trim().to_owned())
            .filter(|note: &String| !note.is_empty());
        if input.note.as_deref().is_some_and(|note: &str| note.len() > 1000) {
            return Err(UrgentWorkError::InvalidInput("manual urgent-work note is invalid"));
        }
        let report_id: Uuid = Uuid::new_v4();
        let result: Result<UrgentWorkItem, UrgentWorkError> = self
            .repo
            .submit_manual(
                tenant_id,
                actor_account_id,
                Uuid::new_v4(),
                report_id,
                Uuid::new_v4(),
                &input,
            )
            .await;
        log_result(
            "urgent_work.submit_manual",
            tenant_id,
            Some(actor_account_id),
            Some(report_id),
            &result,
        );
        result
    }

    pub async fn cancel(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        report_id: Uuid,
        reason: String,
    ) -> Result<(), UrgentWorkError> {
        if report_id.is_nil() {
            return Err(UrgentWorkError::InvalidInput("urgent-work report ID is invalid"));
        }
        let reason: String = reason.trim().to_owned();
        if !(3..=500).contains(&reason.chars().count()) {
            return Err(UrgentWorkError::InvalidInput(
                "urgent-work cancellation reason is invalid",
            ));
        }
        let result = self.repo.cancel(tenant_id, actor_account_id, report_id, &reason).await;
        log_result(
            "urgent_work.cancel",
            tenant_id,
            Some(actor_account_id),
            Some(report_id),
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
        cursor: Option<UrgentReconcileCursor>,
    ) -> Result<UrgentReconcilePage, UrgentWorkError> {
        if limit <= 0 {
            return Err(UrgentWorkError::InvalidInput(
                "urgent reconciliation page size must be positive",
            ));
        }
        if collection == ReconcileCollection::Confirmed
            && !matches!((period_start, period_end), (Some(start), Some(end)) if end > start)
        {
            return Err(UrgentWorkError::InvalidInput(
                "confirmed reconciliation period is invalid",
            ));
        }
        let result: Result<UrgentReconcilePage, UrgentWorkError> = self
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
        log_result("urgent_work.list_reconciliations", tenant_id, None, None, &result);
        result
    }

    pub async fn upsert_customer_record(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        report_id: Uuid,
        input: UrgentCustomerWorkRecordInput,
        allow_terminal_correction: bool,
    ) -> Result<UrgentCustomerWorkRecord, UrgentWorkError> {
        if input.confirmed_customer_id.is_nil() || input.confirmed_ended_at <= input.confirmed_started_at {
            return Err(UrgentWorkError::InvalidInput("urgent customer evidence is invalid"));
        }
        if input
            .customer_reference
            .as_deref()
            .is_some_and(|value: &str| value.len() > 200)
            || input.notes.as_deref().is_some_and(|value: &str| value.len() > 1000)
        {
            return Err(UrgentWorkError::InvalidInput(
                "urgent customer evidence text is invalid",
            ));
        }
        let record_id: Uuid = Uuid::new_v4();
        let result: Result<UrgentCustomerWorkRecord, UrgentWorkError> = self
            .repo
            .upsert_customer_record(
                tenant_id,
                actor_account_id,
                record_id,
                report_id,
                &input,
                allow_terminal_correction,
            )
            .await;
        log_result(
            "urgent_work.upsert_customer_record",
            tenant_id,
            Some(actor_account_id),
            Some(report_id),
            &result,
        );
        result
    }

    pub async fn reconcile(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        report_id: Uuid,
        mut input: UrgentWorkReconcileInput,
    ) -> Result<UrgentWorkReconcile, UrgentWorkError> {
        if input.final_customer_id.is_nil() || input.job_id.is_nil() || input.worked_seconds <= 0 {
            return Err(UrgentWorkError::InvalidInput(
                "urgent reconciliation values are invalid",
            ));
        }
        input.adjustment_reason = input
            .adjustment_reason
            .take()
            .map(|reason: String| reason.trim().to_owned())
            .filter(|reason: &String| !reason.is_empty());
        if input
            .adjustment_reason
            .as_deref()
            .is_some_and(|reason: &str| !(3..=500).contains(&reason.len()))
        {
            return Err(UrgentWorkError::InvalidInput("urgent reconciliation reason is invalid"));
        }
        if let Some(manual_rate) = input.manual_rate.as_mut() {
            manual_rate.reason = manual_rate.reason.trim().to_owned();
            validate_manual_rate(manual_rate)?;
        }
        let shift_id: Uuid = Uuid::new_v4();
        let assignment_id: Uuid = Uuid::new_v4();
        let result: Result<UrgentWorkReconcile, UrgentWorkError> = self
            .repo
            .reconcile(tenant_id, actor_account_id, shift_id, assignment_id, report_id, &input)
            .await;
        log_result(
            "urgent_work.reconcile",
            tenant_id,
            Some(actor_account_id),
            Some(report_id),
            &result,
        );
        result
    }

    pub async fn accept_staff_record(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        report_id: Uuid,
        job_id: Uuid,
    ) -> Result<UrgentWorkReconcile, UrgentWorkError> {
        if job_id.is_nil() {
            return Err(UrgentWorkError::InvalidInput("urgent reconciliation job is invalid"));
        }
        let result: Result<UrgentWorkReconcile, UrgentWorkError> = self
            .repo
            .accept_staff_record(
                tenant_id,
                actor_account_id,
                Uuid::new_v4(),
                Uuid::new_v4(),
                report_id,
                job_id,
            )
            .await;
        log_result(
            "urgent_work.accept_staff_record",
            tenant_id,
            Some(actor_account_id),
            Some(report_id),
            &result,
        );
        result
    }
}

fn validate_manual_rate(rate: &ManualRateOverride) -> Result<(), UrgentWorkError> {
    let reason_valid: bool = (3..=500).contains(&rate.reason.len()) && rate.reason == rate.reason.trim();
    let currency_valid: bool =
        rate.currency.len() == 3 && rate.currency.bytes().all(|byte: u8| byte.is_ascii_uppercase());
    let bill_rate_valid: bool = is_positive_decimal(&rate.bill_hourly_rate);
    let worker_rate_valid: bool = is_positive_decimal(&rate.worker_hourly_rate);
    if reason_valid && currency_valid && bill_rate_valid && worker_rate_valid {
        Ok(())
    } else {
        Err(UrgentWorkError::InvalidInput("urgent manual rate is invalid"))
    }
}

fn is_positive_decimal(value: &str) -> bool {
    let mut parts: std::str::Split<'_, char> = value.split('.');
    let whole: Option<&str> = parts.next();
    let fraction: Option<&str> = parts.next();
    let has_extra_part: bool = parts.next().is_some();
    let is_zero: bool = value
        .chars()
        .all(|character: char| character == '0' || character == '.');
    !value.is_empty()
        && value == value.trim()
        && !value.starts_with('-')
        && !has_extra_part
        && whole.is_some_and(|part: &str| {
            !part.is_empty() && part.chars().all(|character: char| character.is_ascii_digit())
        })
        && fraction.is_none_or(|part: &str| {
            !part.is_empty() && part.len() <= 4 && part.chars().all(|character: char| character.is_ascii_digit())
        })
        && !is_zero
}

fn log_result<T>(
    operation: &'static str,
    tenant_id: Uuid,
    actor_account_id: Option<Uuid>,
    resource_id: Option<Uuid>,
    result: &Result<T, UrgentWorkError>,
) {
    match result {
        Ok(_) => {
            info!(operation, tenant_id = %tenant_id, actor_account_id = ?actor_account_id, resource_id = ?resource_id, "Urgent-work operation completed")
        }
        Err(UrgentWorkError::BackendUnavailable) => {
            error!(operation, tenant_id = %tenant_id, actor_account_id = ?actor_account_id, resource_id = ?resource_id, "Urgent-work backend operation failed")
        }
        Err(operation_error) => {
            warn!(operation, tenant_id = %tenant_id, actor_account_id = ?actor_account_id, resource_id = ?resource_id, reason = ?operation_error, "Urgent-work operation rejected")
        }
    }
}
