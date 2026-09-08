use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
};
use tracing::{error, warn, info, debug, trace};
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    AppContext,
    auth::{AuthedUser, PermissionRouteExt, invalidate_tenant_accounts},
    pagination::{decode_cursor, encode_cursor, normalize_search, resolve_limit},
};
use super::core::{
    Branch, BranchCreateRequest, BranchCursor, BranchErr, BranchSummary, BranchSummaryCursor, BranchUpdateRequest,
};

const READ_PERMISSION: &str = "business.branches.read";
const MANAGE_PERMISSION: &str = "business.branches.manage";

#[derive(Debug, Deserialize)]
struct BranchPageQuery {
    limit: Option<u16>,
    cursor: Option<String>,
    search: Option<String>,
}

#[derive(Debug, Serialize, TS)]
pub struct BranchPageResponse {
    pub items: Vec<Branch>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub limit: u16,
}

#[derive(Debug, Serialize, TS)]
pub struct BranchSummaryPageResponse {
    pub items: Vec<BranchSummary>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub limit: u16,
}

pub fn routes() -> Router<Arc<AppContext>> {
    Router::new()
        .route("/branches", get(list_branches).require_one(READ_PERMISSION))
        .route("/branches", post(create_branch).require_one(MANAGE_PERMISSION))
        .route(
            "/branches/manage",
            get(list_managed_branches).require_one(MANAGE_PERMISSION),
        )
        .route(
            "/branches/{branch_id}",
            put(update_branch).require_one(MANAGE_PERMISSION),
        )
}

async fn list_branches(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Query(query): Query<BranchPageQuery>,
) -> Result<Json<BranchSummaryPageResponse>, StatusCode> {
    let limit = resolve_limit(&ctx.pagination, query.limit)?;
    let cursor: Option<BranchSummaryCursor> = decode_cursor(query.cursor.as_deref())?;
    let page = ctx
        .core
        .branch
        .list_active_branches(
            user.tenant_id,
            user.branch_ids.clone(),
            normalize_search(query.search),
            i64::from(limit),
            cursor,
        )
        .await
        .map_err(|error: BranchErr| business_status("list branches", &user, error))?;
    let next_cursor = encode_cursor(page.next_cursor.as_ref())?;
    debug!(
        operation = "branch.list_accessible",
        tenant_id = %user.tenant_id,
        account_id = %user.account_id,
        active_branch_id = ?user.active_branch_id,
        branch_count = page.items.len(),
        "Returning only branches authorized for the current account"
    );
    Ok(Json(BranchSummaryPageResponse {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

async fn list_managed_branches(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Query(query): Query<BranchPageQuery>,
) -> Result<Json<BranchPageResponse>, StatusCode> {
    let limit = resolve_limit(&ctx.pagination, query.limit)?;
    let cursor: Option<BranchCursor> = decode_cursor(query.cursor.as_deref())?;
    let page = ctx
        .core
        .branch
        .list_managed_branches(
            user.tenant_id,
            user.account_id,
            normalize_search(query.search),
            i64::from(limit),
            cursor,
        )
        .await
        .map_err(|error: BranchErr| business_status("list managed branches", &user, error))?;
    let next_cursor = encode_cursor(page.next_cursor.as_ref())?;
    Ok(Json(BranchPageResponse {
        has_more: next_cursor.is_some(),
        items: page.items,
        next_cursor,
        limit,
    }))
}

async fn create_branch(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Json(request): Json<BranchCreateRequest>,
) -> Result<(StatusCode, Json<Branch>), StatusCode> {
    let branch = ctx
        .core
        .branch
        .create_branch(user.tenant_id, user.account_id, request)
        .await
        .map_err(|error: BranchErr| business_status("create branch", &user, error))?;
    invalidate_tenant_accounts(&ctx.auth, user.tenant_id).await;
    info!(
        operation = "branch.create",
        tenant_id = %user.tenant_id,
        account_id = %user.account_id,
        branch_id = %branch.id,
        "Tenant branch created"
    );
    Ok((StatusCode::CREATED, Json(branch)))
}

async fn update_branch(
    State(ctx): State<Arc<AppContext>>,
    Extension(user): Extension<AuthedUser>,
    Path(branch_id): Path<Uuid>,
    Json(request): Json<BranchUpdateRequest>,
) -> Result<Json<Branch>, StatusCode> {
    let branch = ctx
        .core
        .branch
        .update_branch(user.tenant_id, user.account_id, branch_id, request)
        .await
        .map_err(|error: BranchErr| business_status("update branch", &user, error))?;
    invalidate_tenant_accounts(&ctx.auth, user.tenant_id).await;
    info!(
        operation = "branch.update",
        tenant_id = %user.tenant_id,
        account_id = %user.account_id,
        branch_id = %branch.id,
        branch_status = %branch.status,
        "Tenant branch updated"
    );
    Ok(Json(branch))
}

fn business_status(operation: &str, user: &AuthedUser, error: BranchErr) -> StatusCode {
    let status = match error {
        BranchErr::Forbidden => StatusCode::FORBIDDEN,
        BranchErr::Conflict => StatusCode::CONFLICT,
        BranchErr::InvalidInput(reason) => {
            warn!(
                operation,
                tenant_id = %user.tenant_id,
                account_id = %user.account_id,
                reason,
                "Branch input rejected"
            );
            StatusCode::BAD_REQUEST
        }
        BranchErr::BackendUnavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    if status.is_server_error() {
        error!(
            operation,
            tenant_id = %user.tenant_id,
            account_id = %user.account_id,
            %status,
            "Branch request failed"
        );
    }
    status
}
