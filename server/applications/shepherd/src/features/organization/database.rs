use std::sync::Arc;

use async_trait::async_trait;
use crate::features::organization::core::{BranchSummary, FacilitySummary, OrganizationError, OrganizationRepo};
use infra_postgres::{DatabaseAdapter, TenantDbErr};
use sqlx::PgConnection;
use tracing::{error, warn, info, debug, trace};
use uuid::Uuid;

pub struct OrganizationProvider {
    db: Arc<DatabaseAdapter>,
}

impl OrganizationProvider {
    pub fn new_arc(db: Arc<DatabaseAdapter>) -> Arc<Self> {
        Arc::new(Self { db })
    }
}

#[async_trait]
impl OrganizationRepo for OrganizationProvider {
    async fn list_active_branches(&self, tenant_id: Uuid) -> Result<Vec<BranchSummary>, OrganizationError> {
        let branches: Vec<BranchSummary> = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
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
            .map_err(|database_error: TenantDbErr| {
                error!(
                    operation = "organization.list_active_branches",
                    tenant_id = %tenant_id,
                    reason = %database_error,
                    "Active branch list tenant operation failed"
                );
                OrganizationError::BackendUnavailable
            })?;
        info!(
            "Active tenant branches loaded: tenant_id={} branches={}",
            tenant_id,
            branches.len()
        );
        Ok(branches)
    }

    async fn list_active_facilities(&self, tenant_id: Uuid) -> Result<Vec<FacilitySummary>, OrganizationError> {
        let facilities: Vec<FacilitySummary> = self
            .db
            .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
                sqlx::query_as!(
                    FacilitySummary,
                    r#"
                    SELECT facility.id, facility.branch_id, facility.code, facility.name
                    FROM facilities AS facility
                    INNER JOIN branches AS branch
                        ON branch.tenant_id = facility.tenant_id
                       AND branch.id = facility.branch_id
                       AND branch.status = 'active'
                    WHERE facility.tenant_id = $1
                      AND facility.status = 'active'
                    ORDER BY lower(branch.name), lower(facility.name), facility.code
                    "#,
                    tenant_id,
                )
                .fetch_all(connection)
                .await
            })
            .await
            .map_err(|database_error: TenantDbErr| {
                error!(
                    operation = "organization.list_active_facilities",
                    tenant_id = %tenant_id,
                    reason = %database_error,
                    "Active facility list tenant operation failed"
                );
                OrganizationError::BackendUnavailable
            })?;
        info!(
            "Active tenant facilities loaded: tenant_id={} facilities={}",
            tenant_id,
            facilities.len()
        );
        Ok(facilities)
    }
}
