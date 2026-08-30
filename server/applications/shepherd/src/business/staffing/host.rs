use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, NaiveDate, Utc};
use tracing::{error, warn, info, debug, trace};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::{AppContext, auth::AuthenticatedUser};
use crate::pagination::{decode_cursor, encode_cursor, normalize_search, resolve_limit};

use super::core::{
    BusinessRecordStatus, Customer, CustomerCursor, CustomerInput, CustomerPage, CustomerWorkRecord,
    CustomerWorkRecordInput, ManualRateOverride, ShiftAssignment, ShiftAssignmentInput, StaffingCandidate,
    StaffingEligibility, StaffingEligibilityInput, StaffingError, StaffingJob, StaffingPriceSet, StaffingPriceSetInput,
    StaffingRate, StaffingRateCursor, StaffingRatePage, StaffingReconciliation, StaffingReconciliationCursor,
    StaffingReconciliationPage, StaffingShift, StaffingShiftInput, StaffingStaff, StaffingStaffCursor,
    StaffingStaffPage,
};

#[derive(Debug, Deserialize)]
pub struct ReconciliationPageQuery {
    pub limit: Option<u16>,
    pub cursor: Option<String>,
    pub customer_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct CustomerPageQuery {
    pub limit: Option<u16>,
    pub cursor: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Serialize, TS)]
pub struct CustomerPageResponse {
    pub items: Vec<Customer>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub limit: u16,
}

#[derive(Debug, Deserialize)]
pub struct StaffingRatePageQuery {
    pub limit: Option<u16>,
    pub cursor: Option<String>,
    pub customer_id: Option<Uuid>,
}

#[derive(Debug, Serialize, TS)]
pub struct StaffingRatePageResponse {
    pub items: Vec<StaffingRate>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub limit: u16,
}

#[derive(Debug, Deserialize)]
pub struct StaffingStaffPageQuery {
    pub limit: Option<u16>,
    pub cursor: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Serialize, TS)]
pub struct StaffingStaffPageResponse {
    pub items: Vec<StaffingStaff>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub limit: u16,
}

#[derive(Debug, Serialize, TS)]
pub struct StaffingReconciliationPageResponse {
    pub items: Vec<StaffingReconciliation>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub limit: u16,
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct CustomerUpsertRequest {
    pub code: String,
    pub name: String,
    pub address: Option<String>,
    pub time_zone: String,
    pub billing_email: Option<String>,
    pub status: BusinessRecordStatus,
}

impl From<CustomerUpsertRequest> for CustomerInput {
    fn from(value: CustomerUpsertRequest) -> Self {
        Self {
            code: value.code.trim().to_ascii_lowercase(),
            name: value.name.trim().to_owned(),
            address: normalize_optional(value.address),
            time_zone: value.time_zone.trim().to_owned(),
            billing_email: normalize_optional(value.billing_email),
            status: value.status,
        }
    }
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct StaffingPriceSetRequest {
    pub customer_id: Uuid,
    pub employee_id: Option<Uuid>,
    pub currency: String,
    pub customer_hourly_rate: String,
    pub worker_hourly_rate: String,
    pub effective_from: NaiveDate,
}

impl From<StaffingPriceSetRequest> for StaffingPriceSetInput {
    fn from(value: StaffingPriceSetRequest) -> Self {
        Self {
            customer_id: value.customer_id,
            employee_id: value.employee_id,
            currency: value.currency.trim().to_ascii_uppercase(),
            customer_hourly_rate: value.customer_hourly_rate.trim().to_owned(),
            worker_hourly_rate: value.worker_hourly_rate.trim().to_owned(),
            effective_from: value.effective_from,
        }
    }
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct StaffingEligibilityCreateRequest {
    pub employee_id: Uuid,
    pub job_id: Uuid,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub notes: Option<String>,
}

impl From<StaffingEligibilityCreateRequest> for StaffingEligibilityInput {
    fn from(value: StaffingEligibilityCreateRequest) -> Self {
        Self {
            employee_id: value.employee_id,
            job_id: value.job_id,
            effective_from: value.effective_from,
            effective_to: value.effective_to,
            notes: normalize_optional(value.notes),
        }
    }
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct StaffingShiftCreateRequest {
    pub customer_id: Uuid,
    pub job_id: Uuid,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub required_workers: i32,
    pub notes: Option<String>,
}

impl From<StaffingShiftCreateRequest> for StaffingShiftInput {
    fn from(value: StaffingShiftCreateRequest) -> Self {
        Self {
            customer_id: value.customer_id,
            job_id: value.job_id,
            starts_at: value.starts_at,
            ends_at: value.ends_at,
            required_workers: value.required_workers,
            notes: normalize_optional(value.notes),
        }
    }
}

#[derive(Debug, Deserialize, TS)]
pub struct ManualRateOverrideRequest {
    pub reason: String,
    pub currency: String,
    pub bill_hourly_rate: String,
    pub worker_hourly_rate: String,
}

impl From<ManualRateOverrideRequest> for ManualRateOverride {
    fn from(value: ManualRateOverrideRequest) -> Self {
        Self {
            reason: value.reason.trim().to_owned(),
            currency: value.currency.trim().to_ascii_uppercase(),
            bill_hourly_rate: value.bill_hourly_rate.trim().to_owned(),
            worker_hourly_rate: value.worker_hourly_rate.trim().to_owned(),
        }
    }
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct ShiftAssignmentCreateRequest {
    pub employee_id: Uuid,
    pub manual_rate: Option<ManualRateOverrideRequest>,
}

impl From<ShiftAssignmentCreateRequest> for ShiftAssignmentInput {
    fn from(value: ShiftAssignmentCreateRequest) -> Self {
        Self {
            employee_id: value.employee_id,
            manual_rate: value.manual_rate.map(ManualRateOverride::from),
        }
    }
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct ShiftAssignmentApproveRequest {
    pub worked_seconds: Option<i64>,
    pub adjustment_reason: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct CustomerWorkRecordUpsertRequest {
    pub confirmed_customer_id: Uuid,
    pub confirmed_started_at: DateTime<Utc>,
    pub confirmed_ended_at: DateTime<Utc>,
    pub customer_reference: Option<String>,
    pub notes: Option<String>,
}

impl From<CustomerWorkRecordUpsertRequest> for CustomerWorkRecordInput {
    fn from(value: CustomerWorkRecordUpsertRequest) -> Self {
        Self {
            confirmed_customer_id: value.confirmed_customer_id,
            confirmed_started_at: value.confirmed_started_at,
            confirmed_ended_at: value.confirmed_ended_at,
            customer_reference: normalize_optional(value.customer_reference),
            notes: normalize_optional(value.notes),
        }
    }
}

pub fn routes() -> Router<Arc<AppContext>> {
    Router::new()
        .route("/customers", get(list_customers).post(create_customer))
        .route("/customers/{customer_id}", put(update_customer))
        .route("/staffing/rates", get(list_rates))
        .route("/staffing/jobs", get(list_jobs))
        .route("/staffing/staff", get(list_staff))
        .route("/staffing/prices", post(set_prices))
        .route(
            "/staffing/eligibilities",
            get(list_eligibilities).post(create_eligibility),
        )
        .route("/staffing/shifts", get(list_shifts).post(create_shift))
        .route(
            "/staffing/shifts/{shift_id}/assignments",
            get(list_shift_assignments).post(create_shift_assignment),
        )
        .route("/staffing/shifts/{shift_id}/candidates", get(list_shift_candidates))
        .route("/staffing/reconciliations", get(list_reconciliations))
        .route(
            "/staffing/assignments/{assignment_id}/customer-record",
            put(upsert_customer_work_record),
        )
        .route(
            "/staffing/assignments/{assignment_id}/approve",
            post(approve_shift_assignment),
        )
        .route(
            "/staffing/assignments/{assignment_id}/reconcile",
            post(reconcile_shift_assignment),
        )
        .route(
            "/staffing/assignments/{assignment_id}/accept-staff-record",
            post(accept_staff_work_record),
        )
}

pub async fn list_customers(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(query): Query<CustomerPageQuery>,
) -> Result<Json<CustomerPageResponse>, StatusCode> {
    require_permission(&user, "business.customers.read")?;
    let limit: u16 = resolve_limit(&context.list_pagination, query.limit)?;
    let cursor: Option<CustomerCursor> = decode_cursor(query.cursor.as_deref())?;
    let page: CustomerPage = context
        .core
        .staffing
        .list_customers(user.tenant_id, normalize_search(query.search), i64::from(limit), cursor)
        .await
        .map_err(|error| staffing_status("list customers", &user, error))?;
    let next_cursor: Option<String> = encode_cursor(page.next_cursor.as_ref())?;
    Ok(Json(CustomerPageResponse {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

pub async fn create_customer(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<CustomerUpsertRequest>,
) -> Result<(StatusCode, Json<Customer>), StatusCode> {
    require_permission(&user, "business.customers.manage")?;
    let customer: Customer = context
        .core
        .staffing
        .create_customer(user.tenant_id, payload.into(), user.account_id)
        .await
        .map_err(|error| staffing_status("create customer", &user, error))?;
    Ok((StatusCode::CREATED, Json(customer)))
}

pub async fn update_customer(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(customer_id): Path<Uuid>,
    Json(payload): Json<CustomerUpsertRequest>,
) -> Result<Json<Customer>, StatusCode> {
    require_permission(&user, "business.customers.manage")?;
    let customer: Customer = context
        .core
        .staffing
        .update_customer(user.tenant_id, customer_id, payload.into(), user.account_id)
        .await
        .map_err(|error: StaffingError| staffing_status("update customer", &user, error))?;
    Ok(Json(customer))
}

pub async fn list_rates(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(query): Query<StaffingRatePageQuery>,
) -> Result<Json<StaffingRatePageResponse>, StatusCode> {
    require_permission(&user, "business.staffing_rates.read")?;
    let limit: u16 = resolve_limit(&context.list_pagination, query.limit)?;
    let cursor: Option<StaffingRateCursor> = decode_cursor(query.cursor.as_deref())?;
    let page: StaffingRatePage = context
        .core
        .staffing
        .list_rates(user.tenant_id, query.customer_id, i64::from(limit), cursor)
        .await
        .map_err(|error| staffing_status("list staffing rates", &user, error))?;
    let next_cursor: Option<String> = encode_cursor(page.next_cursor.as_ref())?;
    Ok(Json(StaffingRatePageResponse {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

pub async fn list_jobs(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<StaffingJob>>, StatusCode> {
    require_permission(&user, "business.staffing_jobs.read")?;
    context
        .core
        .staffing
        .list_jobs(user.tenant_id)
        .await
        .map(Json)
        .map_err(|error| staffing_status("list staffing jobs", &user, error))
}

pub async fn list_staff(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(query): Query<StaffingStaffPageQuery>,
) -> Result<Json<StaffingStaffPageResponse>, StatusCode> {
    require_permission(&user, "business.staffing_rates.read")?;
    let limit: u16 = resolve_limit(&context.list_pagination, query.limit)?;
    let cursor: Option<StaffingStaffCursor> = decode_cursor(query.cursor.as_deref())?;
    let page: StaffingStaffPage = context
        .core
        .staffing
        .list_staff(user.tenant_id, normalize_search(query.search), i64::from(limit), cursor)
        .await
        .map_err(|error| staffing_status("list staffing staff", &user, error))?;
    let next_cursor: Option<String> = encode_cursor(page.next_cursor.as_ref())?;
    Ok(Json(StaffingStaffPageResponse {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

pub async fn set_prices(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<StaffingPriceSetRequest>,
) -> Result<(StatusCode, Json<StaffingPriceSet>), StatusCode> {
    require_permission(&user, "business.staffing_rates.manage")?;
    let prices: StaffingPriceSet = context
        .core
        .staffing
        .set_prices(user.tenant_id, payload.into(), user.account_id)
        .await
        .map_err(|error| staffing_status("set staffing prices", &user, error))?;
    Ok((StatusCode::CREATED, Json(prices)))
}

pub async fn list_eligibilities(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<StaffingEligibility>>, StatusCode> {
    require_permission(&user, "business.staffing_eligibility.read")?;
    context
        .core
        .staffing
        .list_eligibilities(user.tenant_id)
        .await
        .map(Json)
        .map_err(|error: StaffingError| staffing_status("list staffing eligibilities", &user, error))
}

pub async fn create_eligibility(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<StaffingEligibilityCreateRequest>,
) -> Result<(StatusCode, Json<StaffingEligibility>), StatusCode> {
    require_permission(&user, "business.staffing_eligibility.manage")?;
    let eligibility: StaffingEligibility = context
        .core
        .staffing
        .create_eligibility(user.tenant_id, payload.into(), user.account_id)
        .await
        .map_err(|error: StaffingError| staffing_status("create staffing eligibility", &user, error))?;
    Ok((StatusCode::CREATED, Json(eligibility)))
}

pub async fn list_shifts(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<StaffingShift>>, StatusCode> {
    require_permission(&user, "business.shifts.read")?;
    context
        .core
        .staffing
        .list_shifts(user.tenant_id)
        .await
        .map(Json)
        .map_err(|error| staffing_status("list shifts", &user, error))
}

pub async fn create_shift(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<StaffingShiftCreateRequest>,
) -> Result<(StatusCode, Json<StaffingShift>), StatusCode> {
    require_permission(&user, "business.shifts.manage")?;
    let shift: StaffingShift = context
        .core
        .staffing
        .create_shift(user.tenant_id, payload.into(), user.account_id)
        .await
        .map_err(|error| staffing_status("create shift", &user, error))?;
    Ok((StatusCode::CREATED, Json(shift)))
}

pub async fn list_shift_assignments(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(shift_id): Path<Uuid>,
) -> Result<Json<Vec<ShiftAssignment>>, StatusCode> {
    require_permission(&user, "business.shifts.read")?;
    context
        .core
        .staffing
        .list_shift_assignments(user.tenant_id, shift_id)
        .await
        .map(Json)
        .map_err(|error| staffing_status("list shift assignments", &user, error))
}

pub async fn list_shift_candidates(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(shift_id): Path<Uuid>,
) -> Result<Json<Vec<StaffingCandidate>>, StatusCode> {
    require_permission(&user, "business.shifts.read")?;
    context
        .core
        .staffing
        .list_shift_candidates(user.tenant_id, shift_id)
        .await
        .map(Json)
        .map_err(|error| staffing_status("list shift candidates", &user, error))
}

pub async fn create_shift_assignment(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(shift_id): Path<Uuid>,
    Json(payload): Json<ShiftAssignmentCreateRequest>,
) -> Result<(StatusCode, Json<ShiftAssignment>), StatusCode> {
    require_permission(&user, "business.shifts.manage")?;
    let assignment: ShiftAssignment = context
        .core
        .staffing
        .create_shift_assignment(user.tenant_id, shift_id, payload.into(), user.account_id)
        .await
        .map_err(|error| staffing_status("create shift assignment", &user, error))?;
    Ok((StatusCode::CREATED, Json(assignment)))
}

pub async fn list_reconciliations(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Query(query): Query<ReconciliationPageQuery>,
) -> Result<Json<StaffingReconciliationPageResponse>, StatusCode> {
    require_permission(&user, "business.reconciliation.read")?;
    let pagination = &context.list_pagination;
    let limit: u16 = query.limit.unwrap_or(pagination.default_limit);
    if !(pagination.minimum_limit..=pagination.maximum_limit).contains(&limit) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let cursor: Option<StaffingReconciliationCursor> = query
        .cursor
        .as_deref()
        .map(decode_staffing_reconciliation_cursor)
        .transpose()?;
    let page: StaffingReconciliationPage = context
        .core
        .staffing
        .list_reconciliations(user.tenant_id, query.customer_id, i64::from(limit), cursor)
        .await
        .map_err(|error| staffing_status("list staffing reconciliations", &user, error))?;
    let next_cursor: Option<String> = page
        .next_cursor
        .as_ref()
        .map(encode_staffing_reconciliation_cursor)
        .transpose()?;
    Ok(Json(StaffingReconciliationPageResponse {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

fn decode_staffing_reconciliation_cursor(value: &str) -> Result<StaffingReconciliationCursor, StatusCode> {
    let bytes: Vec<u8> = URL_SAFE_NO_PAD.decode(value).map_err(|_| StatusCode::BAD_REQUEST)?;
    serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_REQUEST)
}

fn encode_staffing_reconciliation_cursor(cursor: &StaffingReconciliationCursor) -> Result<String, StatusCode> {
    let bytes: Vec<u8> = serde_json::to_vec(cursor).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub async fn upsert_customer_work_record(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(assignment_id): Path<Uuid>,
    Json(payload): Json<CustomerWorkRecordUpsertRequest>,
) -> Result<Json<CustomerWorkRecord>, StatusCode> {
    require_permission(&user, "business.reconciliation.manage")?;
    context
        .core
        .staffing
        .upsert_customer_work_record(user.tenant_id, assignment_id, payload.into(), user.account_id)
        .await
        .map(Json)
        .map_err(|error| staffing_status("record customer staffing evidence", &user, error))
}

pub async fn reconcile_shift_assignment(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(assignment_id): Path<Uuid>,
    Json(payload): Json<ShiftAssignmentApproveRequest>,
) -> Result<Json<ShiftAssignment>, StatusCode> {
    require_permission(&user, "business.reconciliation.manage")?;
    reconcile(context, user, assignment_id, payload).await
}

pub async fn accept_staff_work_record(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(assignment_id): Path<Uuid>,
) -> Result<Json<ShiftAssignment>, StatusCode> {
    require_permission(&user, "business.reconciliation.manage")?;
    context
        .core
        .staffing
        .accept_staff_work_record(user.tenant_id, assignment_id, user.account_id)
        .await
        .map(Json)
        .map_err(|error| staffing_status("accept staff work record", &user, error))
}

pub async fn approve_shift_assignment(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(assignment_id): Path<Uuid>,
    Json(payload): Json<ShiftAssignmentApproveRequest>,
) -> Result<Json<ShiftAssignment>, StatusCode> {
    require_permission(&user, "business.shifts.approve")?;
    require_permission(&user, "business.reconciliation.manage")?;
    reconcile(context, user, assignment_id, payload).await
}

async fn reconcile(
    context: Arc<AppContext>,
    user: AuthenticatedUser,
    assignment_id: Uuid,
    payload: ShiftAssignmentApproveRequest,
) -> Result<Json<ShiftAssignment>, StatusCode> {
    context
        .core
        .staffing
        .approve_shift_assignment(
            user.tenant_id,
            assignment_id,
            payload.worked_seconds,
            normalize_optional(payload.adjustment_reason),
            user.account_id,
        )
        .await
        .map(Json)
        .map_err(|error| staffing_status("approve shift assignment", &user, error))
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let normalized: String = value.trim().to_owned();
        (!normalized.is_empty()).then_some(normalized)
    })
}

fn require_permission(user: &AuthenticatedUser, permission: &str) -> Result<(), StatusCode> {
    if user.has_permission(permission) {
        Ok(())
    } else {
        info!(
            "Staffing request denied: tenant_id={} account_id={} required_permission={}",
            user.tenant_id, user.account_id, permission
        );
        Err(StatusCode::FORBIDDEN)
    }
}

fn staffing_status(operation: &str, user: &AuthenticatedUser, error: StaffingError) -> StatusCode {
    let status: StatusCode = match error {
        StaffingError::NotFound => StatusCode::NOT_FOUND,
        StaffingError::Conflict => StatusCode::CONFLICT,
        StaffingError::InvalidInput(message) => {
            warn!(
                "Staffing request input rejected: operation={} tenant_id={} account_id={} reason={}",
                operation, user.tenant_id, user.account_id, message
            );
            StatusCode::BAD_REQUEST
        }
        StaffingError::MissingStaffingRate => StatusCode::UNPROCESSABLE_ENTITY,
        StaffingError::BackendUnavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    if status.is_server_error() {
        error!(
            "Staffing request failed: operation={} tenant_id={} account_id={} status={}",
            operation, user.tenant_id, user.account_id, status
        );
    }
    status
}
