use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use infra_kernel::debug::*;
use serde::Deserialize;
use ts_rs::TS;
use uuid::Uuid;

use crate::{AppContext, auth::AuthenticatedUser};

use super::{
    super::core::StaffingError,
    core::{OwnStaffingAssignment, ShiftWorkActionInput, ShiftWorkSession},
};

fn staffing_gps_enabled() -> bool {
    std::env::var("STAFFING_GPS_ENABLED")
        .ok()
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

#[derive(Debug, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct ShiftWorkActionRequest {
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub accuracy_meters: Option<f32>,
}

impl ShiftWorkActionRequest {
    fn into_input(self, idempotency_key: Uuid) -> ShiftWorkActionInput {
        let (latitude, longitude, accuracy_meters) = if staffing_gps_enabled() {
            (self.latitude, self.longitude, self.accuracy_meters)
        } else {
            (None, None, None)
        };
        ShiftWorkActionInput {
            idempotency_key,
            latitude,
            longitude,
            accuracy_meters,
        }
    }
}

pub fn routes() -> Router<Arc<AppContext>> {
    Router::new()
        .route("/staffing/assignments/me", get(list_own_assignments))
        .route("/staffing/assignments/{assignment_id}/start", post(start))
        .route("/staffing/assignments/{assignment_id}/end", post(end))
}

async fn list_own_assignments(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<OwnStaffingAssignment>>, StatusCode> {
    require_permission(&user, "business.staffing_work.self.read")?;
    context
        .core
        .staffing_work
        .list_own_assignments(user.tenant_id, user.account_id)
        .await
        .map(Json)
        .map_err(|error| staffing_work_status("list own assignments", &user, error))
}

async fn start(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(assignment_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<ShiftWorkActionRequest>,
) -> Result<(StatusCode, Json<ShiftWorkSession>), StatusCode> {
    require_permission(&user, "business.staffing_work.self.manage")?;
    let session = context
        .core
        .staffing_work
        .start(
            user.tenant_id,
            assignment_id,
            user.account_id,
            payload.into_input(idempotency_key(&headers)?),
        )
        .await
        .map_err(|error| staffing_work_status("start assignment work", &user, error))?;
    context.notifications.wake();
    Ok((StatusCode::CREATED, Json(session)))
}

async fn end(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(assignment_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<ShiftWorkActionRequest>,
) -> Result<Json<ShiftWorkSession>, StatusCode> {
    require_permission(&user, "business.staffing_work.self.manage")?;
    let session = context
        .core
        .staffing_work
        .end(
            user.tenant_id,
            assignment_id,
            user.account_id,
            payload.into_input(idempotency_key(&headers)?),
        )
        .await
        .map_err(|error| staffing_work_status("end assignment work", &user, error))?;
    context.notifications.wake();
    Ok(Json(session))
}

fn idempotency_key(headers: &HeaderMap) -> Result<Uuid, StatusCode> {
    headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or(StatusCode::BAD_REQUEST)
}

fn require_permission(user: &AuthenticatedUser, permission: &str) -> Result<(), StatusCode> {
    if user.has_permission(permission) {
        Ok(())
    } else {
        log_notice!(
            "Staffing work request denied: tenant_id={} account_id={} required_permission={}",
            user.tenant_id,
            user.account_id,
            permission
        );
        Err(StatusCode::FORBIDDEN)
    }
}

fn staffing_work_status(operation: &str, user: &AuthenticatedUser, error: StaffingError) -> StatusCode {
    let status = match error {
        StaffingError::NotFound => StatusCode::NOT_FOUND,
        StaffingError::Conflict => StatusCode::CONFLICT,
        StaffingError::InvalidInput(message) => {
            log_warn!(
                "Staffing work input rejected: operation={} tenant_id={} account_id={} reason={}",
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
            "Staffing work request failed: operation={} tenant_id={} account_id={} status={}",
            operation,
            user.tenant_id,
            user.account_id,
            status
        );
    }
    status
}

#[cfg(test)]
mod tests {
    use super::{HeaderMap, StatusCode, idempotency_key};

    #[test]
    fn parses_uuid_idempotency_header_case_insensitively() -> Result<(), Box<dyn std::error::Error>> {
        let expected = uuid::Uuid::new_v4();
        let mut headers = HeaderMap::new();
        headers.insert("Idempotency-Key", expected.to_string().parse()?);

        assert_eq!(idempotency_key(&headers), Ok(expected));
        Ok(())
    }

    #[test]
    fn rejects_missing_or_malformed_idempotency_header() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(idempotency_key(&HeaderMap::new()), Err(StatusCode::BAD_REQUEST));

        let mut headers = HeaderMap::new();
        headers.insert("idempotency-key", "not-a-uuid".parse()?);
        assert_eq!(idempotency_key(&headers), Err(StatusCode::BAD_REQUEST));
        Ok(())
    }
}
