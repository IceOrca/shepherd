use std::{collections::BTreeSet, sync::Arc};

use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use chrono::{DateTime, Utc};
use infra_postgres::TenantDbErr;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, postgres::PgQueryResult};
use tracing::{debug, error, info, trace, warn};
use ts_rs::TS;
use uuid::Uuid;

use crate::{AuthCodeError, AuthService, PermissionCode, RoleCode};

use super::{
    account::{AccountStatus, AuthenticatedUser},
    account_cache::AuthenticatedUserCacheError,
    auth_admin::{AuthAccountAccessContext, AuthAccountProvisioner, AuthAccountProvisioningError, AuthAdminPolicy},
};

const MAX_AUDIT_ROWS: i64 = 100;

#[derive(Clone)]
struct AccessControlContext {
    auth: Arc<AuthService>,
    policy: AuthAdminPolicy,
    provisioner: Arc<dyn AuthAccountProvisioner>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AccessRoleScope {
    Tenant,
    Branch,
}

impl AccessRoleScope {
    fn as_code(self) -> &'static str {
        match self {
            Self::Tenant => "tenant",
            Self::Branch => "branch",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        match code {
            "tenant" => Some(Self::Tenant),
            "branch" => Some(Self::Branch),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum PermissionOverrideEffect {
    Allow,
    Deny,
}

impl PermissionOverrideEffect {
    fn as_code(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        match code {
            "allow" => Some(Self::Allow),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct AccessControlBranch {
    #[ts(type = "string")]
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub time_zone: String,
    pub status: String,
    pub version: i64,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct AccessControlPermission {
    pub code: PermissionCode,
    pub description: String,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct AccessControlRole {
    pub code: RoleCode,
    pub display_name: String,
    pub description: Option<String>,
    pub scope: AccessRoleScope,
    pub is_system: bool,
    pub is_active: bool,
    pub version: i64,
    pub permission_codes: Vec<PermissionCode>,
    pub assigned_account_count: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct AccountRoleAssignmentContract {
    pub role_code: RoleCode,
    #[ts(type = "string | null")]
    pub branch_id: Option<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct AccountPermissionOverrideContract {
    pub permission_code: PermissionCode,
    #[ts(type = "string | null")]
    pub branch_id: Option<Uuid>,
    pub effect: PermissionOverrideEffect,
    #[ts(type = "string | null")]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct AccessControlUser {
    #[ts(type = "string")]
    pub account_id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub status: AccountStatus,
    pub primary_role: RoleCode,
    pub authorization_version: i64,
    pub assignments: Vec<AccountRoleAssignmentContract>,
    pub permission_overrides: Vec<AccountPermissionOverrideContract>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct AccessControlAuditEntry {
    #[ts(type = "string")]
    pub id: Uuid,
    #[ts(type = "string")]
    pub actor_account_id: Uuid,
    pub action: String,
    pub object_type: String,
    pub object_id: String,
    #[ts(type = "string | null")]
    pub branch_id: Option<Uuid>,
    #[ts(type = "unknown")]
    pub before_value: Option<Value>,
    #[ts(type = "unknown")]
    pub after_value: Option<Value>,
    #[ts(type = "string")]
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct AccessControlSnapshot {
    pub branches: Vec<AccessControlBranch>,
    pub permissions: Vec<AccessControlPermission>,
    pub roles: Vec<AccessControlRole>,
    pub users: Vec<AccessControlUser>,
    pub audit: Vec<AccessControlAuditEntry>,
}

#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateAccessControlBranchRequest {
    pub code: String,
    pub name: String,
    pub time_zone: String,
}

#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export)]
pub struct UpdateAccessControlBranchRequest {
    pub name: String,
    pub time_zone: String,
    pub status: String,
    pub expected_version: i64,
}

#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateAccessControlRoleRequest {
    pub code: RoleCode,
    pub display_name: String,
    pub description: Option<String>,
    pub scope: AccessRoleScope,
    pub permission_codes: Vec<PermissionCode>,
}

#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export)]
pub struct UpdateAccessControlRoleRequest {
    pub display_name: String,
    pub description: Option<String>,
    pub is_active: bool,
    pub expected_version: i64,
    pub permission_codes: Vec<PermissionCode>,
}

#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export)]
pub struct UpdateAccountAccessRequest {
    pub primary_role: RoleCode,
    pub expected_version: i64,
    pub assignments: Vec<AccountRoleAssignmentContract>,
    pub permission_overrides: Vec<AccountPermissionOverrideContract>,
}

#[derive(Debug)]
enum AccessControlError {
    Forbidden,
    Validation(String),
    Conflict(String),
    NotFound(String),
    Unavailable,
    Internal,
}

#[derive(Serialize)]
struct AccessControlErrorBody {
    code: &'static str,
    message: String,
}

impl IntoResponse for AccessControlError {
    fn into_response(self) -> Response {
        let response: (StatusCode, &'static str, String) = match self {
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "This account cannot manage tenant access control.".to_owned(),
            ),
            Self::Validation(message) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_failed", message),
            Self::Conflict(message) => (StatusCode::CONFLICT, "conflict", message),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, "not_found", message),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "temporarily_unavailable",
                "Access control is temporarily unavailable.".to_owned(),
            ),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "The access-control operation could not be completed.".to_owned(),
            ),
        };
        (
            response.0,
            Json(AccessControlErrorBody {
                code: response.1,
                message: response.2,
            }),
        )
            .into_response()
    }
}

struct BranchRow {
    id: Uuid,
    code: String,
    name: String,
    time_zone: String,
    status: String,
    version: i64,
}

struct PermissionRow {
    code: String,
    description: String,
}

struct RoleRow {
    code: String,
    display_name: String,
    description: Option<String>,
    scope_type: String,
    is_system: bool,
    is_active: bool,
    version: i64,
    assigned_account_count: i64,
}

struct RolePermissionRow {
    role_code: String,
    permission_code: String,
}

struct UserRow {
    account_id: Uuid,
    username: String,
    email: Option<String>,
    status: String,
    primary_role_code: String,
    authorization_version: i64,
}

struct AssignmentRow {
    account_id: Uuid,
    role_code: String,
    branch_id: Option<Uuid>,
}

struct OverrideRow {
    account_id: Uuid,
    permission_code: String,
    branch_id: Option<Uuid>,
    effect: String,
    expires_at: Option<DateTime<Utc>>,
}

struct AuditRow {
    id: Uuid,
    actor_account_id: Uuid,
    action: String,
    object_type: String,
    object_id: String,
    branch_id: Option<Uuid>,
    before_value: Option<Value>,
    after_value: Option<Value>,
    created_at: DateTime<Utc>,
}

type AccessControlSnapshotRows = (
    Vec<BranchRow>,
    Vec<PermissionRow>,
    Vec<RoleRow>,
    Vec<RolePermissionRow>,
    Vec<UserRow>,
    Vec<AssignmentRow>,
    Vec<OverrideRow>,
    Vec<AuditRow>,
);

struct AccessAuditMutation {
    action: &'static str,
    object_type: &'static str,
    object_id: String,
    branch_id: Option<Uuid>,
    before_value: Option<Value>,
    after_value: Option<Value>,
}

struct IdentityRow {
    issuer: String,
    subject: String,
}

struct RoleRuleRow {
    min_assignments: i16,
    max_assignments: Option<i16>,
}

pub fn routes(auth: Arc<AuthService>, policy: AuthAdminPolicy, provisioner: Arc<dyn AuthAccountProvisioner>) -> Router {
    let context: Arc<AccessControlContext> = Arc::new(AccessControlContext {
        auth,
        policy,
        provisioner,
    });
    info!(
        operation = "register_access_control_routes",
        "Registering tenant access-control administration routes"
    );
    Router::new()
        .route("/admin/access-control", get(snapshot))
        .route("/admin/access-control/branches", post(create_branch))
        .route("/admin/access-control/branches/{branch_id}", put(update_branch))
        .route("/admin/access-control/roles", post(create_role))
        .route("/admin/access-control/roles/{role_code}", put(update_role))
        .route("/admin/access-control/users/{account_id}", put(update_user_access))
        .with_state(context)
}

async fn snapshot(
    State(context): State<Arc<AccessControlContext>>,
    Extension(actor): Extension<AuthenticatedUser>,
) -> Result<Json<AccessControlSnapshot>, AccessControlError> {
    // The snapshot intentionally contains every tenant branch and account. It
    // is therefore a management view, not the branch-filtered role catalog.
    require_permission(&actor, &context.policy.role_manage_permission)?;
    info!(operation = "access_control.snapshot", tenant_id = %actor.tenant_id, actor_id = %actor.account_id, "Access-control snapshot request accepted");
    let snapshot: AccessControlSnapshot = load_snapshot(&context.auth, actor.tenant_id).await?;
    debug!(
        operation = "access_control.snapshot",
        tenant_id = %actor.tenant_id,
        actor_id = %actor.account_id,
        branch_count = snapshot.branches.len(),
        role_count = snapshot.roles.len(),
        user_count = snapshot.users.len(),
        permission_count = snapshot.permissions.len(),
        "Access-control snapshot returned"
    );
    Ok(Json(snapshot))
}

async fn create_branch(
    State(context): State<Arc<AccessControlContext>>,
    Extension(actor): Extension<AuthenticatedUser>,
    Json(mut request): Json<CreateAccessControlBranchRequest>,
) -> Result<(StatusCode, Json<AccessControlBranch>), AccessControlError> {
    require_permission(&actor, &context.policy.branch_manage_permission)?;
    normalize_branch_request(&mut request)?;
    let tenant_id: Uuid = actor.tenant_id;
    let actor_id: Uuid = actor.account_id;
    let branch_id: Uuid = Uuid::new_v4();
    let branch: AccessControlBranch = context
        .auth
        .db
        .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
            let inserted: BranchRow = sqlx::query_as!(
                BranchRow,
                r#"
                INSERT INTO branches (
                    id, tenant_id, code, name, time_zone, status,
                    created_by_account_id, updated_by_account_id
                )
                VALUES ($1, $2, $3, $4, $5, 'active', $6, $6)
                RETURNING id, code, name, time_zone, status, version
                "#,
                branch_id,
                tenant_id,
                request.code,
                request.name,
                request.time_zone,
                actor_id,
            )
            .fetch_one(&mut *connection)
            .await?;
            let after_value: Value = json!({
                "id": inserted.id,
                "code": inserted.code,
                "name": inserted.name,
                "time_zone": inserted.time_zone,
                "status": inserted.status,
            });
            insert_audit(
                connection,
                tenant_id,
                actor_id,
                AccessAuditMutation {
                    action: "branch.create",
                    object_type: "branch",
                    object_id: branch_id.to_string(),
                    branch_id: Some(branch_id),
                    before_value: None,
                    after_value: Some(after_value),
                },
            )
            .await?;
            Ok(inserted)
        })
        .await
        .map_err(|database_error: TenantDbErr| {
            database_error_status("create branch", tenant_id, actor_id, database_error)
        })?
        .into();
    invalidate_tenant_accounts(&context.auth, tenant_id).await;
    info!(operation = "access_control.branch.create", tenant_id = %tenant_id, actor_id = %actor_id, branch_id = %branch.id, "Tenant branch created");
    Ok((StatusCode::CREATED, Json(branch)))
}

async fn update_branch(
    State(context): State<Arc<AccessControlContext>>,
    Extension(actor): Extension<AuthenticatedUser>,
    Path(branch_id): Path<Uuid>,
    Json(mut request): Json<UpdateAccessControlBranchRequest>,
) -> Result<Json<AccessControlBranch>, AccessControlError> {
    require_permission(&actor, &context.policy.branch_manage_permission)?;
    normalize_update_branch_request(&mut request)?;
    let tenant_id: Uuid = actor.tenant_id;
    let actor_id: Uuid = actor.account_id;
    let result: Option<BranchRow> = context
        .auth
        .db
        .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
            let before: Option<BranchRow> = sqlx::query_as!(
                BranchRow,
                "SELECT id, code, name, time_zone, status, version FROM branches WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
                tenant_id,
                branch_id,
            )
            .fetch_optional(&mut *connection)
            .await?;
            let Some(before) = before else {
                return Ok(None);
            };
            let updated: Option<BranchRow> = sqlx::query_as!(
                BranchRow,
                r#"
                UPDATE branches
                SET name = $3,
                    time_zone = $4,
                    status = $5,
                    version = version + 1,
                    updated_by_account_id = $6,
                    updated_at = CURRENT_TIMESTAMP
                WHERE tenant_id = $1 AND id = $2 AND version = $7
                RETURNING id, code, name, time_zone, status, version
                "#,
                tenant_id,
                branch_id,
                request.name,
                request.time_zone,
                request.status,
                actor_id,
                request.expected_version,
            )
            .fetch_optional(&mut *connection)
            .await?;
            if let Some(updated) = &updated {
                let before_value: Value = branch_value(&before);
                let after_value: Value = branch_value(updated);
                insert_audit(
                    connection,
                    tenant_id,
                    actor_id,
                    AccessAuditMutation {
                        action: "branch.update",
                        object_type: "branch",
                        object_id: branch_id.to_string(),
                        branch_id: Some(branch_id),
                        before_value: Some(before_value),
                        after_value: Some(after_value),
                    },
                )
                .await?;
            }
            Ok(updated)
        })
        .await
        .map_err(|database_error: TenantDbErr| database_error_status("update branch", tenant_id, actor_id, database_error))?;
    let branch: AccessControlBranch = result
        .ok_or_else(|| {
            AccessControlError::Conflict("The branch changed or no longer exists; reload and retry.".to_owned())
        })?
        .into();
    invalidate_tenant_accounts(&context.auth, tenant_id).await;
    info!(operation = "access_control.branch.update", tenant_id = %tenant_id, actor_id = %actor_id, branch_id = %branch_id, branch_status = %branch.status, "Tenant branch updated");
    Ok(Json(branch))
}

async fn create_role(
    State(context): State<Arc<AccessControlContext>>,
    Extension(actor): Extension<AuthenticatedUser>,
    Json(mut request): Json<CreateAccessControlRoleRequest>,
) -> Result<(StatusCode, Json<AccessControlRole>), AccessControlError> {
    require_permission(&actor, &context.policy.role_manage_permission)?;
    normalize_create_role_request(&mut request)?;
    let tenant_id: Uuid = actor.tenant_id;
    let actor_id: Uuid = actor.account_id;
    let role_code: RoleCode = request.code.clone();
    context
        .auth
        .db
        .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
            sqlx::query!(
                r#"
                INSERT INTO tenant_roles (
                    tenant_id, code, display_name, description, scope_type,
                    is_system, created_by_account_id, updated_by_account_id
                )
                VALUES ($1, $2, $3, $4, $5, FALSE, $6, $6)
                "#,
                tenant_id,
                request.code.as_str(),
                request.display_name,
                request.description,
                request.scope.as_code(),
                actor_id,
            )
            .execute(&mut *connection)
            .await?;
            replace_role_permissions(
                connection,
                tenant_id,
                request.code.as_str(),
                &request.permission_codes,
                actor_id,
            )
            .await?;
            let after_value: Value = json!({
                "code": request.code.as_str(),
                "display_name": request.display_name,
                "description": request.description,
                "scope": request.scope.as_code(),
                "permission_codes": request.permission_codes,
            });
            insert_audit(
                connection,
                tenant_id,
                actor_id,
                AccessAuditMutation {
                    action: "role.create",
                    object_type: "role",
                    object_id: request.code.as_str().to_owned(),
                    branch_id: None,
                    before_value: None,
                    after_value: Some(after_value),
                },
            )
            .await?;
            Ok(())
        })
        .await
        .map_err(|database_error: TenantDbErr| {
            database_error_status("create role", tenant_id, actor_id, database_error)
        })?;
    let role: AccessControlRole = load_role(&context.auth, tenant_id, &role_code).await?;
    info!(operation = "access_control.role.create", tenant_id = %tenant_id, actor_id = %actor_id, role_code = %role_code, "Tenant role created");
    Ok((StatusCode::CREATED, Json(role)))
}

async fn update_role(
    State(context): State<Arc<AccessControlContext>>,
    Extension(actor): Extension<AuthenticatedUser>,
    Path(role_code_raw): Path<String>,
    Json(mut request): Json<UpdateAccessControlRoleRequest>,
) -> Result<Json<AccessControlRole>, AccessControlError> {
    require_permission(&actor, &context.policy.role_manage_permission)?;
    let role_code: RoleCode = RoleCode::parse(role_code_raw)
        .map_err(|code_error: AuthCodeError| AccessControlError::Validation(code_error.to_string()))?;
    normalize_update_role_request(&mut request)?;
    let tenant_id: Uuid = actor.tenant_id;
    let actor_id: Uuid = actor.account_id;
    let role_code_for_update: RoleCode = role_code.clone();
    let update_result: bool = context
        .auth
        .db
        .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
            let before: Option<RoleRow> =
                load_role_row(connection, tenant_id, role_code_for_update.as_str(), true).await?;
            let Some(before) = before else {
                return Ok(false);
            };
            let updated: PgQueryResult = sqlx::query!(
                r#"
                UPDATE tenant_roles
                SET display_name = $3,
                    description = $4,
                    is_active = $5,
                    version = version + 1,
                    updated_by_account_id = $6,
                    updated_at = CURRENT_TIMESTAMP
                WHERE tenant_id = $1 AND code = $2 AND version = $7
                "#,
                tenant_id,
                role_code_for_update.as_str(),
                request.display_name,
                request.description,
                request.is_active,
                actor_id,
                request.expected_version,
            )
            .execute(&mut *connection)
            .await?;
            if updated.rows_affected() != 1 {
                return Ok(false);
            }
            replace_role_permissions(
                connection,
                tenant_id,
                role_code_for_update.as_str(),
                &request.permission_codes,
                actor_id,
            )
            .await?;
            let before_value: Value = role_row_value(&before);
            let after_value: Value = json!({
                "code": role_code_for_update.as_str(),
                "display_name": request.display_name,
                "description": request.description,
                "is_active": request.is_active,
                "permission_codes": request.permission_codes,
            });
            insert_audit(
                connection,
                tenant_id,
                actor_id,
                AccessAuditMutation {
                    action: "role.update",
                    object_type: "role",
                    object_id: role_code_for_update.as_str().to_owned(),
                    branch_id: None,
                    before_value: Some(before_value),
                    after_value: Some(after_value),
                },
            )
            .await?;
            Ok(true)
        })
        .await
        .map_err(|database_error: TenantDbErr| {
            database_error_status("update role", tenant_id, actor_id, database_error)
        })?;
    if !update_result {
        return Err(AccessControlError::Conflict(
            "The role changed or no longer exists; reload and retry.".to_owned(),
        ));
    }
    invalidate_role_accounts(&context.auth, tenant_id, &role_code).await;
    let role: AccessControlRole = load_role(&context.auth, tenant_id, &role_code).await?;
    info!(operation = "access_control.role.update", tenant_id = %tenant_id, actor_id = %actor_id, role_code = %role_code, "Tenant role and permissions updated");
    Ok(Json(role))
}

async fn update_user_access(
    State(context): State<Arc<AccessControlContext>>,
    Extension(actor): Extension<AuthenticatedUser>,
    Path(account_id): Path<Uuid>,
    Json(mut request): Json<UpdateAccountAccessRequest>,
) -> Result<Json<AccessControlUser>, AccessControlError> {
    require_permission(&actor, &context.policy.update_permission)?;
    require_permission(&actor, &context.policy.role_manage_permission)?;
    normalize_user_access_request(&mut request)?;
    let tenant_id: Uuid = actor.tenant_id;
    let actor_id: Uuid = actor.account_id;
    let provisioner: Arc<dyn AuthAccountProvisioner> = Arc::clone(&context.provisioner);
    let updated: bool = context
        .auth
        .db
        .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
            let before: Option<UserRow> = load_user_row(connection, tenant_id, account_id, true).await?;
            let Some(before) = before else {
                return Ok(false);
            };
            validate_primary_role_assignments(connection, tenant_id, &request).await?;
            validate_assignment_references(connection, tenant_id, &request).await?;

            let version_update: PgQueryResult = sqlx::query!(
                r#"
                UPDATE accounts
                SET primary_role_code = $3,
                    authorization_version = authorization_version + 1,
                    updated_by_account_id = $4,
                    updated_at = CURRENT_TIMESTAMP
                WHERE tenant_id = $1 AND id = $2 AND authorization_version = $5
                "#,
                tenant_id,
                account_id,
                request.primary_role.as_str(),
                actor_id,
                request.expected_version,
            )
            .execute(&mut *connection)
            .await?;
            if version_update.rows_affected() != 1 {
                return Ok(false);
            }

            sqlx::query!("DELETE FROM account_role_assignments WHERE tenant_id = $1 AND account_id = $2", tenant_id, account_id)
                .execute(&mut *connection)
                .await?;
            sqlx::query!("DELETE FROM account_permission_overrides WHERE tenant_id = $1 AND account_id = $2", tenant_id, account_id)
                .execute(&mut *connection)
                .await?;
            sqlx::query!("DELETE FROM account_branch_assignments WHERE tenant_id = $1 AND account_id = $2", tenant_id, account_id)
                .execute(&mut *connection)
                .await?;
            sqlx::query!("DELETE FROM account_roles WHERE tenant_id = $1 AND account_id = $2", tenant_id, account_id)
                .execute(&mut *connection)
                .await?;

            for assignment in &request.assignments {
                sqlx::query!(
                    r#"
                    INSERT INTO account_role_assignments (
                        tenant_id, account_id, role_code, branch_id, assigned_by_account_id
                    )
                    VALUES ($1, $2, $3, $4, $5)
                    "#,
                    tenant_id,
                    account_id,
                    assignment.role_code.as_str(),
                    assignment.branch_id,
                    actor_id,
                )
                .execute(&mut *connection)
                .await?;
            }
            for account_override in &request.permission_overrides {
                sqlx::query!(
                    r#"
                    INSERT INTO account_permission_overrides (
                        tenant_id, account_id, permission_code, branch_id,
                        effect, expires_at, granted_by_account_id
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7)
                    "#,
                    tenant_id,
                    account_id,
                    account_override.permission_code.as_str(),
                    account_override.branch_id,
                    account_override.effect.as_code(),
                    account_override.expires_at,
                    actor_id,
                )
                .execute(&mut *connection)
                .await?;
            }

            // Keep the isolated legacy compatibility tables aligned with the
            // protected organizational primary role while runtime reads use
            // the scoped assignment tables above.
            sqlx::query!(
                "INSERT INTO account_roles (tenant_id, account_id, role_code, assigned_by_account_id) VALUES ($1, $2, $3, $4)",
                tenant_id,
                account_id,
                request.primary_role.as_str(),
                actor_id,
            )
            .execute(&mut *connection)
            .await?;
            let primary_branch_ids: BTreeSet<Uuid> = request
                .assignments
                .iter()
                .filter(|assignment: &&AccountRoleAssignmentContract| assignment.role_code == request.primary_role)
                .filter_map(|assignment: &AccountRoleAssignmentContract| assignment.branch_id)
                .collect();
            for branch_id in primary_branch_ids {
                sqlx::query!(
                    r#"
                    INSERT INTO account_branch_assignments (
                        tenant_id, account_id, branch_id, assigned_by_account_id
                    )
                    VALUES ($1, $2, $3, $4)
                    "#,
                    tenant_id,
                    account_id,
                    branch_id,
                    actor_id,
                )
                .execute(&mut *connection)
                .await?;
            }

            let primary_branch_ids: Vec<Uuid> = request
                .assignments
                .iter()
                .filter(|assignment: &&AccountRoleAssignmentContract| assignment.role_code == request.primary_role)
                .filter_map(|assignment: &AccountRoleAssignmentContract| assignment.branch_id)
                .collect();
            let access_context: AuthAccountAccessContext = AuthAccountAccessContext {
                tenant_id,
                actor_account_id: actor_id,
                account_id,
                username: before.username.clone(),
                email: before.email.clone(),
                primary_role: request.primary_role.clone(),
                branch_ids: primary_branch_ids,
            };
            provisioner
                .update_access(connection, &access_context)
                .await
                .map_err(|provisioning_error: AuthAccountProvisioningError| {
                    error!(
                        operation = "access_control.account.application_hook",
                        tenant_id = %tenant_id,
                        actor_id = %actor_id,
                        account_id = %account_id,
                        provisioning_error_code = provisioning_error.code(),
                        "Application-specific account access hook failed"
                    );
                    sqlx::Error::Protocol("application-specific account access hook failed".to_owned())
                })?;

            let before_value: Value = json!({
                "primary_role": before.primary_role_code,
                "authorization_version": before.authorization_version,
            });
            let after_value: Value = json!({
                "primary_role": request.primary_role.as_str(),
                "assignments": request.assignments,
                "permission_overrides": request.permission_overrides,
            });
            insert_audit(
                connection,
                tenant_id,
                actor_id,
                AccessAuditMutation {
                    action: "account.access.update",
                    object_type: "account",
                    object_id: account_id.to_string(),
                    branch_id: None,
                    before_value: Some(before_value),
                    after_value: Some(after_value),
                },
            )
            .await?;
            Ok(true)
        })
        .await
        .map_err(|database_error: TenantDbErr| database_error_status("update account access", tenant_id, actor_id, database_error))?;
    if !updated {
        return Err(AccessControlError::Conflict(
            "The account changed or no longer exists; reload and retry.".to_owned(),
        ));
    }
    invalidate_accounts(&context.auth, tenant_id, Some(account_id)).await;
    let user: AccessControlUser = load_user(&context.auth, tenant_id, account_id).await?;
    info!(operation = "access_control.account.update", tenant_id = %tenant_id, actor_id = %actor_id, account_id = %account_id, assignment_count = user.assignments.len(), override_count = user.permission_overrides.len(), "Account branch roles and permission overrides updated");
    Ok(Json(user))
}

async fn load_snapshot(auth: &AuthService, tenant_id: Uuid) -> Result<AccessControlSnapshot, AccessControlError> {
    let rows: AccessControlSnapshotRows = auth
        .db
        .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
            let branches: Vec<BranchRow> = sqlx::query_as!(BranchRow, "SELECT id, code, name, time_zone, status, version FROM branches WHERE tenant_id = $1 ORDER BY lower(name), id", tenant_id)
                .fetch_all(&mut *connection).await?;
            let permissions: Vec<PermissionRow> = sqlx::query_as!(PermissionRow, "SELECT code, description FROM permissions ORDER BY code")
                .fetch_all(&mut *connection).await?;
            let roles: Vec<RoleRow> = sqlx::query_as!(
                RoleRow,
                r#"
                SELECT role.code, role.display_name, role.description, role.scope_type,
                       role.is_system, role.is_active, role.version,
                       COUNT(DISTINCT assignment.account_id)::BIGINT AS "assigned_account_count!"
                FROM tenant_roles AS role
                LEFT JOIN account_role_assignments AS assignment
                    ON assignment.tenant_id = role.tenant_id
                   AND assignment.role_code = role.code
                WHERE role.tenant_id = $1
                GROUP BY role.tenant_id, role.code
                ORDER BY role.is_system DESC, lower(role.display_name), role.code
                "#,
                tenant_id,
            ).fetch_all(&mut *connection).await?;
            let role_permissions: Vec<RolePermissionRow> = sqlx::query_as!(RolePermissionRow, "SELECT role_code, permission_code FROM tenant_role_permissions WHERE tenant_id = $1 ORDER BY role_code, permission_code", tenant_id)
                .fetch_all(&mut *connection).await?;
            let users: Vec<UserRow> = sqlx::query_as!(UserRow, "SELECT id AS account_id, username, email, status, primary_role_code, authorization_version FROM accounts WHERE tenant_id = $1 ORDER BY lower(username), id", tenant_id)
                .fetch_all(&mut *connection).await?;
            let assignments: Vec<AssignmentRow> = sqlx::query_as!(AssignmentRow, "SELECT account_id, role_code, branch_id FROM account_role_assignments WHERE tenant_id = $1 ORDER BY account_id, branch_id NULLS FIRST, role_code", tenant_id)
                .fetch_all(&mut *connection).await?;
            let overrides: Vec<OverrideRow> = sqlx::query_as!(OverrideRow, "SELECT account_id, permission_code, branch_id, effect, expires_at FROM account_permission_overrides WHERE tenant_id = $1 ORDER BY account_id, branch_id NULLS FIRST, permission_code", tenant_id)
                .fetch_all(&mut *connection).await?;
            let audit: Vec<AuditRow> = sqlx::query_as!(AuditRow, "SELECT id, actor_account_id, action, object_type, object_id, branch_id, before_value, after_value, created_at FROM access_control_audit_log WHERE tenant_id = $1 ORDER BY created_at DESC, id DESC LIMIT $2", tenant_id, MAX_AUDIT_ROWS)
                .fetch_all(&mut *connection).await?;
            Ok((branches, permissions, roles, role_permissions, users, assignments, overrides, audit))
        })
        .await
        .map_err(|database_error: TenantDbErr| database_error_status("load access-control snapshot", tenant_id, Uuid::nil(), database_error))?;
    snapshot_from_rows(rows)
}

fn snapshot_from_rows(rows: AccessControlSnapshotRows) -> Result<AccessControlSnapshot, AccessControlError> {
    let (
        branch_rows,
        permission_rows,
        role_rows,
        role_permission_rows,
        user_rows,
        assignment_rows,
        override_rows,
        audit_rows,
    ) = rows;
    let mut roles: Vec<AccessControlRole> = role_rows
        .into_iter()
        .map(role_from_row)
        .collect::<Result<Vec<AccessControlRole>, AccessControlError>>()?;
    for role_permission in role_permission_rows {
        let permission_code: PermissionCode = parse_permission(role_permission.permission_code)?;
        if let Some(role) = roles
            .iter_mut()
            .find(|role: &&mut AccessControlRole| role.code.as_str() == role_permission.role_code)
        {
            role.permission_codes.push(permission_code);
        }
    }
    let mut users: Vec<AccessControlUser> = user_rows
        .into_iter()
        .map(user_from_row)
        .collect::<Result<Vec<AccessControlUser>, AccessControlError>>()?;
    for assignment in assignment_rows {
        let role_code: RoleCode = parse_role(assignment.role_code)?;
        if let Some(user) = users
            .iter_mut()
            .find(|user: &&mut AccessControlUser| user.account_id == assignment.account_id)
        {
            user.assignments.push(AccountRoleAssignmentContract {
                role_code,
                branch_id: assignment.branch_id,
            });
        }
    }
    for account_override in override_rows {
        let permission_code: PermissionCode = parse_permission(account_override.permission_code)?;
        let effect: PermissionOverrideEffect = PermissionOverrideEffect::from_code(&account_override.effect)
            .ok_or_else(|| {
                error!(effect = %account_override.effect, "Persisted account permission override has invalid effect");
                AccessControlError::Internal
            })?;
        if let Some(user) = users
            .iter_mut()
            .find(|user: &&mut AccessControlUser| user.account_id == account_override.account_id)
        {
            user.permission_overrides.push(AccountPermissionOverrideContract {
                permission_code,
                branch_id: account_override.branch_id,
                effect,
                expires_at: account_override.expires_at,
            });
        }
    }
    let branches: Vec<AccessControlBranch> = branch_rows.into_iter().map(AccessControlBranch::from).collect();
    let permissions: Vec<AccessControlPermission> = permission_rows
        .into_iter()
        .map(
            |row: PermissionRow| -> Result<AccessControlPermission, AccessControlError> {
                Ok(AccessControlPermission {
                    code: parse_permission(row.code)?,
                    description: row.description,
                })
            },
        )
        .collect::<Result<Vec<AccessControlPermission>, AccessControlError>>()?;
    let audit: Vec<AccessControlAuditEntry> = audit_rows.into_iter().map(AccessControlAuditEntry::from).collect();
    Ok(AccessControlSnapshot {
        branches,
        permissions,
        roles,
        users,
        audit,
    })
}

async fn load_role(
    auth: &AuthService,
    tenant_id: Uuid,
    role_code: &RoleCode,
) -> Result<AccessControlRole, AccessControlError> {
    let snapshot: AccessControlSnapshot = load_snapshot(auth, tenant_id).await?;
    snapshot
        .roles
        .into_iter()
        .find(|role: &AccessControlRole| role.code == *role_code)
        .ok_or_else(|| AccessControlError::NotFound("The tenant role does not exist.".to_owned()))
}

async fn load_user(
    auth: &AuthService,
    tenant_id: Uuid,
    account_id: Uuid,
) -> Result<AccessControlUser, AccessControlError> {
    let snapshot: AccessControlSnapshot = load_snapshot(auth, tenant_id).await?;
    snapshot
        .users
        .into_iter()
        .find(|user: &AccessControlUser| user.account_id == account_id)
        .ok_or_else(|| AccessControlError::NotFound("The tenant account does not exist.".to_owned()))
}

async fn load_role_row(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    role_code: &str,
    lock: bool,
) -> Result<Option<RoleRow>, sqlx::Error> {
    if lock {
        sqlx::query_as!(RoleRow, r#"SELECT code, display_name, description, scope_type, is_system, is_active, version, 0::BIGINT AS "assigned_account_count!" FROM tenant_roles WHERE tenant_id = $1 AND code = $2 FOR UPDATE"#, tenant_id, role_code)
            .fetch_optional(connection).await
    } else {
        sqlx::query_as!(RoleRow, r#"SELECT code, display_name, description, scope_type, is_system, is_active, version, 0::BIGINT AS "assigned_account_count!" FROM tenant_roles WHERE tenant_id = $1 AND code = $2"#, tenant_id, role_code)
            .fetch_optional(connection).await
    }
}

async fn load_user_row(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    account_id: Uuid,
    lock: bool,
) -> Result<Option<UserRow>, sqlx::Error> {
    if lock {
        sqlx::query_as!(UserRow, "SELECT id AS account_id, username, email, status, primary_role_code, authorization_version FROM accounts WHERE tenant_id = $1 AND id = $2 FOR UPDATE", tenant_id, account_id)
            .fetch_optional(connection).await
    } else {
        sqlx::query_as!(UserRow, "SELECT id AS account_id, username, email, status, primary_role_code, authorization_version FROM accounts WHERE tenant_id = $1 AND id = $2", tenant_id, account_id)
            .fetch_optional(connection).await
    }
}

async fn replace_role_permissions(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    role_code: &str,
    permission_codes: &[PermissionCode],
    actor_id: Uuid,
) -> Result<(), sqlx::Error> {
    let permission_values: Vec<String> = permission_codes
        .iter()
        .map(|permission_code: &PermissionCode| permission_code.as_str().to_owned())
        .collect();
    sqlx::query!(
        "DELETE FROM tenant_role_permissions WHERE tenant_id = $1 AND role_code = $2 AND NOT (permission_code = ANY($3))",
        tenant_id,
        role_code,
        permission_values.as_slice(),
    )
        .execute(&mut *connection).await?;
    for permission_code in permission_codes {
        sqlx::query!("INSERT INTO tenant_role_permissions (tenant_id, role_code, permission_code, granted_by_account_id) VALUES ($1, $2, $3, $4) ON CONFLICT (tenant_id, role_code, permission_code) DO NOTHING", tenant_id, role_code, permission_code.as_str(), actor_id)
            .execute(&mut *connection).await?;
    }
    Ok(())
}

async fn validate_primary_role_assignments(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    request: &UpdateAccountAccessRequest,
) -> Result<(), sqlx::Error> {
    let rule: Option<RoleRuleRow> = sqlx::query_as!(
        RoleRuleRow,
        "SELECT min_assignments, max_assignments FROM auth_role_branch_assignment_rules WHERE role_code = $1",
        request.primary_role.as_str()
    )
    .fetch_optional(&mut *connection)
    .await?;
    let Some(rule) = rule else {
        return Err(sqlx::Error::Protocol(
            "Primary organizational role has no branch-assignment rule".to_owned(),
        ));
    };
    let primary_assignments: Vec<&AccountRoleAssignmentContract> = request
        .assignments
        .iter()
        .filter(|assignment: &&AccountRoleAssignmentContract| assignment.role_code == request.primary_role)
        .collect();
    let branch_count: i64 = primary_assignments
        .iter()
        .filter(|assignment: &&&AccountRoleAssignmentContract| assignment.branch_id.is_some())
        .count() as i64;
    let has_tenant_assignment: bool = primary_assignments
        .iter()
        .any(|assignment: &&AccountRoleAssignmentContract| assignment.branch_id.is_none());
    let minimum: i64 = i64::from(rule.min_assignments);
    let maximum: Option<i64> = rule.max_assignments.map(i64::from);
    let valid_tenant_role: bool = maximum == Some(0) && branch_count == 0 && has_tenant_assignment;
    let valid_branch_role: bool = maximum != Some(0)
        && !has_tenant_assignment
        && branch_count >= minimum
        && maximum.is_none_or(|value: i64| branch_count <= value);
    if !valid_tenant_role && !valid_branch_role {
        return Err(sqlx::Error::Protocol(
            "Primary role assignments violate the configured branch cardinality".to_owned(),
        ));
    }
    Ok(())
}

async fn validate_assignment_references(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    request: &UpdateAccountAccessRequest,
) -> Result<(), sqlx::Error> {
    for assignment in &request.assignments {
        let valid: bool = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM tenant_roles AS role
                LEFT JOIN branches AS branch
                    ON branch.tenant_id = role.tenant_id
                   AND branch.id = $3
                   AND branch.status = 'active'
                WHERE role.tenant_id = $1
                  AND role.code = $2
                  AND role.is_active
                  AND (
                      (role.scope_type = 'tenant' AND $3::UUID IS NULL)
                      OR (role.scope_type = 'branch' AND branch.id IS NOT NULL)
                  )
            ) AS "exists!"
            "#,
            tenant_id,
            assignment.role_code.as_str(),
            assignment.branch_id,
        )
        .fetch_one(&mut *connection)
        .await?;
        if !valid {
            return Err(sqlx::Error::Protocol(
                "A role assignment references an inactive role, invalid scope, or inactive branch".to_owned(),
            ));
        }
    }
    for account_override in &request.permission_overrides {
        let valid: bool = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM permissions WHERE code = $2
            ) AND (
                $3::UUID IS NULL OR EXISTS (
                    SELECT 1 FROM branches
                    WHERE tenant_id = $1 AND id = $3 AND status = 'active'
                )
            ) AS "exists!"
            "#,
            tenant_id,
            account_override.permission_code.as_str(),
            account_override.branch_id,
        )
        .fetch_one(&mut *connection)
        .await?;
        if !valid {
            return Err(sqlx::Error::Protocol(
                "A permission override references an unknown permission or inactive branch".to_owned(),
            ));
        }
    }
    Ok(())
}

async fn insert_audit(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    actor_id: Uuid,
    audit: AccessAuditMutation,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO access_control_audit_log (
            tenant_id, actor_account_id, action, object_type, object_id,
            branch_id, before_value, after_value
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
        tenant_id,
        actor_id,
        audit.action,
        audit.object_type,
        audit.object_id,
        audit.branch_id,
        audit.before_value,
        audit.after_value,
    )
    .execute(connection)
    .await?;
    Ok(())
}

async fn invalidate_tenant_accounts(auth: &AuthService, tenant_id: Uuid) {
    invalidate_accounts(auth, tenant_id, None).await;
}

async fn invalidate_role_accounts(auth: &AuthService, tenant_id: Uuid, role_code: &RoleCode) {
    let account_ids: Result<Vec<Uuid>, TenantDbErr> = auth
        .db
        .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
            sqlx::query_scalar!(
                "SELECT DISTINCT account_id FROM account_role_assignments WHERE tenant_id = $1 AND role_code = $2",
                tenant_id,
                role_code.as_str()
            )
            .fetch_all(connection)
            .await
        })
        .await;
    match account_ids {
        Ok(account_ids) => {
            for account_id in account_ids {
                invalidate_accounts(auth, tenant_id, Some(account_id)).await;
            }
        }
        Err(database_error) => {
            error!(operation = "access_control.invalidate_role", tenant_id = %tenant_id, role_code = %role_code, reason = %database_error, "Could not enumerate role accounts for cache invalidation; bounded TTL remains in force")
        }
    }
}

async fn invalidate_accounts(auth: &AuthService, tenant_id: Uuid, account_id: Option<Uuid>) {
    let identities: Result<Vec<IdentityRow>, TenantDbErr> = auth.db.run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
        sqlx::query_as!(IdentityRow, "SELECT issuer, subject FROM account_identities WHERE tenant_id = $1 AND ($2::UUID IS NULL OR account_id = $2)", tenant_id, account_id)
            .fetch_all(connection).await
    }).await;
    let identities: Vec<IdentityRow> = match identities {
        Ok(identities) => identities,
        Err(database_error) => {
            error!(operation = "access_control.invalidate_accounts", tenant_id = %tenant_id, account_id = ?account_id, reason = %database_error, "Could not load identities for cache invalidation; bounded TTL remains in force");
            return;
        }
    };
    for identity in identities {
        let result: Result<(), AuthenticatedUserCacheError> = auth
            .account_cache
            .invalidate(&identity.issuer, &identity.subject, tenant_id)
            .await;
        if let Err(cache_error) = result {
            warn!(operation = "access_control.invalidate_account", tenant_id = %tenant_id, account_id = ?account_id, reason = %cache_error, "Authenticated-user cache invalidation failed; bounded TTL remains in force");
        }
    }
}

fn require_permission(actor: &AuthenticatedUser, permission: &PermissionCode) -> Result<(), AccessControlError> {
    if actor.has_permission(permission.as_str()) {
        Ok(())
    } else {
        warn!(operation = "access_control.authorize", tenant_id = %actor.tenant_id, actor_id = %actor.account_id, required_permission = %permission, "Access-control request denied");
        Err(AccessControlError::Forbidden)
    }
}

fn normalize_branch_request(request: &mut CreateAccessControlBranchRequest) -> Result<(), AccessControlError> {
    request.code = request.code.trim().to_ascii_lowercase();
    request.name = request.name.trim().to_owned();
    request.time_zone = request.time_zone.trim().to_owned();
    if request.code.len() < 2
        || request.code.len() > 63
        || !request.code.chars().all(|character: char| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-' || character == '_'
        })
    {
        return Err(AccessControlError::Validation(
            "Branch code must contain 2-63 lowercase letters, digits, hyphens, or underscores.".to_owned(),
        ));
    }
    if request.name.is_empty() || request.time_zone.is_empty() {
        return Err(AccessControlError::Validation(
            "Branch name and IANA time zone are required.".to_owned(),
        ));
    }
    Ok(())
}

fn normalize_update_branch_request(request: &mut UpdateAccessControlBranchRequest) -> Result<(), AccessControlError> {
    request.name = request.name.trim().to_owned();
    request.time_zone = request.time_zone.trim().to_owned();
    request.status = request.status.trim().to_ascii_lowercase();
    if request.name.is_empty()
        || request.time_zone.is_empty()
        || !matches!(request.status.as_str(), "active" | "disabled")
        || request.expected_version < 1
    {
        return Err(AccessControlError::Validation(
            "Branch name, IANA time zone, valid status, and positive version are required.".to_owned(),
        ));
    }
    Ok(())
}

fn normalize_create_role_request(request: &mut CreateAccessControlRoleRequest) -> Result<(), AccessControlError> {
    request.display_name = request.display_name.trim().to_owned();
    request.description = request
        .description
        .take()
        .map(|description: String| description.trim().to_owned())
        .filter(|description: &String| !description.is_empty());
    normalize_permissions(&mut request.permission_codes);
    if request.display_name.is_empty() {
        return Err(AccessControlError::Validation(
            "Role display name is required.".to_owned(),
        ));
    }
    Ok(())
}

fn normalize_update_role_request(request: &mut UpdateAccessControlRoleRequest) -> Result<(), AccessControlError> {
    request.display_name = request.display_name.trim().to_owned();
    request.description = request
        .description
        .take()
        .map(|description: String| description.trim().to_owned())
        .filter(|description: &String| !description.is_empty());
    normalize_permissions(&mut request.permission_codes);
    if request.display_name.is_empty() || request.expected_version < 1 {
        return Err(AccessControlError::Validation(
            "Role display name and positive version are required.".to_owned(),
        ));
    }
    Ok(())
}

fn normalize_permissions(permission_codes: &mut Vec<PermissionCode>) {
    permission_codes.sort();
    permission_codes.dedup();
}

fn normalize_user_access_request(request: &mut UpdateAccountAccessRequest) -> Result<(), AccessControlError> {
    if request.expected_version < 1 || request.assignments.is_empty() {
        return Err(AccessControlError::Validation(
            "At least one role assignment and a positive authorization version are required.".to_owned(),
        ));
    }
    request.assignments.sort_by(
        |left: &AccountRoleAssignmentContract, right: &AccountRoleAssignmentContract| {
            (left.branch_id, left.role_code.as_str()).cmp(&(right.branch_id, right.role_code.as_str()))
        },
    );
    request.assignments.dedup_by(
        |left: &mut AccountRoleAssignmentContract, right: &mut AccountRoleAssignmentContract| {
            left.branch_id == right.branch_id && left.role_code == right.role_code
        },
    );
    request.permission_overrides.sort_by(
        |left: &AccountPermissionOverrideContract, right: &AccountPermissionOverrideContract| {
            (left.branch_id, left.permission_code.as_str()).cmp(&(right.branch_id, right.permission_code.as_str()))
        },
    );
    request.permission_overrides.dedup_by(
        |left: &mut AccountPermissionOverrideContract, right: &mut AccountPermissionOverrideContract| {
            left.branch_id == right.branch_id && left.permission_code == right.permission_code
        },
    );
    if request
        .permission_overrides
        .iter()
        .any(|account_override: &AccountPermissionOverrideContract| {
            account_override
                .expires_at
                .is_some_and(|expires_at: DateTime<Utc>| expires_at <= Utc::now())
        })
    {
        return Err(AccessControlError::Validation(
            "Permission override expiry must be in the future.".to_owned(),
        ));
    }
    Ok(())
}

fn parse_role(code: String) -> Result<RoleCode, AccessControlError> {
    RoleCode::try_from(code).map_err(|code_error: AuthCodeError| {
        error!(reason = %code_error, "Persisted tenant role code is invalid");
        AccessControlError::Internal
    })
}

fn parse_permission(code: String) -> Result<PermissionCode, AccessControlError> {
    PermissionCode::try_from(code).map_err(|code_error: AuthCodeError| {
        error!(reason = %code_error, "Persisted permission code is invalid");
        AccessControlError::Internal
    })
}

fn role_from_row(row: RoleRow) -> Result<AccessControlRole, AccessControlError> {
    let scope: AccessRoleScope = AccessRoleScope::from_code(&row.scope_type).ok_or_else(|| {
        error!(role_code = %row.code, scope = %row.scope_type, "Persisted tenant role scope is invalid");
        AccessControlError::Internal
    })?;
    Ok(AccessControlRole {
        code: parse_role(row.code)?,
        display_name: row.display_name,
        description: row.description,
        scope,
        is_system: row.is_system,
        is_active: row.is_active,
        version: row.version,
        permission_codes: Vec::new(),
        assigned_account_count: row.assigned_account_count,
    })
}

fn user_from_row(row: UserRow) -> Result<AccessControlUser, AccessControlError> {
    let status: AccountStatus = AccountStatus::from_code(&row.status).ok_or_else(|| {
        error!(account_id = %row.account_id, account_status = %row.status, "Persisted account status is invalid");
        AccessControlError::Internal
    })?;
    Ok(AccessControlUser {
        account_id: row.account_id,
        username: row.username,
        email: row.email,
        status,
        primary_role: parse_role(row.primary_role_code)?,
        authorization_version: row.authorization_version,
        assignments: Vec::new(),
        permission_overrides: Vec::new(),
    })
}

fn branch_value(row: &BranchRow) -> Value {
    json!({ "id": row.id, "code": row.code, "name": row.name, "time_zone": row.time_zone, "status": row.status, "version": row.version })
}

fn role_row_value(row: &RoleRow) -> Value {
    json!({ "code": row.code, "display_name": row.display_name, "description": row.description, "scope": row.scope_type, "is_system": row.is_system, "is_active": row.is_active, "version": row.version })
}

fn database_error_status(
    operation: &str,
    tenant_id: Uuid,
    actor_id: Uuid,
    database_error: TenantDbErr,
) -> AccessControlError {
    let message: String = database_error.to_string();
    error!(operation, tenant_id = %tenant_id, actor_id = %actor_id, reason = %database_error, "Access-control database operation failed");
    if message.contains("duplicate key") {
        AccessControlError::Conflict("That code or assignment already exists.".to_owned())
    } else if message.contains("tenant owner")
        || message.contains("system tenant role")
        || message.contains("branch cardinality")
        || message.contains("Primary organizational role")
        || message.contains("role assignment")
        || message.contains("permission override")
        || message.contains("inactive role")
        || message.contains("invalid scope")
        || message.contains("unknown permission")
    {
        AccessControlError::Validation(message)
    } else {
        AccessControlError::Unavailable
    }
}

impl From<BranchRow> for AccessControlBranch {
    fn from(row: BranchRow) -> Self {
        Self {
            id: row.id,
            code: row.code,
            name: row.name,
            time_zone: row.time_zone,
            status: row.status,
            version: row.version,
        }
    }
}

impl From<AuditRow> for AccessControlAuditEntry {
    fn from(row: AuditRow) -> Self {
        Self {
            id: row.id,
            actor_account_id: row.actor_account_id,
            action: row.action,
            object_type: row.object_type,
            object_id: row.object_id,
            branch_id: row.branch_id,
            before_value: row.before_value,
            after_value: row.after_value,
            created_at: row.created_at,
        }
    }
}
