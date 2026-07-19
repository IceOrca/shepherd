use std::sync::Arc;

use async_trait::async_trait;
use foundation_kernel::debug::*;
use crate::features::organization::core::{BranchSummary, FacilitySummary, OrganizationError, OrganizationRepo};
use uuid::Uuid;

use foundation_postgres::DatabaseAdapter;

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
        let mut transaction = self.db.begin_tenant(tenant_id).await.map_err(|error| {
            log_error!(
                "Branch list tenant transaction failed: tenant_id={} error={}",
                tenant_id,
                error
            );
            OrganizationError::BackendUnavailable
        })?;
        let branches: Vec<BranchSummary> = sqlx::query_as!(
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
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| {
            log_error!("Branch list failed: tenant_id={} error={}", tenant_id, error);
            OrganizationError::BackendUnavailable
        })?;
        transaction.commit().await.map_err(|error| {
            log_error!(
                "Branch list transaction commit failed: tenant_id={} error={}",
                tenant_id,
                error
            );
            OrganizationError::BackendUnavailable
        })?;
        log_info!(
            "Active tenant branches loaded: tenant_id={} branches={}",
            tenant_id,
            branches.len()
        );
        Ok(branches)
    }

    async fn list_active_facilities(&self, tenant_id: Uuid) -> Result<Vec<FacilitySummary>, OrganizationError> {
        let mut transaction = self.db.begin_tenant(tenant_id).await.map_err(|error| {
            log_error!(
                "Facility list tenant transaction failed: tenant_id={} error={}",
                tenant_id,
                error
            );
            OrganizationError::BackendUnavailable
        })?;
        let facilities: Vec<FacilitySummary> = sqlx::query_as!(
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
        .fetch_all(transaction.connection())
        .await
        .map_err(|error| {
            log_error!("Facility list failed: tenant_id={} error={}", tenant_id, error);
            OrganizationError::BackendUnavailable
        })?;
        transaction.commit().await.map_err(|error| {
            log_error!(
                "Facility list transaction commit failed: tenant_id={} error={}",
                tenant_id,
                error
            );
            OrganizationError::BackendUnavailable
        })?;
        log_info!(
            "Active tenant facilities loaded: tenant_id={} facilities={}",
            tenant_id,
            facilities.len()
        );
        Ok(facilities)
    }
}
