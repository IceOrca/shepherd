use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::database::BranchRepo;

#[derive(Clone, Debug, Serialize, TS)]
pub struct BranchSummary {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub time_zone: String,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct Branch {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub time_zone: String,
    pub status: String,
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize, TS)]
pub struct BranchCreateRequest {
    pub code: String,
    pub name: String,
    pub time_zone: String,
}

#[derive(Clone, Debug, Deserialize, TS)]
pub struct BranchUpdateRequest {
    pub name: String,
    pub time_zone: String,
    pub status: String,
    pub expected_version: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BranchCursor {
    pub code: String,
    pub id: Uuid,
}

pub struct BranchPage {
    pub items: Vec<Branch>,
    pub next_cursor: Option<BranchCursor>,
}

#[derive(Debug)]
pub enum BranchErr {
    Forbidden,
    Conflict,
    InvalidInput(&'static str),
    BackendUnavailable,
}

pub struct BranchService {
    repo: Arc<BranchRepo>,
}

impl BranchService {
    pub fn new_arc(repo: Arc<BranchRepo>) -> Arc<Self> {
        Arc::new(Self { repo })
    }

    pub async fn list_active_branches(&self, tenant_id: Uuid) -> Result<Vec<BranchSummary>, BranchErr> {
        self.repo.list_active_branches(tenant_id).await
    }

    pub async fn list_managed_branches(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        search: Option<String>,
        limit: i64,
        cursor: Option<BranchCursor>,
    ) -> Result<BranchPage, BranchErr> {
        self.repo
            .list_managed_branches(tenant_id, actor_account_id, search, limit, cursor)
            .await
    }

    pub async fn create_branch(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        mut request: BranchCreateRequest,
    ) -> Result<Branch, BranchErr> {
        normalize_create_request(&mut request)?;
        self.repo.create_branch(tenant_id, actor_account_id, request).await
    }

    pub async fn update_branch(
        &self,
        tenant_id: Uuid,
        actor_account_id: Uuid,
        branch_id: Uuid,
        mut request: BranchUpdateRequest,
    ) -> Result<Branch, BranchErr> {
        normalize_update_request(&mut request)?;
        self.repo
            .update_branch(tenant_id, actor_account_id, branch_id, request)
            .await
    }
}

fn normalize_create_request(request: &mut BranchCreateRequest) -> Result<(), BranchErr> {
    request.code = request.code.trim().to_ascii_lowercase();
    request.name = request.name.trim().to_owned();
    request.time_zone = request.time_zone.trim().to_owned();
    if request.code.len() < 2
        || request.code.len() > 63
        || !request
            .code
            .starts_with(|character: char| character.is_ascii_lowercase() || character.is_ascii_digit())
        || !request
            .code
            .ends_with(|character: char| character.is_ascii_lowercase() || character.is_ascii_digit())
        || !request.code.chars().all(|character: char| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-' || character == '_'
        })
    {
        return Err(BranchErr::InvalidInput("invalid branch code"));
    }
    validate_name_and_time_zone(&request.name, &request.time_zone)
}

fn normalize_update_request(request: &mut BranchUpdateRequest) -> Result<(), BranchErr> {
    request.name = request.name.trim().to_owned();
    request.time_zone = request.time_zone.trim().to_owned();
    request.status = request.status.trim().to_ascii_lowercase();
    validate_name_and_time_zone(&request.name, &request.time_zone)?;
    if !matches!(request.status.as_str(), "active" | "disabled") || request.expected_version < 1 {
        return Err(BranchErr::InvalidInput("invalid branch status or expected version"));
    }
    Ok(())
}

fn validate_name_and_time_zone(name: &str, time_zone: &str) -> Result<(), BranchErr> {
    if name.is_empty() || name.len() > 200 || time_zone.is_empty() || time_zone.len() > 64 {
        return Err(BranchErr::InvalidInput("invalid branch name or IANA time zone"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{BranchCreateRequest, BranchErr, normalize_create_request};

    #[test]
    fn create_request_is_normalized() {
        let mut request = BranchCreateRequest {
            code: "  HCM-01  ".to_owned(),
            name: "  Hồ Chí Minh  ".to_owned(),
            time_zone: "  Asia/Ho_Chi_Minh  ".to_owned(),
        };

        normalize_create_request(&mut request).expect("valid branch request must normalize");

        assert_eq!(request.code, "hcm-01");
        assert_eq!(request.name, "Hồ Chí Minh");
        assert_eq!(request.time_zone, "Asia/Ho_Chi_Minh");
    }

    #[test]
    fn create_request_rejects_edge_separators() {
        for code in ["-hcm", "hcm-", "_hcm", "hcm_"] {
            let mut request = BranchCreateRequest {
                code: code.to_owned(),
                name: "Hồ Chí Minh".to_owned(),
                time_zone: "Asia/Ho_Chi_Minh".to_owned(),
            };

            assert!(matches!(
                normalize_create_request(&mut request),
                Err(BranchErr::InvalidInput("invalid branch code"))
            ));
        }
    }
}
