use std::sync::Arc;

use infra_postgres::{DatabaseAdapter, TenantDbErr};
use serde_json::json;
use sqlx::PgConnection;
use tracing::{debug, error, warn};
use uuid::Uuid;

use crate::branch::core::{
    Branch, BranchCreateRequest, BranchCursor, BranchErr, BranchPage, BranchSummary, BranchUpdateRequest,
};

const MANAGE_PERMISSION: &str = "business.branches.manage";
const TENANT_PERMISSION_REQUIRED: &str = "branch management requires tenant-scoped permission";

pub struct BranchRepo {
    db: Arc<DatabaseAdapter>,
}

impl BranchRepo {
    pub fn new_arc(db: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self { db })
    }

    pub async fn list_active_branches(&self, tenant_id: Uuid) -> Result<Vec<BranchSummary>, BranchErr> {
        let branches: Vec<BranchSummary> = self
            .db
            .tran_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
                sqlx::query_as!(
                    BranchSummary,
                    r#"
                    SELECT id, code, name, time_zone
                    FROM branches
                    WHERE tenant_id = $1
                      AND status = 'active'
                    ORDER BY lower(name), code
                    "#,
                    tenant_id,
                )
                .fetch_all(connection)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| database_failure("list active branches", tenant_id, error))?;
        debug!(
            operation = "branch.list_active",
            tenant_id = %tenant_id,
            branch_count = branches.len(),
            "Active tenant branches loaded"
        );
        Ok(branches)
    }

    pub async fn list_managed_branches(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        search: Option<String>,
        limit: i64,
        cursor: Option<BranchCursor>,
    ) -> Result<BranchPage, BranchErr> {
        let cursor_code = cursor.as_ref().map(|value: &BranchCursor| value.code.clone());
        let cursor_id = cursor.as_ref().map(|value: &BranchCursor| value.id);
        let mut items = self
            .db
            .tran_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
                require_tenant_permission(connection, tenant_id, actor_account_id).await?;
                sqlx::query_as!(
                    Branch,
                    r#"
                    SELECT id, code, name, time_zone, status, version
                    FROM branches
                    WHERE tenant_id = $1
                      AND (
                          $3::TEXT IS NULL
                          OR code LIKE '%' || $3 || '%'
                          OR lower(name) LIKE '%' || $3 || '%'
                      )
                      AND (
                          $4::TEXT IS NULL
                          OR (code, id) > ($4, $5::UUID)
                      )
                    ORDER BY code, id
                    LIMIT $2
                    "#,
                    tenant_id,
                    limit + 1,
                    search,
                    cursor_code,
                    cursor_id,
                )
                .fetch_all(connection)
                .await
            })
            .await
            .map_err(|error: TenantDbErr| mutation_failure("list managed branches", tenant_id, error))?;
        let has_more = items.len() > usize::try_from(limit).unwrap_or(usize::MAX);
        if has_more {
            items.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        }
        let next_cursor = if has_more {
            items.last().map(|branch: &Branch| BranchCursor {
                code: branch.code.clone(),
                id: branch.id,
            })
        } else {
            None
        };
        Ok(BranchPage { items, next_cursor })
    }

    pub async fn create_branch(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        request: BranchCreateRequest,
    ) -> Result<Branch, BranchErr> {
        let branch_id = Uuid::new_v4();
        self.db
            .tran_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
                require_tenant_permission(connection, tenant_id, actor_account_id).await?;
                let inserted = sqlx::query_as!(
                    Branch,
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
                    actor_account_id,
                )
                .fetch_one(&mut *connection)
                .await?;
                let after_value = json!({
                    "id": inserted.id,
                    "code": inserted.code,
                    "name": inserted.name,
                    "time_zone": inserted.time_zone,
                    "status": inserted.status,
                    "version": inserted.version,
                });
                insert_audit(
                    connection,
                    tenant_id,
                    actor_account_id,
                    "branch.create",
                    branch_id,
                    None,
                    Some(after_value),
                )
                .await?;
                Ok(inserted)
            })
            .await
            .map_err(|error: TenantDbErr| mutation_failure("create branch", tenant_id, error))
    }

    pub async fn update_branch(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        branch_id: Uuid,
        request: BranchUpdateRequest,
    ) -> Result<Branch, BranchErr> {
        let result: Option<Branch> = self
            .db
            .tran_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
                require_tenant_permission(connection, tenant_id, actor_account_id).await?;
                let before = sqlx::query_as!(
                    Branch,
                    "SELECT id, code, name, time_zone, status, version FROM branches WHERE tenant_id = $1 AND id = $2 FOR UPDATE",
                    tenant_id,
                    branch_id,
                )
                .fetch_optional(&mut *connection)
                .await?;
                let Some(before) = before else {
                    return Ok(None);
                };
                let updated = sqlx::query_as!(
                    Branch,
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
                    actor_account_id,
                    request.expected_version,
                )
                .fetch_optional(&mut *connection)
                .await?;
                if let Some(updated) = &updated {
                    let before_value = json!({
                        "id": before.id,
                        "code": before.code,
                        "name": before.name,
                        "time_zone": before.time_zone,
                        "status": before.status,
                        "version": before.version,
                    });
                    let after_value = json!({
                        "id": updated.id,
                        "code": updated.code,
                        "name": updated.name,
                        "time_zone": updated.time_zone,
                        "status": updated.status,
                        "version": updated.version,
                    });
                    insert_audit(
                        connection,
                        tenant_id,
                        actor_account_id,
                        "branch.update",
                        branch_id,
                        Some(before_value),
                        Some(after_value),
                    )
                    .await?;
                }
                Ok(updated)
            })
            .await
            .map_err(|error: TenantDbErr| mutation_failure("update branch", tenant_id, error))?;
        result.ok_or(BranchErr::Conflict)
    }
}

async fn require_tenant_permission(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    actor_account_id: Uuid,
) -> Result<(), sqlx::Error> {
    let allowed = sqlx::query_scalar!(
        r#"SELECT shepherd_account_has_tenant_permission($1, $2, $3) AS "allowed!""#,
        tenant_id,
        actor_account_id,
        MANAGE_PERMISSION,
    )
    .fetch_one(connection)
    .await?;
    if allowed {
        Ok(())
    } else {
        Err(sqlx::Error::Protocol(TENANT_PERMISSION_REQUIRED.to_owned()))
    }
}

async fn insert_audit(
    connection: &mut PgConnection,
    tenant_id: Uuid,
    actor_account_id: Uuid,
    action: &str,
    branch_id: Uuid,
    before_value: Option<serde_json::Value>,
    after_value: Option<serde_json::Value>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO access_control_audit_log (
            tenant_id, actor_account_id, action, object_type, object_id,
            branch_id, before_value, after_value
        )
        VALUES ($1, $2, $3, 'branch', $4, $5, $6, $7)
        "#,
        tenant_id,
        actor_account_id,
        action,
        branch_id.to_string(),
        branch_id,
        before_value,
        after_value,
    )
    .execute(connection)
    .await?;
    Ok(())
}

fn database_failure(operation: &str, tenant_id: Uuid, error: TenantDbErr) -> BranchErr {
    error!(operation, tenant_id = %tenant_id, reason = %error, "Branch database query failed");
    BranchErr::BackendUnavailable
}

fn mutation_failure(operation: &str, tenant_id: Uuid, error: TenantDbErr) -> BranchErr {
    let mapped = match &error {
        TenantDbErr::Sqlx(sqlx::Error::Protocol(message)) if message == TENANT_PERMISSION_REQUIRED => {
            BranchErr::Forbidden
        }
        TenantDbErr::Sqlx(sqlx::Error::Database(database_error)) if database_error.is_unique_violation() => {
            BranchErr::Conflict
        }
        TenantDbErr::Sqlx(sqlx::Error::Database(database_error))
            if database_error.is_check_violation() || database_error.is_foreign_key_violation() =>
        {
            BranchErr::InvalidInput("branch data violates a database constraint")
        }
        _ => BranchErr::BackendUnavailable,
    };
    if matches!(mapped, BranchErr::BackendUnavailable) {
        error!(operation, tenant_id = %tenant_id, reason = %error, "Branch mutation failed unexpectedly");
    } else {
        warn!(operation, tenant_id = %tenant_id, reason = %error, "Branch mutation rejected");
    }
    mapped
}
