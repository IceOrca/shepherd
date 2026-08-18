use std::sync::Arc;

use axum::{Extension, Json, Router, extract::State, http::StatusCode, routing::get};
use tracing::{error, warn, info, debug, trace};
use crate::features::organization::core::{BranchSummary, FacilitySummary, OrganizationError};

use crate::{AppContext, auth::AuthenticatedUser};

pub fn routes() -> Router<Arc<AppContext>> {
    Router::new()
        .route("/branches", get(list_branches))
        .route("/facilities", get(list_facilities))
}

pub async fn list_branches(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<BranchSummary>>, StatusCode> {
    require_permission(&user, "business.branches.read")?;
    host.core
        .organization
        .list_active_branches(user.tenant_id)
        .await
        .map(Json)
        .map_err(|error| business_status("list branches", &user, error))
}

pub async fn list_facilities(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<FacilitySummary>>, StatusCode> {
    require_permission(&user, "business.facilities.read")?;
    host.core
        .organization
        .list_active_facilities(user.tenant_id)
        .await
        .map(Json)
        .map_err(|error| business_status("list facilities", &user, error))
}

fn require_permission(user: &AuthenticatedUser, permission: &str) -> Result<(), StatusCode> {
    if user.has_permission(permission) {
        Ok(())
    } else {
        info!(
            "Business location request denied: tenant_id={} account_id={} required_permission={}",
            user.tenant_id, user.account_id, permission
        );
        Err(StatusCode::FORBIDDEN)
    }
}

fn business_status(operation: &str, user: &AuthenticatedUser, error: OrganizationError) -> StatusCode {
    let status = match error {
        OrganizationError::BackendUnavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    error!(
        "Business location request failed: operation={} tenant_id={} account_id={} status={}",
        operation, user.tenant_id, user.account_id, status
    );
    status
}
