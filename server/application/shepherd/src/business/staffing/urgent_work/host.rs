use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    routing::{get, post, put},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, trace, warn};
use ts_rs::TS;
use uuid::Uuid;

use crate::{
    AppContext,
    auth::AuthedUser,
    pagination::{decode_cursor, encode_cursor, normalize_search, resolve_limit},
};

use super::super::{
    core::{ManualRateOverride, ReconcileCollection},
    host::ManualRateOverrideRequest,
};
use super::core::{
    UrgentCustomerCursor, UrgentCustomerPage, UrgentCustomerWorkRecord, UrgentCustomerWorkRecordInput,
    UrgentEmployeeCursor, UrgentEmployeePage, UrgentOwnWorkCursor, UrgentOwnWorkPage, UrgentReconcileCursor,
    UrgentReconcilePage, UrgentTeamWorkPage, UrgentWorkEmployee, UrgentWorkEndInput, UrgentWorkError,
    UrgentWorkCustomer, UrgentWorkItem, UrgentWorkLocationInput, UrgentWorkManualInput, UrgentWorkReconcileInput,
    UrgentWorkReconcile, UrgentWorkStartInput,
};

#[derive(Debug, Deserialize)]
pub struct UrgentSelectorPageQuery {
    pub limit: Option<u16>,
    pub cursor: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UrgentListPageRsp<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub limit: u16,
}

#[derive(Debug, Deserialize)]
pub struct UrgentOwnWorkPageQuery {
    pub limit: Option<u16>,
    pub cursor: Option<String>,
}

#[derive(Debug, Serialize, TS)]
pub struct UrgentOwnWorkPageRsp {
    pub items: Vec<UrgentWorkItem>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub limit: u16,
}

#[derive(Debug, Deserialize)]
pub struct UrgentReconcilePageQuery {
    pub limit: Option<u16>,
    pub cursor: Option<String>,
    pub customer_id: Option<Uuid>,
    pub collection: Option<ReconcileCollection>,
    pub period_start: Option<DateTime<Utc>>,
    pub period_end: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, TS)]
pub struct UrgentReconcileRsp {
    pub items: Vec<UrgentWorkReconcile>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub limit: u16,
}

#[derive(Debug, Deserialize, TS)]
pub struct UrgentWorkStartReq {
    pub customer_id: Uuid,
    pub employee_ids: Vec<Uuid>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub accuracy_meters: Option<f32>,
}

#[derive(Debug, Deserialize, TS)]
pub struct UrgentWorkEndReq {
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub accuracy_meters: Option<f32>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct UrgentWorkManualReq {
    pub customer_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct UrgentCustomerWorkRecordUpsertReq {
    pub confirmed_customer_id: Uuid,
    pub confirmed_started_at: DateTime<Utc>,
    pub confirmed_ended_at: DateTime<Utc>,
    pub customer_reference: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct UrgentWorkReconcileReq {
    pub final_customer_id: Uuid,
    pub job_id: Uuid,
    pub worked_seconds: i64,
    pub adjustment_reason: Option<String>,
    pub manual_rate: Option<ManualRateOverrideRequest>,
}

#[derive(Debug, Deserialize, TS)]
pub struct UrgentWorkAcceptStaffRecordReq {
    pub job_id: Uuid,
}

pub fn routes() -> Router<Arc<AppContext>> {
    info!("Configured urgent-first staffing routes");
    Router::new()
        .route("/staffing/urgent-work/customers", get(list_customers))
        .route("/staffing/urgent-work/employees", get(list_employees))
        .route("/staffing/urgent-work/me", get(list_own_work))
        .route("/staffing/urgent-work/team", get(list_team_work))
        .route("/staffing/urgent-work/start", post(start_work))
        .route("/staffing/urgent-work/manual", post(submit_manual_work))
        .route("/staffing/urgent-work/{report_id}/end", post(end_work))
        .route("/staffing/urgent-work/reconciliations", get(list_reconciliations))
        .route(
            "/staffing/urgent-work/{report_id}/customer-record",
            put(upsert_customer_record),
        )
        .route("/staffing/urgent-work/{report_id}/reconcile", post(reconcile))
        .route(
            "/staffing/urgent-work/{report_id}/accept-staff-record",
            post(accept_staff_record),
        )
}

async fn list_customers(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Query(query): Query<UrgentSelectorPageQuery>,
) -> Result<Json<UrgentListPageRsp<UrgentWorkCustomer>>, StatusCode> {
    require_any_permission(&user, &["business.urgent_work.read", "business.reconciliation.read"])?;
    let limit: u16 = resolve_limit(&ctx.list_pagination, query.limit)?;
    let cursor: Option<UrgentCustomerCursor> = decode_cursor(query.cursor.as_deref())?;
    let page: UrgentCustomerPage = ctx
        .core
        .urgent_work
        .list_customers(user.tenant_id, normalize_search(query.search), i64::from(limit), cursor)
        .await
        .map_err(|operation_error: UrgentWorkError| status("list urgent customers", &user, operation_error))?;
    let next_cursor: Option<String> = encode_cursor(page.next_cursor.as_ref())?;
    debug!(tenant_id = %user.tenant_id, account_id = %user.account_id, customer_count = page.items.len(), "Urgent customer request completed");
    Ok(Json(UrgentListPageRsp {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

async fn list_employees(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Query(query): Query<UrgentSelectorPageQuery>,
) -> Result<Json<UrgentListPageRsp<UrgentWorkEmployee>>, StatusCode> {
    require_permission(&user, "business.urgent_work.read")?;
    let limit: u16 = resolve_limit(&ctx.list_pagination, query.limit)?;
    let cursor: Option<UrgentEmployeeCursor> = decode_cursor(query.cursor.as_deref())?;
    let page: UrgentEmployeePage = ctx
        .core
        .urgent_work
        .list_employees(
            user.tenant_id,
            user.account_id,
            normalize_search(query.search),
            i64::from(limit),
            cursor,
        )
        .await
        .map_err(|operation_error: UrgentWorkError| status("list urgent employees", &user, operation_error))?;
    let next_cursor: Option<String> = encode_cursor(page.next_cursor.as_ref())?;
    debug!(tenant_id = %user.tenant_id, account_id = %user.account_id, employee_count = page.items.len(), "Urgent employee request completed");
    Ok(Json(UrgentListPageRsp {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

async fn list_own_work(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Query(query): Query<UrgentOwnWorkPageQuery>,
) -> Result<Json<UrgentOwnWorkPageRsp>, StatusCode> {
    require_permission(&user, "business.urgent_work.read")?;
    let limit: u16 = resolve_limit(&ctx.list_pagination, query.limit)?;
    let cursor: Option<UrgentOwnWorkCursor> = decode_cursor(query.cursor.as_deref())?;
    let page: UrgentOwnWorkPage = ctx
        .core
        .urgent_work
        .list_own_work(user.tenant_id, user.account_id, i64::from(limit), cursor)
        .await
        .map_err(|operation_error: UrgentWorkError| status("list own urgent work", &user, operation_error))?;
    let next_cursor: Option<String> = encode_cursor(page.next_cursor.as_ref())?;
    Ok(Json(UrgentOwnWorkPageRsp {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

async fn list_team_work(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Query(query): Query<UrgentOwnWorkPageQuery>,
) -> Result<Json<UrgentListPageRsp<UrgentWorkItem>>, StatusCode> {
    require_permission(&user, "business.urgent_work.peer_manage")?;
    let limit: u16 = resolve_limit(&ctx.list_pagination, query.limit)?;
    let cursor: Option<UrgentOwnWorkCursor> = decode_cursor(query.cursor.as_deref())?;
    let page: UrgentTeamWorkPage = ctx
        .core
        .urgent_work
        .list_team_work(user.tenant_id, user.account_id, i64::from(limit), cursor)
        .await
        .map_err(|operation_error: UrgentWorkError| status("list urgent team work", &user, operation_error))?;
    let next_cursor: Option<String> = encode_cursor(page.next_cursor.as_ref())?;
    Ok(Json(UrgentListPageRsp {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

async fn start_work(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    headers: HeaderMap,
    Json(request): Json<UrgentWorkStartReq>,
) -> Result<(StatusCode, Json<Vec<UrgentWorkItem>>), StatusCode> {
    require_permission(&user, "business.urgent_work.start")?;
    let allow_peer: bool = user.has_permission("business.urgent_work.peer_manage");
    let idempotency_key: Uuid = idempotency_key(&headers, &user)?;
    let target_count: usize = request.employee_ids.len();
    info!(tenant_id = %user.tenant_id, account_id = %user.account_id, customer_id = %request.customer_id, target_count, allow_peer, "Urgent-work start request accepted");
    let location: UrgentWorkLocationInput =
        location_input(request.latitude, request.longitude, request.accuracy_meters);
    let input: UrgentWorkStartInput = UrgentWorkStartInput {
        customer_id: request.customer_id,
        employee_ids: request.employee_ids,
        idempotency_key,
        location,
    };
    let work: Vec<UrgentWorkItem> = ctx
        .core
        .urgent_work
        .start(user.tenant_id, user.account_id, allow_peer, input)
        .await
        .map_err(|operation_error: UrgentWorkError| status("start urgent work", &user, operation_error))?;
    ctx.notifications.wake();
    info!(tenant_id = %user.tenant_id, account_id = %user.account_id, report_count = work.len(), "Urgent-work start request completed");
    Ok((StatusCode::CREATED, Json(work)))
}

async fn end_work(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(report_id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<UrgentWorkEndReq>,
) -> Result<Json<UrgentWorkItem>, StatusCode> {
    require_permission(&user, "business.urgent_work.start")?;
    let allow_peer: bool = user.has_permission("business.urgent_work.peer_manage");
    let idempotency_key: Uuid = idempotency_key(&headers, &user)?;
    info!(tenant_id = %user.tenant_id, account_id = %user.account_id, report_id = %report_id, allow_peer, "Urgent-work end request accepted");
    let input: UrgentWorkEndInput = UrgentWorkEndInput {
        idempotency_key,
        location: location_input(request.latitude, request.longitude, request.accuracy_meters),
    };
    let work: UrgentWorkItem = ctx
        .core
        .urgent_work
        .end(user.tenant_id, user.account_id, allow_peer, report_id, input)
        .await
        .map_err(|operation_error: UrgentWorkError| status("end urgent work", &user, operation_error))?;
    ctx.notifications.wake();
    info!(tenant_id = %user.tenant_id, account_id = %user.account_id, report_id = %report_id, worked_seconds = ?work.worked_seconds, "Urgent-work end request completed");
    Ok(Json(work))
}

async fn submit_manual_work(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    headers: HeaderMap,
    Json(request): Json<UrgentWorkManualReq>,
) -> Result<(StatusCode, Json<UrgentWorkItem>), StatusCode> {
    require_permission(&user, "business.urgent_work.start")?;
    let idempotency_key: Uuid = idempotency_key(&headers, &user)?;
    let input: UrgentWorkManualInput = UrgentWorkManualInput {
        customer_id: request.customer_id,
        started_at: request.started_at,
        ended_at: request.ended_at,
        note: normalize_optional(request.note),
        idempotency_key,
    };
    let work: UrgentWorkItem = ctx
        .core
        .urgent_work
        .submit_manual(user.tenant_id, user.account_id, input)
        .await
        .map_err(|operation_error: UrgentWorkError| status("submit manual urgent work", &user, operation_error))?;
    ctx.notifications.wake();
    Ok((StatusCode::CREATED, Json(work)))
}

async fn list_reconciliations(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Query(query): Query<UrgentReconcilePageQuery>,
) -> Result<Json<UrgentReconcileRsp>, StatusCode> {
    require_permission(&user, "business.reconciliation.read")?;
    let pagination = &ctx.list_pagination;
    let limit: u16 = query.limit.unwrap_or(pagination.default_limit);
    if !(pagination.minimum_limit..=pagination.maximum_limit).contains(&limit) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let cursor: Option<UrgentReconcileCursor> = query
        .cursor
        .as_deref()
        .map(decode_urgent_reconciliation_cursor)
        .transpose()?;
    let page: UrgentReconcilePage = ctx
        .core
        .urgent_work
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
        .map_err(|operation_error: UrgentWorkError| status("list urgent reconciliations", &user, operation_error))?;
    let next_cursor: Option<String> = page
        .next_cursor
        .as_ref()
        .map(encode_urgent_reconciliation_cursor)
        .transpose()?;
    Ok(Json(UrgentReconcileRsp {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

fn decode_urgent_reconciliation_cursor(value: &str) -> Result<UrgentReconcileCursor, StatusCode> {
    let bytes: Vec<u8> = URL_SAFE_NO_PAD.decode(value).map_err(|_| StatusCode::BAD_REQUEST)?;
    serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_REQUEST)
}

fn encode_urgent_reconciliation_cursor(cursor: &UrgentReconcileCursor) -> Result<String, StatusCode> {
    let bytes: Vec<u8> = serde_json::to_vec(cursor).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

async fn upsert_customer_record(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(report_id): Path<Uuid>,
    Json(request): Json<UrgentCustomerWorkRecordUpsertReq>,
) -> Result<Json<UrgentCustomerWorkRecord>, StatusCode> {
    if !user.has_permission("business.urgent_work.reconcile") && !user.has_permission("business.reconciliation.correct")
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let allow_terminal_correction = user.has_permission("business.reconciliation.correct");
    let input: UrgentCustomerWorkRecordInput = UrgentCustomerWorkRecordInput {
        confirmed_customer_id: request.confirmed_customer_id,
        confirmed_started_at: request.confirmed_started_at,
        confirmed_ended_at: request.confirmed_ended_at,
        customer_reference: normalize_optional(request.customer_reference),
        notes: normalize_optional(request.notes),
    };
    let record: UrgentCustomerWorkRecord = ctx
        .core
        .urgent_work
        .upsert_customer_record(
            user.tenant_id,
            user.account_id,
            report_id,
            input,
            allow_terminal_correction,
        )
        .await
        .map_err(|operation_error: UrgentWorkError| status("save urgent customer evidence", &user, operation_error))?;
    Ok(Json(record))
}

async fn reconcile(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(report_id): Path<Uuid>,
    Json(request): Json<UrgentWorkReconcileReq>,
) -> Result<Json<UrgentWorkReconcile>, StatusCode> {
    require_permission(&user, "business.urgent_work.reconcile")?;
    let manual_rate: Option<ManualRateOverride> = request.manual_rate.map(ManualRateOverride::from);
    let input: UrgentWorkReconcileInput = UrgentWorkReconcileInput {
        final_customer_id: request.final_customer_id,
        job_id: request.job_id,
        worked_seconds: request.worked_seconds,
        adjustment_reason: normalize_optional(request.adjustment_reason),
        manual_rate,
    };
    let result: UrgentWorkReconcile = ctx
        .core
        .urgent_work
        .reconcile(user.tenant_id, user.account_id, report_id, input)
        .await
        .map_err(|operation_error: UrgentWorkError| status("reconcile urgent work", &user, operation_error))?;
    Ok(Json(result))
}

async fn accept_staff_record(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(report_id): Path<Uuid>,
    Json(request): Json<UrgentWorkAcceptStaffRecordReq>,
) -> Result<Json<UrgentWorkReconcile>, StatusCode> {
    require_permission(&user, "business.urgent_work.reconcile")?;
    let result: UrgentWorkReconcile = ctx
        .core
        .urgent_work
        .accept_staff_record(user.tenant_id, user.account_id, report_id, request.job_id)
        .await
        .map_err(|operation_error: UrgentWorkError| {
            status("accept urgent staff work record", &user, operation_error)
        })?;
    Ok(Json(result))
}

fn location_input(
    latitude: Option<f64>,
    longitude: Option<f64>,
    accuracy_meters: Option<f32>,
) -> UrgentWorkLocationInput {
    let gps_enabled: bool = std::env::var("STAFFING_GPS_ENABLED")
        .ok()
        .is_some_and(|value: String| value.eq_ignore_ascii_case("true"));
    let supplied: bool = latitude.is_some() || longitude.is_some() || accuracy_meters.is_some();
    trace!(
        gps_enabled,
        supplied,
        retained = gps_enabled && supplied,
        "Prepared urgent-work location without logging coordinates"
    );
    filter_location(gps_enabled, latitude, longitude, accuracy_meters)
}

fn filter_location(
    gps_enabled: bool,
    latitude: Option<f64>,
    longitude: Option<f64>,
    accuracy_meters: Option<f32>,
) -> UrgentWorkLocationInput {
    if gps_enabled {
        UrgentWorkLocationInput {
            latitude,
            longitude,
            accuracy_meters,
        }
    } else {
        UrgentWorkLocationInput {
            latitude: None,
            longitude: None,
            accuracy_meters: None,
        }
    }
}

fn idempotency_key(headers: &HeaderMap, user: &AuthedUser) -> Result<Uuid, StatusCode> {
    let raw_header: Option<&HeaderValue> = headers.get("idempotency-key");
    let parsed: Option<Uuid> = raw_header
        .and_then(|value: &HeaderValue| value.to_str().ok())
        .and_then(|value: &str| Uuid::parse_str(value).ok());
    parsed.ok_or_else(|| {
        warn!(tenant_id = %user.tenant_id, account_id = %user.account_id, header_present = raw_header.is_some(), "Urgent-work request rejected without valid idempotency key");
        StatusCode::BAD_REQUEST
    })
}

fn require_permission(user: &AuthedUser, permission: &str) -> Result<(), StatusCode> {
    if user.has_permission(permission) {
        trace!(tenant_id = %user.tenant_id, account_id = %user.account_id, permission, "Urgent-work permission accepted");
        Ok(())
    } else {
        warn!(tenant_id = %user.tenant_id, account_id = %user.account_id, permission, "Urgent-work permission rejected");
        Err(StatusCode::FORBIDDEN)
    }
}

fn require_any_permission(user: &AuthedUser, permissions: &[&str]) -> Result<(), StatusCode> {
    if permissions
        .iter()
        .any(|permission: &&str| user.has_permission(permission))
    {
        trace!(
            tenant_id = %user.tenant_id,
            account_id = %user.account_id,
            permission_count = permissions.len(),
            "Urgent-work alternative permission set accepted"
        );
        Ok(())
    } else {
        warn!(
            tenant_id = %user.tenant_id,
            account_id = %user.account_id,
            permission_count = permissions.len(),
            "Urgent-work alternative permission set rejected"
        );
        Err(StatusCode::FORBIDDEN)
    }
}

fn status(operation: &str, user: &AuthedUser, operation_error: UrgentWorkError) -> StatusCode {
    let response_status: StatusCode = match operation_error {
        UrgentWorkError::NotFound => StatusCode::NOT_FOUND,
        UrgentWorkError::Forbidden => StatusCode::FORBIDDEN,
        UrgentWorkError::Conflict => StatusCode::CONFLICT,
        UrgentWorkError::InvalidInput(_) => StatusCode::UNPROCESSABLE_ENTITY,
        UrgentWorkError::MissingStaffingRate => StatusCode::UNPROCESSABLE_ENTITY,
        UrgentWorkError::BackendUnavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    if response_status.is_server_error() {
        error!(operation, tenant_id = %user.tenant_id, account_id = %user.account_id, status = %response_status, reason = ?operation_error, "Urgent-work request failed unexpectedly");
    } else {
        warn!(operation, tenant_id = %user.tenant_id, account_id = %user.account_id, status = %response_status, reason = ?operation_error, "Urgent-work request rejected");
    }
    response_status
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|text: String| text.trim().to_owned())
        .filter(|text: &String| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::filter_location;
    use crate::business::staffing::urgent_work::core::UrgentWorkLocationInput;

    #[test]
    fn gps_disabled_discards_supplied_coordinates() {
        let location: UrgentWorkLocationInput = filter_location(false, Some(10.77), Some(106.69), Some(4.0));
        assert!(location.latitude.is_none());
        assert!(location.longitude.is_none());
        assert!(location.accuracy_meters.is_none());
    }
}
