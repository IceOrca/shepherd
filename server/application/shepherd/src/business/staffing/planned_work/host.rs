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

use crate::{AppContext, auth::AuthedUser};
use crate::pagination::{decode_cursor, encode_cursor, normalize_search, resolve_limit};

use super::core::{
    BusinessRecordStatus, Customer, CustomerCursor, CustomerInput, CustomerPage, CustomerWorkRecord, NameCodeCursor,
    ShiftAssignment, ShiftAssignmentCursor, StaffingCandidate, StaffingCandidateCursor, StaffingJob, StaffingPriceSet,
    StaffingRate, StaffingRateCursor, StaffingRatePage, StaffingReconcile, StaffingReconcilePage, StaffingShift,
    StaffingShiftCursor, StaffingStaff, StaffingStaffCursor, StaffingStaffPage,
};

use super::super::{
    host::StaffingListPageResponse, CustomerWorkRecordInput, ManualRateOverride, ReconcileCollection, ReconcileStatus,
    ShiftAssignmentInput, StaffingErr, StaffingEligibilityInput, StaffingPriceSetInput, StaffingReconcileCursor,
    StaffingShiftInput, ManualRateOverrideRequest,
};

#[derive(Debug, Deserialize)]
pub struct ReconcilePageQuery {
    pub limit: Option<u16>,
    pub cursor: Option<String>,
    pub customer_id: Option<Uuid>,
    pub collection: Option<ReconcileCollection>,
    pub period_start: Option<DateTime<Utc>>,
    pub period_end: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct PlannedListPageQuery {
    pub limit: Option<u16>,
    pub cursor: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Serialize, TS)]
pub struct StaffingReconcilePageRsp {
    pub items: Vec<StaffingReconcile>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub limit: u16,
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
#[ts(optional_fields = nullable)]
pub struct ShiftAssignmentCreateRequest {
    pub employee_id: Uuid,
    pub manual_rate: Option<ManualRateOverrideRequest>,
}

#[derive(Debug, Deserialize, TS)]
pub struct StaffingCancellationRequest {
    pub reason: String,
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
    pub final_customer_id: Option<Uuid>,
    pub final_job_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct CustomerWorkRecordUpsertReq {
    pub confirmed_customer_id: Uuid,
    pub confirmed_started_at: DateTime<Utc>,
    pub confirmed_ended_at: DateTime<Utc>,
    pub customer_reference: Option<String>,
    pub notes: Option<String>,
}

impl From<CustomerWorkRecordUpsertReq> for CustomerWorkRecordInput {
    fn from(value: CustomerWorkRecordUpsertReq) -> Self {
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
        .route("/staffing/shifts", get(list_shifts).post(create_shift))
        .route("/staffing/shifts/{shift_id}/cancel", post(cancel_shift))
        .route(
            "/staffing/shifts/{shift_id}/assignments",
            get(list_shift_assignments).post(create_shift_assignment),
        )
        .route("/staffing/shifts/{shift_id}/candidates", get(list_shift_candidates))
        .route(
            "/staffing/assignments/{assignment_id}/cancel",
            post(cancel_shift_assignment),
        )
        .route("/staffing/assignments/reconciliations", get(list_reconciliations))
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

pub async fn list_shifts(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Query(query): Query<PlannedListPageQuery>,
) -> Result<Json<StaffingListPageResponse<StaffingShift>>, StatusCode> {
    require_permission(&user, "business.shifts.read")?;
    let limit = resolve_limit(&ctx.pagination, query.limit)?;
    let cursor: Option<StaffingShiftCursor> = decode_cursor(query.cursor.as_deref())?;
    let page = ctx
        .core
        .planned_staffing
        .list_shifts(user.tenant_id, i64::from(limit), cursor)
        .await
        .map_err(|error| staffing_status("list shifts", &user, error))?;
    let next_cursor = encode_cursor(page.next_cursor.as_ref())?;
    Ok(Json(StaffingListPageResponse {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

pub async fn create_shift(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Json(payload): Json<StaffingShiftCreateRequest>,
) -> Result<(StatusCode, Json<StaffingShift>), StatusCode> {
    require_permission(&user, "business.shifts.manage")?;
    let shift: StaffingShift = ctx
        .core
        .planned_staffing
        .create_shift(user.tenant_id, payload.into(), user.account_id)
        .await
        .map_err(|error: StaffingErr| staffing_status("create shift", &user, error))?;
    Ok((StatusCode::CREATED, Json(shift)))
}

pub async fn list_shift_assignments(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(shift_id): Path<Uuid>,
    Query(query): Query<PlannedListPageQuery>,
) -> Result<Json<StaffingListPageResponse<ShiftAssignment>>, StatusCode> {
    require_permission(&user, "business.shifts.read")?;
    let limit = resolve_limit(&ctx.pagination, query.limit)?;
    let cursor: Option<ShiftAssignmentCursor> = decode_cursor(query.cursor.as_deref())?;
    let page = ctx
        .core
        .planned_staffing
        .list_shift_assignments(user.tenant_id, shift_id, i64::from(limit), cursor)
        .await
        .map_err(|error| staffing_status("list shift assignments", &user, error))?;
    let next_cursor = encode_cursor(page.next_cursor.as_ref())?;
    Ok(Json(StaffingListPageResponse {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

pub async fn list_shift_candidates(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(shift_id): Path<Uuid>,
    Query(query): Query<PlannedListPageQuery>,
) -> Result<Json<StaffingListPageResponse<StaffingCandidate>>, StatusCode> {
    require_permission(&user, "business.shifts.read")?;
    let limit: u16 = resolve_limit(&ctx.pagination, query.limit)?;
    let cursor: Option<StaffingCandidateCursor> = decode_cursor(query.cursor.as_deref())?;
    let page: super::core::KeysetPage<StaffingCandidate, StaffingCandidateCursor> = ctx
        .core
        .planned_staffing
        .list_shift_candidates(
            user.tenant_id,
            shift_id,
            normalize_search(query.search),
            i64::from(limit),
            cursor,
        )
        .await
        .map_err(|error: StaffingErr| staffing_status("list shift candidates", &user, error))?;
    let next_cursor: Option<String> = encode_cursor(page.next_cursor.as_ref())?;
    Ok(Json(StaffingListPageResponse {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

pub async fn create_shift_assignment(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(shift_id): Path<Uuid>,
    Json(payload): Json<ShiftAssignmentCreateRequest>,
) -> Result<(StatusCode, Json<ShiftAssignment>), StatusCode> {
    require_permission(&user, "business.shifts.manage")?;
    let assignment: ShiftAssignment = ctx
        .core
        .planned_staffing
        .create_shift_assignment(user.tenant_id, shift_id, payload.into(), user.account_id)
        .await
        .map_err(|error| staffing_status("create shift assignment", &user, error))?;
    Ok((StatusCode::CREATED, Json(assignment)))
}

pub async fn cancel_shift(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(shift_id): Path<Uuid>,
    Json(payload): Json<StaffingCancellationRequest>,
) -> Result<StatusCode, StatusCode> {
    require_permission(&user, "business.shifts.manage")?;
    ctx.core
        .planned_staffing
        .cancel_shift(user.tenant_id, shift_id, payload.reason, user.account_id)
        .await
        .map_err(|error| staffing_status("cancel staffing shift", &user, error))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn cancel_shift_assignment(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(assignment_id): Path<Uuid>,
    Json(payload): Json<StaffingCancellationRequest>,
) -> Result<StatusCode, StatusCode> {
    require_permission(&user, "business.shifts.manage")?;
    ctx.core
        .planned_staffing
        .cancel_shift_assignment(user.tenant_id, assignment_id, payload.reason, user.account_id)
        .await
        .map_err(|error| staffing_status("cancel staffing shift assignment", &user, error))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_reconciliations(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Query(query): Query<ReconcilePageQuery>,
) -> Result<Json<StaffingReconcilePageRsp>, StatusCode> {
    require_permission(&user, "business.reconciliation.read")?;
    let pagination = &ctx.pagination;
    let limit: u16 = query.limit.unwrap_or(pagination.def_limit);
    if !(pagination.min_limit..=pagination.max_limit).contains(&limit) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let cursor: Option<StaffingReconcileCursor> = query
        .cursor
        .as_deref()
        .map(decode_staffing_reconciliation_cursor)
        .transpose()?;
    let page: StaffingReconcilePage = ctx
        .core
        .planned_staffing
        .list_reconciliations(
            user.tenant_id,
            query.customer_id,
            query.collection.unwrap_or(ReconcileCollection::Pending),
            query.period_start,
            query.period_end,
            i64::from(limit),
            cursor,
        )
        .await
        .map_err(|error| staffing_status("list staffing reconciliations", &user, error))?;
    let next_cursor: Option<String> = page
        .next_cursor
        .as_ref()
        .map(encode_staffing_reconciliation_cursor)
        .transpose()?;
    Ok(Json(StaffingReconcilePageRsp {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

fn decode_staffing_reconciliation_cursor(value: &str) -> Result<StaffingReconcileCursor, StatusCode> {
    let bytes: Vec<u8> = URL_SAFE_NO_PAD.decode(value).map_err(|_| StatusCode::BAD_REQUEST)?;
    serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_REQUEST)
}

fn encode_staffing_reconciliation_cursor(cursor: &StaffingReconcileCursor) -> Result<String, StatusCode> {
    let bytes: Vec<u8> = serde_json::to_vec(cursor).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub async fn upsert_customer_work_record(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(assignment_id): Path<Uuid>,
    Json(payload): Json<CustomerWorkRecordUpsertReq>,
) -> Result<Json<CustomerWorkRecord>, StatusCode> {
    if !user.has_permission("business.reconciliation.manage") && !user.has_permission("business.reconciliation.correct")
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let allow_terminal_correction = user.has_permission("business.reconciliation.correct");
    ctx.core
        .planned_staffing
        .upsert_customer_work_record(
            user.tenant_id,
            assignment_id,
            payload.into(),
            user.account_id,
            allow_terminal_correction,
        )
        .await
        .map(Json)
        .map_err(|error| staffing_status("record customer staffing evidence", &user, error))
}

pub async fn reconcile_shift_assignment(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(assignment_id): Path<Uuid>,
    Json(payload): Json<ShiftAssignmentApproveRequest>,
) -> Result<Json<ShiftAssignment>, StatusCode> {
    require_permission(&user, "business.reconciliation.manage")?;
    reconcile(ctx, user, assignment_id, payload).await
}

pub async fn accept_staff_work_record(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(assignment_id): Path<Uuid>,
) -> Result<Json<ShiftAssignment>, StatusCode> {
    require_permission(&user, "business.reconciliation.manage")?;
    ctx.core
        .planned_staffing
        .accept_staff_work_record(user.tenant_id, assignment_id, user.account_id)
        .await
        .map(Json)
        .map_err(|error| staffing_status("accept staff work record", &user, error))
}

pub async fn approve_shift_assignment(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(assignment_id): Path<Uuid>,
    Json(payload): Json<ShiftAssignmentApproveRequest>,
) -> Result<Json<ShiftAssignment>, StatusCode> {
    require_permission(&user, "business.shifts.approve")?;
    require_permission(&user, "business.reconciliation.manage")?;
    reconcile(ctx, user, assignment_id, payload).await
}

async fn reconcile(
    ctx: Arc<AppContext>,
    user: AuthedUser,
    assignment_id: Uuid,
    payload: ShiftAssignmentApproveRequest,
) -> Result<Json<ShiftAssignment>, StatusCode> {
    ctx.core
        .planned_staffing
        .approve_shift_assignment(
            user.tenant_id,
            assignment_id,
            payload.worked_seconds,
            normalize_optional(payload.adjustment_reason),
            payload.final_customer_id,
            payload.final_job_id,
            user.account_id,
        )
        .await
        .map(Json)
        .map_err(|error: StaffingErr| staffing_status("approve shift assignment", &user, error))
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let normalized: String = value.trim().to_owned();
        (!normalized.is_empty()).then_some(normalized)
    })
}

fn require_permission(user: &AuthedUser, perm: &str) -> Result<(), StatusCode> {
    if user.has_permission(perm) {
        Ok(())
    } else {
        info!(
            "Staffing request denied: tenant_id={} account_id={} required_permission={}",
            user.tenant_id, user.account_id, perm
        );
        Err(StatusCode::FORBIDDEN)
    }
}

fn staffing_status(operation: &str, user: &AuthedUser, err: StaffingErr) -> StatusCode {
    let status: StatusCode = match err {
        StaffingErr::NotFound => StatusCode::NOT_FOUND,
        StaffingErr::Conflict => StatusCode::CONFLICT,
        StaffingErr::InvalidInput(message) => {
            warn!(
                "Staffing request input rejected: operation={} tenant_id={} account_id={} reason={}",
                operation, user.tenant_id, user.account_id, message
            );
            StatusCode::BAD_REQUEST
        }
        StaffingErr::MissingStaffingRate => StatusCode::UNPROCESSABLE_ENTITY,
        StaffingErr::BackendUnavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    if status.is_server_error() {
        error!(
            "Staffing request failed: operation={} tenant_id={} account_id={} status={}",
            operation, user.tenant_id, user.account_id, status
        );
    }
    status
}
