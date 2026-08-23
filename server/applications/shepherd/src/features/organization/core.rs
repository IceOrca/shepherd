use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, TS)]
pub struct BranchSummary {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub time_zone: String,
}

#[derive(Debug)]
pub enum OrganizationError {
    BackendUnavailable,
}

#[async_trait]
pub trait OrganizationRepo {
    async fn list_active_branches(&self, tenant_id: Uuid) -> Result<Vec<BranchSummary>, OrganizationError>;
}

pub type DynOrganizationRepo = Arc<dyn OrganizationRepo + Send + Sync>;

pub struct OrganizationService {
    repository: DynOrganizationRepo,
}

impl OrganizationService {
    pub fn new_arc(repository: DynOrganizationRepo) -> Arc<Self> {
        Arc::new(Self { repository })
    }

    pub async fn list_active_branches(&self, tenant_id: Uuid) -> Result<Vec<BranchSummary>, OrganizationError> {
        self.repository.list_active_branches(tenant_id).await
    }
}
