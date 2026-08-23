use std::sync::Arc;

use axum::{Extension, Json, Router, extract::State, http::StatusCode, routing::get};
use tracing::{error, warn, info, debug, trace};
use crate::features::organization::core::{BranchSummary, OrganizationError};

use crate::{AppContext, auth::AuthenticatedUser};

pub fn routes() -> Router<Arc<AppContext>> {
    Router::new().route("/branches", get(list_branches))
}

pub async fn list_branches(
    State(host): State<Arc<AppContext>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<BranchSummary>>, StatusCode> {
    require_permission(&user, "business.branches.read")?;
    let mut branches: Vec<BranchSummary> = host
        .core
        .organization
        .list_active_branches(user.tenant_id)
        .await
        .map_err(|error| business_status("list branches", &user, error))?;
    branches.retain(|branch: &BranchSummary| user.branch_ids.contains(&branch.id));
    debug!(
        operation = "organization.list_accessible_branches",
        tenant_id = %user.tenant_id,
        account_id = %user.account_id,
        active_branch_id = ?user.active_branch_id,
        branch_count = branches.len(),
        "Returning only branches authorized for the current account"
    );
    Ok(Json(branches))
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
