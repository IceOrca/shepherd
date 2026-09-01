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
pub enum BranchErr {
    BackendUnavailable,
}

#[async_trait]
pub trait BranchRepo {
    async fn list_active_branches(&self, tenant_id: Uuid) -> Result<Vec<BranchSummary>, BranchErr>;
}

pub type DynBranchRepo = Arc<dyn BranchRepo + Send + Sync>;

pub struct BranchService {
    repo: DynBranchRepo,
}

impl BranchService {
    pub fn new_arc(repo: DynBranchRepo) -> Arc<Self> {
        Arc::new(Self { repo })
    }

    pub async fn list_active_branches(&self, tenant_id: Uuid) -> Result<Vec<BranchSummary>, BranchErr> {
        self.repo.list_active_branches(tenant_id).await
    }
}
