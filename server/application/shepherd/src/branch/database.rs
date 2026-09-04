use std::sync::Arc;

use async_trait::async_trait;
use crate::branch::core::{BranchSummary, BranchErr};
use infra_postgres::{DatabaseAdapter, TenantDbErr};
use sqlx::PgConnection;
use tracing::{error, warn, info, debug, trace};
use uuid::Uuid;

pub struct BranchRepo {
    db: Arc<DatabaseAdapter>,
}

impl BranchRepo {
    pub fn new_arc(db: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self { db })
    }
}

impl BranchRepo {
    pub async fn list_active_branches(&self, tenant_id: Uuid) -> Result<Vec<BranchSummary>, BranchErr> {
        let branches: Vec<BranchSummary> = self
            .db
            .tran_with_tenant(tenant_id, async move |conn: &mut PgConnection| {
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
                .fetch_all(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| {
                error!(
                    operation = "branch.list_active_branches",
                    tenant_id = %tenant_id,
                    reason = %err,
                    "Active branch list tenant operation failed"
                );
                BranchErr::BackendUnavailable
            })?;
        debug!(
            "Active tenant branches loaded: tenant_id={} branches={}",
            tenant_id,
            branches.len()
        );
        Ok(branches)
    }
}
