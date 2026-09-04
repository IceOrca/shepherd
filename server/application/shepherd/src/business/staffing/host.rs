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

use super::{
    CustomerWorkRecordInput, ManualRateOverride, ReconcileCollection, ReconcileStatus, ReconciliationCorrectionInput,
    ReconciliationRevision, ShiftAssignmentInput, StaffingErr, StaffingEligibilityInput, StaffingPriceSetInput,
    StaffingReconcileCursor, StaffingShiftInput, ManualRateOverrideRequest,
};

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

#[derive(Debug, Serialize)]
pub struct StaffingListPageResponse<T> {
    pub items: Vec<T>,
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

#[derive(Debug, Deserialize, TS)]
pub struct ReconciliationCorrectionReq {
    pub expected_revision_id: Uuid,
    pub worked_seconds: i64,
    pub correction_reason: String,
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

pub fn routes() -> Router<Arc<AppContext>> {
    Router::new()
        .route("/customers", get(list_customers).post(create_customer))
        .route("/customers/{customer_id}", put(update_customer))
        .route("/staffing/rates", get(list_rates))
        .route("/staffing/jobs", get(list_jobs))
        .route("/staffing/staff", get(list_staff))
        .route("/staffing/prices", post(set_prices))
        .route(
            "/staffing/assignments/{assignment_id}/reconciliation-corrections",
            post(correct_reconciliation),
        )
}

pub async fn correct_reconciliation(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(assignment_id): Path<Uuid>,
    Json(payload): Json<ReconciliationCorrectionReq>,
) -> Result<Json<ReconciliationRevision>, StatusCode> {
    require_permission(&user, "business.reconciliation.correct")?;
    ctx.core
        .staffing
        .correct_reconciliation(
            user.tenant_id,
            assignment_id,
            ReconciliationCorrectionInput {
                expected_revision_id: payload.expected_revision_id,
                worked_seconds: payload.worked_seconds,
                correction_reason: payload.correction_reason.trim().to_owned(),
            },
            user.account_id,
        )
        .await
        .map(Json)
        .map_err(|error| staffing_status("correct reconciliation", &user, error))
}

pub async fn list_customers(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Query(query): Query<CustomerPageQuery>,
) -> Result<Json<CustomerPageResponse>, StatusCode> {
    require_permission(&user, "business.customers.read")?;
    let limit: u16 = resolve_limit(&ctx.pagination, query.limit)?;
    let cursor: Option<CustomerCursor> = decode_cursor(query.cursor.as_deref())?;
    let page: CustomerPage = ctx
        .core
        .staffing
        .list_customers(user.tenant_id, normalize_search(query.search), i64::from(limit), cursor)
        .await
        .map_err(|err: StaffingErr| staffing_status("list customers", &user, err))?;
    let next_cursor: Option<String> = encode_cursor(page.next_cursor.as_ref())?;
    Ok(Json(CustomerPageResponse {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

pub async fn create_customer(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Json(payload): Json<CustomerUpsertRequest>,
) -> Result<(StatusCode, Json<Customer>), StatusCode> {
    require_permission(&user, "business.customers.manage")?;
    let customer: Customer = ctx
        .core
        .staffing
        .create_customer(user.tenant_id, payload.into(), user.account_id)
        .await
        .map_err(|error: StaffingErr| staffing_status("create customer", &user, error))?;
    Ok((StatusCode::CREATED, Json(customer)))
}

pub async fn update_customer(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(customer_id): Path<Uuid>,
    Json(payload): Json<CustomerUpsertRequest>,
) -> Result<Json<Customer>, StatusCode> {
    require_permission(&user, "business.customers.manage")?;
    let customer: Customer = ctx
        .core
        .staffing
        .update_customer(user.tenant_id, customer_id, payload.into(), user.account_id)
        .await
        .map_err(|error: StaffingErr| staffing_status("update customer", &user, error))?;
    Ok(Json(customer))
}

pub async fn list_rates(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Query(query): Query<StaffingRatePageQuery>,
) -> Result<Json<StaffingRatePageResponse>, StatusCode> {
    require_permission(&user, "business.staffing_rates.read")?;
    let limit: u16 = resolve_limit(&ctx.pagination, query.limit)?;
    let cursor: Option<StaffingRateCursor> = decode_cursor(query.cursor.as_deref())?;
    let page: StaffingRatePage = ctx
        .core
        .staffing
        .list_rates(user.tenant_id, query.customer_id, i64::from(limit), cursor)
        .await
        .map_err(|error: StaffingErr| staffing_status("list staffing rates", &user, error))?;
    let next_cursor: Option<String> = encode_cursor(page.next_cursor.as_ref())?;
    Ok(Json(StaffingRatePageResponse {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

pub async fn list_jobs(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Query(query): Query<CustomerPageQuery>,
) -> Result<Json<StaffingListPageResponse<StaffingJob>>, StatusCode> {
    require_permission(&user, "business.staffing_jobs.read")?;
    let limit: u16 = resolve_limit(&ctx.pagination, query.limit)?;
    let cursor: Option<NameCodeCursor> = decode_cursor(query.cursor.as_deref())?;
    let page: super::core::KeysetPage<StaffingJob, NameCodeCursor> = ctx
        .core
        .staffing
        .list_jobs(user.tenant_id, i64::from(limit), cursor)
        .await
        .map_err(|error: StaffingErr| staffing_status("list staffing jobs", &user, error))?;
    let next_cursor: Option<String> = encode_cursor(page.next_cursor.as_ref())?;
    Ok(Json(StaffingListPageResponse {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

pub async fn list_staff(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Query(query): Query<StaffingStaffPageQuery>,
) -> Result<Json<StaffingStaffPageResponse>, StatusCode> {
    require_permission(&user, "business.staffing_rates.read")?;
    let limit: u16 = resolve_limit(&ctx.pagination, query.limit)?;
    let cursor: Option<StaffingStaffCursor> = decode_cursor(query.cursor.as_deref())?;
    let page: StaffingStaffPage = ctx
        .core
        .staffing
        .list_staff(user.tenant_id, normalize_search(query.search), i64::from(limit), cursor)
        .await
        .map_err(|error: StaffingErr| staffing_status("list staffing staff", &user, error))?;
    let next_cursor: Option<String> = encode_cursor(page.next_cursor.as_ref())?;
    Ok(Json(StaffingStaffPageResponse {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

pub async fn set_prices(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Json(payload): Json<StaffingPriceSetRequest>,
) -> Result<(StatusCode, Json<StaffingPriceSet>), StatusCode> {
    require_permission(&user, "business.staffing_rates.manage")?;
    let prices: StaffingPriceSet = ctx
        .core
        .staffing
        .set_prices(user.tenant_id, payload.into(), user.account_id)
        .await
        .map_err(|error: StaffingErr| staffing_status("set staffing prices", &user, error))?;
    Ok((StatusCode::CREATED, Json(prices)))
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value: String| {
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
