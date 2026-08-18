use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::{NaiveDate, NaiveTime};
use serde::Deserialize;
use tracing::{error, warn, info, debug, trace};
use crate::features::payroll::core::{
    EmployeeCompensation, EmployeeCompensationInput, FacilityRateRule, FacilityRateRuleInput, OvertimeRule,
    OvertimeRuleInput, PayBasis, PayrollError, PayrollRun, TimeBandRule, TimeBandRuleInput,
};
use ts_rs::TS;
use uuid::Uuid;

use crate::{AppContext, auth::AuthenticatedUser};

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct EmployeeCompensationCreateRequest {
    pub currency: String,
    pub pay_basis: PayBasis,
    pub hourly_rate: Option<String>,
    pub monthly_rate: Option<String>,
    pub standard_monthly_hours: Option<String>,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
}

impl From<EmployeeCompensationCreateRequest> for EmployeeCompensationInput {
    fn from(value: EmployeeCompensationCreateRequest) -> Self {
        Self {
            currency: value.currency.trim().to_ascii_uppercase(),
            pay_basis: value.pay_basis,
            hourly_rate: normalize_optional_decimal(value.hourly_rate),
            monthly_rate: normalize_optional_decimal(value.monthly_rate),
            standard_monthly_hours: normalize_optional_decimal(value.standard_monthly_hours),
            effective_from: value.effective_from,
            effective_to: value.effective_to,
        }
    }
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct FacilityRateRuleCreateRequest {
    pub code: String,
    pub name: String,
    pub facility_id: Uuid,
    pub employee_id: Option<Uuid>,
    pub base_multiplier: String,
    pub hourly_adjustment: String,
    pub priority: i16,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub is_active: bool,
}

impl From<FacilityRateRuleCreateRequest> for FacilityRateRuleInput {
    fn from(value: FacilityRateRuleCreateRequest) -> Self {
        Self {
            code: value.code.trim().to_ascii_lowercase(),
            name: value.name.trim().to_owned(),
            facility_id: value.facility_id,
            employee_id: value.employee_id,
            base_multiplier: value.base_multiplier.trim().to_owned(),
            hourly_adjustment: value.hourly_adjustment.trim().to_owned(),
            priority: value.priority,
            effective_from: value.effective_from,
            effective_to: value.effective_to,
            is_active: value.is_active,
        }
    }
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct TimeBandRuleCreateRequest {
    pub code: String,
    pub name: String,
    pub weekdays: Vec<i16>,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub spans_next_day: bool,
    pub premium_multiplier: String,
    pub hourly_adjustment: String,
    pub priority: i16,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub is_active: bool,
}

impl From<TimeBandRuleCreateRequest> for TimeBandRuleInput {
    fn from(value: TimeBandRuleCreateRequest) -> Self {
        Self {
            code: value.code.trim().to_ascii_lowercase(),
            name: value.name.trim().to_owned(),
            weekdays: value.weekdays,
            start_time: value.start_time,
            end_time: value.end_time,
            spans_next_day: value.spans_next_day,
            premium_multiplier: value.premium_multiplier.trim().to_owned(),
            hourly_adjustment: value.hourly_adjustment.trim().to_owned(),
            priority: value.priority,
            effective_from: value.effective_from,
            effective_to: value.effective_to,
            is_active: value.is_active,
        }
    }
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct OvertimeRuleCreateRequest {
    pub code: String,
    pub name: String,
    pub threshold_minutes: i32,
    pub premium_multiplier: String,
    pub hourly_adjustment: String,
    pub priority: i16,
    pub effective_from: NaiveDate,
    pub effective_to: Option<NaiveDate>,
    pub is_active: bool,
}

impl From<OvertimeRuleCreateRequest> for OvertimeRuleInput {
    fn from(value: OvertimeRuleCreateRequest) -> Self {
        Self {
            code: value.code.trim().to_ascii_lowercase(),
            name: value.name.trim().to_owned(),
            threshold_minutes: value.threshold_minutes,
            premium_multiplier: value.premium_multiplier.trim().to_owned(),
            hourly_adjustment: value.hourly_adjustment.trim().to_owned(),
            priority: value.priority,
            effective_from: value.effective_from,
            effective_to: value.effective_to,
            is_active: value.is_active,
        }
    }
}

#[derive(Debug, Deserialize, TS)]
pub struct PayrollCalculateRequest {
    pub year: i32,
    pub month: u32,
    pub time_zone: String,
    pub currency: String,
}

pub fn routes() -> Router<Arc<AppContext>> {
    Router::new()
        .route(
            "/employees/{employee_id}/compensations",
            get(list_compensations).post(create_compensation),
        )
        .route("/facility-rules", get(list_facility_rules).post(create_facility_rule))
        .route(
            "/time-band-rules",
            get(list_time_band_rules).post(create_time_band_rule),
        )
        .route("/overtime-rules", get(list_overtime_rules).post(create_overtime_rule))
        .route("/runs", get(list_runs).post(calculate_run))
        .route("/runs/{run_id}", get(get_run))
        .route("/runs/{run_id}/approve", post(approve_run))
}

pub async fn list_compensations(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(employee_id): Path<Uuid>,
) -> Result<Json<Vec<EmployeeCompensation>>, StatusCode> {
    require_permission(&user, "payroll.config.read")?;
    host.core
        .payroll
        .list_compensations(user.tenant_id, employee_id)
        .await
        .map(Json)
        .map_err(|error| payroll_status("list compensations", &user, error))
}

pub async fn create_compensation(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(employee_id): Path<Uuid>,
    Json(payload): Json<EmployeeCompensationCreateRequest>,
) -> Result<(StatusCode, Json<EmployeeCompensation>), StatusCode> {
    require_permission(&user, "payroll.config.manage")?;
    let compensation: EmployeeCompensation = host
        .core
        .payroll
        .create_compensation(user.tenant_id, employee_id, payload.into(), user.account_id)
        .await
        .map_err(|error| payroll_status("create compensation", &user, error))?;
    Ok((StatusCode::CREATED, Json(compensation)))
}

pub async fn list_facility_rules(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<FacilityRateRule>>, StatusCode> {
    require_permission(&user, "payroll.config.read")?;
    host.core
        .payroll
        .list_facility_rules(user.tenant_id)
        .await
        .map(Json)
        .map_err(|error| payroll_status("list facility rules", &user, error))
}

pub async fn create_facility_rule(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<FacilityRateRuleCreateRequest>,
) -> Result<(StatusCode, Json<FacilityRateRule>), StatusCode> {
    require_permission(&user, "payroll.config.manage")?;
    let rule: FacilityRateRule = host
        .core
        .payroll
        .create_facility_rule(user.tenant_id, payload.into(), user.account_id)
        .await
        .map_err(|error| payroll_status("create facility rule", &user, error))?;
    Ok((StatusCode::CREATED, Json(rule)))
}

pub async fn list_time_band_rules(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<TimeBandRule>>, StatusCode> {
    require_permission(&user, "payroll.config.read")?;
    host.core
        .payroll
        .list_time_band_rules(user.tenant_id)
        .await
        .map(Json)
        .map_err(|error| payroll_status("list time band rules", &user, error))
}

pub async fn create_time_band_rule(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<TimeBandRuleCreateRequest>,
) -> Result<(StatusCode, Json<TimeBandRule>), StatusCode> {
    require_permission(&user, "payroll.config.manage")?;
    let rule: TimeBandRule = host
        .core
        .payroll
        .create_time_band_rule(user.tenant_id, payload.into(), user.account_id)
        .await
        .map_err(|error| payroll_status("create time band rule", &user, error))?;
    Ok((StatusCode::CREATED, Json(rule)))
}

pub async fn list_overtime_rules(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<OvertimeRule>>, StatusCode> {
    require_permission(&user, "payroll.config.read")?;
    host.core
        .payroll
        .list_overtime_rules(user.tenant_id)
        .await
        .map(Json)
        .map_err(|error| payroll_status("list overtime rules", &user, error))
}

pub async fn create_overtime_rule(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<OvertimeRuleCreateRequest>,
) -> Result<(StatusCode, Json<OvertimeRule>), StatusCode> {
    require_permission(&user, "payroll.config.manage")?;
    let rule: OvertimeRule = host
        .core
        .payroll
        .create_overtime_rule(user.tenant_id, payload.into(), user.account_id)
        .await
        .map_err(|error| payroll_status("create overtime rule", &user, error))?;
    Ok((StatusCode::CREATED, Json(rule)))
}

pub async fn list_runs(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<PayrollRun>>, StatusCode> {
    require_permission(&user, "payroll.runs.read")?;
    host.core
        .payroll
        .list_runs(user.tenant_id)
        .await
        .map(Json)
        .map_err(|error| payroll_status("list payroll runs", &user, error))
}

pub async fn calculate_run(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<PayrollCalculateRequest>,
) -> Result<(StatusCode, Json<PayrollRun>), StatusCode> {
    require_permission(&user, "payroll.runs.manage")?;
    let run: PayrollRun = host
        .core
        .payroll
        .calculate_month(
            user.tenant_id,
            payload.year,
            payload.month,
            payload.time_zone,
            payload.currency,
            user.account_id,
        )
        .await
        .map_err(|error| payroll_status("calculate payroll run", &user, error))?;
    Ok((StatusCode::CREATED, Json(run)))
}

pub async fn get_run(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<PayrollRun>, StatusCode> {
    require_permission(&user, "payroll.runs.read")?;
    host.core
        .payroll
        .find_run(user.tenant_id, run_id)
        .await
        .map_err(|error| payroll_status("get payroll run", &user, error))?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

pub async fn approve_run(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(run_id): Path<Uuid>,
) -> Result<Json<PayrollRun>, StatusCode> {
    require_permission(&user, "payroll.runs.approve")?;
    host.core
        .payroll
        .approve_run(user.tenant_id, run_id, user.account_id)
        .await
        .map(Json)
        .map_err(|error| payroll_status("approve payroll run", &user, error))
}

fn normalize_optional_decimal(value: Option<String>) -> Option<String> {
    value.map(|decimal: String| decimal.trim().to_owned())
}

fn require_permission(user: &AuthenticatedUser, permission: &str) -> Result<(), StatusCode> {
    if user.has_permission(permission) {
        Ok(())
    } else {
        info!(
            "Payroll request denied: tenant_id={} account_id={} required_permission={}",
            user.tenant_id, user.account_id, permission
        );
        Err(StatusCode::FORBIDDEN)
    }
}

fn payroll_status(operation: &str, user: &AuthenticatedUser, error: PayrollError) -> StatusCode {
    let status: StatusCode = match error {
        PayrollError::NotFound => StatusCode::NOT_FOUND,
        PayrollError::Conflict => StatusCode::CONFLICT,
        PayrollError::InvalidInput(_) => StatusCode::BAD_REQUEST,
        PayrollError::MissingCompensation => StatusCode::UNPROCESSABLE_ENTITY,
        PayrollError::BackendUnavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    error!(
        "Payroll request failed: operation={} tenant_id={} account_id={} status={} error={:?}",
        operation, user.tenant_id, user.account_id, status, error
    );
    status
}
