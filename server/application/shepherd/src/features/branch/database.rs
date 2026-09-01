use std::sync::Arc;

use async_trait::async_trait;
use crate::features::branch::core::{BranchSummary, BranchErr, BranchRepo};
use infra_postgres::{DatabaseAdapter, TenantDbErr};
use sqlx::PgConnection;
use tracing::{error, warn, info, debug, trace};
use uuid::Uuid;

pub struct BranchDb {
    db: Arc<DatabaseAdapter>,
}

impl BranchDb {
    pub fn new_arc(db: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self { db })
    }
}

#[async_trait]
impl BranchRepo for BranchDb {
    async fn list_active_branches(&self, tenant_id: Uuid) -> Result<Vec<BranchSummary>, BranchErr> {
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
            .map_err(|err: TenantDbErr| {
                error!(
                    operation = "branch.list_active_branches",
                    tenant_id = %tenant_id,
                    reason = %err,
                    "Active branch list tenant operation failed"
                );
                BranchErr::BackendUnavailable
            })?;
        info!(
            "Active tenant branches loaded: tenant_id={} branches={}",
            tenant_id,
            branches.len()
        );
        Ok(branches)
    }
}
