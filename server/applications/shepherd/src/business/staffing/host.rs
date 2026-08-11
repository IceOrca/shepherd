use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::{DateTime, NaiveDate, Utc};
use infra_kernel::debug::*;
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{AppContext, auth::AuthenticatedUser};

use super::core::{
    BusinessRecordStatus, Customer, CustomerFacility, CustomerFacilityInput, CustomerInput, ManualRateOverride,
    ShiftAssignment, ShiftAssignmentInput, StaffingError, StaffingRateAgreement, StaffingRateAgreementInput,
    StaffingShift, StaffingShiftInput,
};

#[derive(Debug, Deserialize, ToSchema)]
pub struct CustomerCreateRequest {
    pub code: String,
    pub name: String,
    pub billing_email: Option<String>,
    pub status: BusinessRecordStatus,
}

impl From<CustomerCreateRequest> for CustomerInput {
    fn from(value: CustomerCreateRequest) -> Self {
        Self {
            code: value.code.trim().to_ascii_lowercase(),
            name: value.name.trim().to_owned(),
            billing_email: normalize_optional(value.billing_email),
            status: value.status,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CustomerFacilityCreateRequest {
    pub code: String,
    pub name: String,
    pub address: Option<String>,
    pub time_zone: String,
    pub status: BusinessRecordStatus,
}

impl From<CustomerFacilityCreateRequest> for CustomerFacilityInput {
    fn from(value: CustomerFacilityCreateRequest) -> Self {
        Self {
            code: value.code.trim().to_ascii_lowercase(),
            name: value.name.trim().to_owned(),
            address: normalize_optional(value.address),
            time_zone: value.time_zone.trim().to_owned(),
            status: value.status,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct StaffingRateAgreementCreateRequest {
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

impl From<StaffingRateAgreementCreateRequest> for StaffingRateAgreementInput {
    fn from(value: StaffingRateAgreementCreateRequest) -> Self {
        Self {
            code: value.code.trim().to_ascii_lowercase(),
            name: value.name.trim().to_owned(),
            customer_id: value.customer_id,
            customer_facility_id: value.customer_facility_id,
            employee_id: value.employee_id,
            job_id: value.job_id,
            currency: value.currency.trim().to_ascii_uppercase(),
            bill_hourly_rate: value.bill_hourly_rate.trim().to_owned(),
            worker_hourly_rate: value.worker_hourly_rate.trim().to_owned(),
            priority: value.priority,
            effective_from: value.effective_from,
            effective_to: value.effective_to,
            is_active: value.is_active,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct StaffingShiftCreateRequest {
    pub customer_id: Uuid,
    pub customer_facility_id: Uuid,
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
            customer_facility_id: value.customer_facility_id,
            job_id: value.job_id,
            starts_at: value.starts_at,
            ends_at: value.ends_at,
            required_workers: value.required_workers,
            notes: normalize_optional(value.notes),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ManualRateOverrideRequest {
    pub currency: String,
    pub bill_hourly_rate: String,
    pub worker_hourly_rate: String,
}

impl From<ManualRateOverrideRequest> for ManualRateOverride {
    fn from(value: ManualRateOverrideRequest) -> Self {
        Self {
            currency: value.currency.trim().to_ascii_uppercase(),
            bill_hourly_rate: value.bill_hourly_rate.trim().to_owned(),
            worker_hourly_rate: value.worker_hourly_rate.trim().to_owned(),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct ShiftAssignmentApproveRequest {
    pub worked_seconds: i64,
}

pub fn routes() -> Router<Arc<AppContext>> {
    Router::new()
        .route("/customers", get(list_customers).post(create_customer))
        .route(
            "/customers/{customer_id}/facilities",
            get(list_customer_facilities).post(create_customer_facility),
        )
        .route(
            "/staffing/rate-agreements",
            get(list_rate_agreements).post(create_rate_agreement),
        )
        .route("/staffing/shifts", get(list_shifts).post(create_shift))
        .route(
            "/staffing/shifts/{shift_id}/assignments",
            get(list_shift_assignments).post(create_shift_assignment),
        )
        .route(
            "/staffing/assignments/{assignment_id}/approve",
            post(approve_shift_assignment),
        )
}

#[utoipa::path(
    get,
    path = "/business/customers",
    tag = "business",
    security(("bearer_auth" = [])),
    responses((status = 200, body = [Customer]), (status = 403), (status = 503))
)]
pub async fn list_customers(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<Customer>>, StatusCode> {
    require_permission(&user, "business.customers.read")?;
    context
        .core
        .staffing
        .list_customers(user.tenant_id)
        .await
        .map(Json)
        .map_err(|error| staffing_status("list customers", &user, error))
}

#[utoipa::path(
    post,
    path = "/business/customers",
    tag = "business",
    security(("bearer_auth" = [])),
    request_body = CustomerCreateRequest,
    responses((status = 201, body = Customer), (status = 400), (status = 403), (status = 409), (status = 503))
)]
pub async fn create_customer(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<CustomerCreateRequest>,
) -> Result<(StatusCode, Json<Customer>), StatusCode> {
    require_permission(&user, "business.customers.manage")?;
    let customer = context
        .core
        .staffing
        .create_customer(user.tenant_id, payload.into(), user.account_id)
        .await
        .map_err(|error| staffing_status("create customer", &user, error))?;
    Ok((StatusCode::CREATED, Json(customer)))
}

#[utoipa::path(
    get,
    path = "/business/customers/{customer_id}/facilities",
    tag = "business",
    security(("bearer_auth" = [])),
    params(("customer_id" = Uuid, Path)),
    responses((status = 200, body = [CustomerFacility]), (status = 403), (status = 503))
)]
pub async fn list_customer_facilities(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(customer_id): Path<Uuid>,
) -> Result<Json<Vec<CustomerFacility>>, StatusCode> {
    require_permission(&user, "business.customers.read")?;
    context
        .core
        .staffing
        .list_customer_facilities(user.tenant_id, customer_id)
        .await
        .map(Json)
        .map_err(|error| staffing_status("list customer facilities", &user, error))
}

#[utoipa::path(
    post,
    path = "/business/customers/{customer_id}/facilities",
    tag = "business",
    security(("bearer_auth" = [])),
    params(("customer_id" = Uuid, Path)),
    request_body = CustomerFacilityCreateRequest,
    responses(
        (status = 201, body = CustomerFacility),
        (status = 400),
        (status = 403),
        (status = 409),
        (status = 503)
    )
)]
pub async fn create_customer_facility(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(customer_id): Path<Uuid>,
    Json(payload): Json<CustomerFacilityCreateRequest>,
) -> Result<(StatusCode, Json<CustomerFacility>), StatusCode> {
    require_permission(&user, "business.customers.manage")?;
    let facility = context
        .core
        .staffing
        .create_customer_facility(user.tenant_id, customer_id, payload.into(), user.account_id)
        .await
        .map_err(|error| staffing_status("create customer facility", &user, error))?;
    Ok((StatusCode::CREATED, Json(facility)))
}

#[utoipa::path(
    get,
    path = "/business/staffing/rate-agreements",
    tag = "staffing",
    security(("bearer_auth" = [])),
    responses((status = 200, body = [StaffingRateAgreement]), (status = 403), (status = 503))
)]
pub async fn list_rate_agreements(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<StaffingRateAgreement>>, StatusCode> {
    require_permission(&user, "business.staffing_rates.read")?;
    context
        .core
        .staffing
        .list_rate_agreements(user.tenant_id)
        .await
        .map(Json)
        .map_err(|error| staffing_status("list rate agreements", &user, error))
}

#[utoipa::path(
    post,
    path = "/business/staffing/rate-agreements",
    tag = "staffing",
    security(("bearer_auth" = [])),
    request_body = StaffingRateAgreementCreateRequest,
    responses(
        (status = 201, body = StaffingRateAgreement),
        (status = 400),
        (status = 403),
        (status = 409),
        (status = 503)
    )
)]
pub async fn create_rate_agreement(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<StaffingRateAgreementCreateRequest>,
) -> Result<(StatusCode, Json<StaffingRateAgreement>), StatusCode> {
    require_permission(&user, "business.staffing_rates.manage")?;
    let agreement = context
        .core
        .staffing
        .create_rate_agreement(user.tenant_id, payload.into(), user.account_id)
        .await
        .map_err(|error| staffing_status("create rate agreement", &user, error))?;
    Ok((StatusCode::CREATED, Json(agreement)))
}

#[utoipa::path(
    get,
    path = "/business/staffing/shifts",
    tag = "staffing",
    security(("bearer_auth" = [])),
    responses((status = 200, body = [StaffingShift]), (status = 403), (status = 503))
)]
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

#[utoipa::path(
    post,
    path = "/business/staffing/shifts",
    tag = "staffing",
    security(("bearer_auth" = [])),
    request_body = StaffingShiftCreateRequest,
    responses((status = 201, body = StaffingShift), (status = 400), (status = 403), (status = 503))
)]
pub async fn create_shift(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<StaffingShiftCreateRequest>,
) -> Result<(StatusCode, Json<StaffingShift>), StatusCode> {
    require_permission(&user, "business.shifts.manage")?;
    let shift = context
        .core
        .staffing
        .create_shift(user.tenant_id, payload.into(), user.account_id)
        .await
        .map_err(|error| staffing_status("create shift", &user, error))?;
    Ok((StatusCode::CREATED, Json(shift)))
}

#[utoipa::path(
    get,
    path = "/business/staffing/shifts/{shift_id}/assignments",
    tag = "staffing",
    security(("bearer_auth" = [])),
    params(("shift_id" = Uuid, Path)),
    responses((status = 200, body = [ShiftAssignment]), (status = 403), (status = 503))
)]
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

#[utoipa::path(
    post,
    path = "/business/staffing/shifts/{shift_id}/assignments",
    tag = "staffing",
    security(("bearer_auth" = [])),
    params(("shift_id" = Uuid, Path)),
    request_body = ShiftAssignmentCreateRequest,
    responses(
        (status = 201, body = ShiftAssignment),
        (status = 400),
        (status = 403),
        (status = 409),
        (status = 422),
        (status = 503)
    )
)]
pub async fn create_shift_assignment(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(shift_id): Path<Uuid>,
    Json(payload): Json<ShiftAssignmentCreateRequest>,
) -> Result<(StatusCode, Json<ShiftAssignment>), StatusCode> {
    require_permission(&user, "business.shifts.manage")?;
    let assignment = context
        .core
        .staffing
        .create_shift_assignment(user.tenant_id, shift_id, payload.into(), user.account_id)
        .await
        .map_err(|error| staffing_status("create shift assignment", &user, error))?;
    Ok((StatusCode::CREATED, Json(assignment)))
}

#[utoipa::path(
    post,
    path = "/business/staffing/assignments/{assignment_id}/approve",
    tag = "staffing",
    security(("bearer_auth" = [])),
    params(("assignment_id" = Uuid, Path)),
    request_body = ShiftAssignmentApproveRequest,
    responses((status = 200, body = ShiftAssignment), (status = 400), (status = 403), (status = 409), (status = 503))
)]
pub async fn approve_shift_assignment(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(assignment_id): Path<Uuid>,
    Json(payload): Json<ShiftAssignmentApproveRequest>,
) -> Result<Json<ShiftAssignment>, StatusCode> {
    require_permission(&user, "business.shifts.approve")?;
    context
        .core
        .staffing
        .approve_shift_assignment(user.tenant_id, assignment_id, payload.worked_seconds, user.account_id)
        .await
        .map(Json)
        .map_err(|error| staffing_status("approve shift assignment", &user, error))
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let normalized = value.trim().to_owned();
        (!normalized.is_empty()).then_some(normalized)
    })
}

fn require_permission(user: &AuthenticatedUser, permission: &str) -> Result<(), StatusCode> {
    if user.has_permission(permission) {
        Ok(())
    } else {
        log_notice!(
            "Staffing request denied: tenant_id={} account_id={} required_permission={}",
            user.tenant_id,
            user.account_id,
            permission
        );
        Err(StatusCode::FORBIDDEN)
    }
}

fn staffing_status(operation: &str, user: &AuthenticatedUser, error: StaffingError) -> StatusCode {
    let status = match error {
        StaffingError::NotFound => StatusCode::NOT_FOUND,
        StaffingError::Conflict => StatusCode::CONFLICT,
        StaffingError::InvalidInput(message) => {
            log_warn!(
                "Staffing request input rejected: operation={} tenant_id={} account_id={} reason={}",
                operation,
                user.tenant_id,
                user.account_id,
                message
            );
            StatusCode::BAD_REQUEST
        }
        StaffingError::MissingRateAgreement => StatusCode::UNPROCESSABLE_ENTITY,
        StaffingError::BackendUnavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    if status.is_server_error() {
        log_error!(
            "Staffing request failed: operation={} tenant_id={} account_id={} status={}",
            operation,
            user.tenant_id,
            user.account_id,
            status
        );
    }
    status
}
