use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    routing::{get, post},
};
use serde::Deserialize;
use tracing::{debug, error, info, trace, warn};
use ts_rs::TS;
use uuid::Uuid;

use crate::{AppContext, auth::AuthenticatedUser};

use super::{
    super::core::StaffingError,
    core::{OwnStaffingAssignment, ShiftWorkActionInput, ShiftWorkSession},
};

fn staffing_gps_enabled() -> bool {
    let enabled: bool = std::env::var("STAFFING_GPS_ENABLED")
        .ok()
        .is_some_and(|value: String| value.eq_ignore_ascii_case("true"));
    trace!(gps_enabled = enabled, "Resolved staffing GPS feature flag");
    enabled
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
        let gps_enabled: bool = staffing_gps_enabled();
        let location_supplied: bool =
            self.latitude.is_some() || self.longitude.is_some() || self.accuracy_meters.is_some();
        let location: (Option<f64>, Option<f64>, Option<f32>) = if gps_enabled {
            (self.latitude, self.longitude, self.accuracy_meters)
        } else {
            (None, None, None)
        };
        trace!(
            gps_enabled,
            location_supplied,
            location_retained = gps_enabled && location_supplied,
            "Prepared staffing work action input without logging coordinates"
        );
        ShiftWorkActionInput {
            idempotency_key,
            latitude: location.0,
            longitude: location.1,
            accuracy_meters: location.2,
        }
    }
}

pub fn routes() -> Router<Arc<AppContext>> {
    info!("Configured staffing employee work-session routes");
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
    debug!(
        operation = "list_own_staffing_assignments",
        tenant_id = %user.tenant_id,
        account_id = %user.account_id,
        "Staffing work request accepted"
    );
    let assignments: Vec<OwnStaffingAssignment> = context
        .core
        .staffing_work
        .list_own_assignments(user.tenant_id, user.account_id)
        .await
        .map_err(|error: StaffingError| staffing_work_status("list own assignments", &user, error))?;
    debug!(
        operation = "list_own_staffing_assignments",
        tenant_id = %user.tenant_id,
        account_id = %user.account_id,
        assignment_count = assignments.len(),
        "Staffing work request completed"
    );
    Ok(Json(assignments))
}

async fn start(
    State(context): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(assignment_id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<ShiftWorkActionRequest>,
) -> Result<(StatusCode, Json<ShiftWorkSession>), StatusCode> {
    require_permission(&user, "business.staffing_work.self.manage")?;
    info!(
        operation = "start_staffing_work",
        tenant_id = %user.tenant_id,
        account_id = %user.account_id,
        assignment_id = %assignment_id,
        "Staffing work start request accepted"
    );
    let key: Uuid = idempotency_key(&headers, &user)?;
    let input: ShiftWorkActionInput = payload.into_input(key);
    let session: ShiftWorkSession = context
        .core
        .staffing_work
        .start(user.tenant_id, assignment_id, user.account_id, input)
        .await
        .map_err(|error: StaffingError| staffing_work_status("start assignment work", &user, error))?;
    context.notifications.wake();
    info!(
        operation = "start_staffing_work",
        tenant_id = %user.tenant_id,
        account_id = %user.account_id,
        assignment_id = %assignment_id,
        session_id = %session.id,
        "Staffing work start completed and notification delivery was scheduled"
    );
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
    info!(
        operation = "end_staffing_work",
        tenant_id = %user.tenant_id,
        account_id = %user.account_id,
        assignment_id = %assignment_id,
        "Staffing work end request accepted"
    );
    let key: Uuid = idempotency_key(&headers, &user)?;
    let input: ShiftWorkActionInput = payload.into_input(key);
    let session: ShiftWorkSession = context
        .core
        .staffing_work
        .end(user.tenant_id, assignment_id, user.account_id, input)
        .await
        .map_err(|error: StaffingError| staffing_work_status("end assignment work", &user, error))?;
    context.notifications.wake();
    info!(
        operation = "end_staffing_work",
        tenant_id = %user.tenant_id,
        account_id = %user.account_id,
        assignment_id = %assignment_id,
        session_id = %session.id,
        worked_seconds = ?session.worked_seconds,
        "Staffing work end completed and notification delivery was scheduled"
    );
    Ok(Json(session))
}

fn idempotency_key(headers: &HeaderMap, user: &AuthenticatedUser) -> Result<Uuid, StatusCode> {
    let raw_header: Option<&HeaderValue> = headers.get("idempotency-key");
    let raw_key: Option<&str> = raw_header.and_then(|value: &HeaderValue| value.to_str().ok());
    let key: Option<Uuid> = raw_key.and_then(|value: &str| Uuid::parse_str(value).ok());

    match key {
        Some(parsed_key) => {
            trace!(
                tenant_id = %user.tenant_id,
                account_id = %user.account_id,
                "Accepted staffing work idempotency header without logging its value"
            );
            Ok(parsed_key)
        }
        None => {
            warn!(
                tenant_id = %user.tenant_id,
                account_id = %user.account_id,
                header_present = raw_header.is_some(),
                "Rejected staffing work action without a valid idempotency header"
            );
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

fn require_permission(user: &AuthenticatedUser, permission: &str) -> Result<(), StatusCode> {
    if user.has_permission(permission) {
        trace!(
            tenant_id = %user.tenant_id,
            account_id = %user.account_id,
            required_permission = permission,
            "Staffing work permission granted"
        );
        Ok(())
    } else {
        warn!(
            tenant_id = %user.tenant_id,
            account_id = %user.account_id,
            required_permission = permission,
            "Staffing work request denied"
        );
        Err(StatusCode::FORBIDDEN)
    }
}

fn staffing_work_status(operation: &str, user: &AuthenticatedUser, error: StaffingError) -> StatusCode {
    let status: StatusCode = match &error {
        StaffingError::NotFound => StatusCode::NOT_FOUND,
        StaffingError::Conflict => StatusCode::CONFLICT,
        StaffingError::InvalidInput(_) => StatusCode::BAD_REQUEST,
        StaffingError::MissingRateAgreement => StatusCode::UNPROCESSABLE_ENTITY,
        StaffingError::BackendUnavailable => StatusCode::SERVICE_UNAVAILABLE,
    };

    if status.is_server_error() {
        error!(
            operation,
            tenant_id = %user.tenant_id,
            account_id = %user.account_id,
            status = %status,
            reason = ?error,
            "Staffing work request failed unexpectedly"
        );
    } else {
        warn!(
            operation,
            tenant_id = %user.tenant_id,
            account_id = %user.account_id,
            status = %status,
            reason = ?error,
            "Staffing work request rejected"
        );
    }
    status
}

#[cfg(test)]
mod tests {
    use super::{AuthenticatedUser, HeaderMap, StatusCode, idempotency_key};

    fn user() -> AuthenticatedUser {
        AuthenticatedUser {
            tenant_id: uuid::Uuid::new_v4(),
            account_id: uuid::Uuid::new_v4(),
            username: "staff-test".to_owned(),
            email: None,
            primary_role: "staff".to_owned(),
            roles: vec!["staff".to_owned()],
            permissions: vec![],
        }
    }

    #[test]
    fn parses_uuid_idempotency_header_case_insensitively() -> Result<(), Box<dyn std::error::Error>> {
        let expected: uuid::Uuid = uuid::Uuid::new_v4();
        let mut headers: HeaderMap = HeaderMap::new();
        let test_user: AuthenticatedUser = user();
        headers.insert("Idempotency-Key", expected.to_string().parse()?);

        assert_eq!(idempotency_key(&headers, &test_user), Ok(expected));
        Ok(())
    }

    #[test]
    fn rejects_missing_or_malformed_idempotency_header() -> Result<(), Box<dyn std::error::Error>> {
        let test_user: AuthenticatedUser = user();
        assert_eq!(
            idempotency_key(&HeaderMap::new(), &test_user),
            Err(StatusCode::BAD_REQUEST)
        );

        let mut headers: HeaderMap = HeaderMap::new();
        headers.insert("idempotency-key", "not-a-uuid".parse()?);
        assert_eq!(idempotency_key(&headers, &test_user), Err(StatusCode::BAD_REQUEST));
        Ok(())
    }
}
