#![cfg_attr(debug_assertions, allow(unused))]

use std::sync::Arc;

use casbin::{CoreApi, DefaultModel, Enforcer, MemoryAdapter, MgmtApi, StringAdapter};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub enum AuthzError {
    #[error("Casbin authorization failed: {0}")]
    Casbin(#[from] casbin::Error),
    #[cfg(feature = "postgres")]
    #[error("Casbin PostgreSQL adapter failed: {0}")]
    PostgresAdapter(String),
}

/// Tenant-aware authorization input. Resource and action meanings belong to
/// the application; the infra treats both as opaque strings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizationRequest {
    pub subject: String,
    pub tenant: String,
    pub resource: String,
    pub action: String,
}

impl AuthorizationRequest {
    pub fn new(
        subject: impl Into<String>,
        tenant: impl Into<String>,
        resource: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            tenant: tenant.into(),
            resource: resource.into(),
            action: action.into(),
        }
    }
}

/// Reusable Casbin engine. Applications own the model and policies supplied
/// to it; this type only owns safe concurrent access and enforcement.
pub struct AuthzService {
    enforcer: RwLock<Enforcer>,
}

impl AuthzService {
    pub fn from_enforcer(enforcer: Enforcer) -> Arc<Self> {
        Arc::new(Self {
            enforcer: RwLock::new(enforcer),
        })
    }

    pub async fn from_model(model: &str) -> Result<Arc<Self>, AuthzError> {
        let model: DefaultModel = DefaultModel::from_str(model).await?;
        let enforcer: Enforcer = Enforcer::new(model, MemoryAdapter::default()).await?;
        Ok(Self::from_enforcer(enforcer))
    }

    /// Build an in-memory engine from application-owned model and policy text.
    /// This is useful for tests and demos; use a persistent adapter in runtime.
    pub async fn from_model_and_policy(model: &str, policy: &str) -> Result<Arc<Self>, AuthzError> {
        let model: DefaultModel = DefaultModel::from_str(model).await?;
        let enforcer: Enforcer = Enforcer::new(model, StringAdapter::new(policy)).await?;
        Ok(Self::from_enforcer(enforcer))
    }

    #[cfg(feature = "postgres")]
    pub async fn from_postgres(model: &str, database_url: &str, max_connections: u32) -> Result<Arc<Self>, AuthzError> {
        let model: DefaultModel = DefaultModel::from_str(model).await?;
        let adapter: sqlx_adapter::SqlxAdapter = sqlx_adapter::SqlxAdapter::new(database_url, max_connections)
            .await
            .map_err(|error| AuthzError::PostgresAdapter(error.to_string()))?;
        let enforcer: Enforcer = Enforcer::new(model, adapter).await?;
        Ok(Self::from_enforcer(enforcer))
    }

    pub async fn is_allowed(&self, request: &AuthorizationRequest) -> Result<bool, AuthzError> {
        let enforcer = self.enforcer.read().await;
        Ok(enforcer.enforce((
            request.subject.as_str(),
            request.tenant.as_str(),
            request.resource.as_str(),
            request.action.as_str(),
        ))?)
    }

    pub async fn add_policy(&self, policy: Vec<String>) -> Result<bool, AuthzError> {
        let mut enforcer = self.enforcer.write().await;
        Ok(enforcer.add_named_policy("p", policy).await?)
    }

    pub async fn add_grouping_policy(&self, policy: Vec<String>) -> Result<bool, AuthzError> {
        let mut enforcer = self.enforcer.write().await;
        Ok(enforcer.add_named_grouping_policy("g", policy).await?)
    }

    pub async fn reload_policy(&self) -> Result<(), AuthzError> {
        let mut enforcer = self.enforcer.write().await;
        enforcer.load_policy().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{AuthorizationRequest, AuthzService};

    const MODEL: &str = r#"
[request_definition]
r = sub, dom, obj, act

[policy_definition]
p = sub, dom, obj, act

[role_definition]
g = _, _, _

[policy_effect]
e = some(where (p.eft == allow))

[matchers]
m = g(r.sub, p.sub, r.dom) && r.dom == p.dom && r.obj == p.obj && r.act == p.act
"#;

    #[tokio::test]
    async fn keeps_role_membership_inside_its_tenant_domain() -> Result<(), Box<dyn std::error::Error>> {
        let authz = AuthzService::from_model(MODEL).await?;
        authz
            .add_policy(vec![
                "manager".to_owned(),
                "tenant-a".to_owned(),
                "employees".to_owned(),
                "read".to_owned(),
            ])
            .await?;
        authz
            .add_grouping_policy(vec!["alice".to_owned(), "manager".to_owned(), "tenant-a".to_owned()])
            .await?;

        assert!(
            authz
                .is_allowed(&AuthorizationRequest::new("alice", "tenant-a", "employees", "read"))
                .await?
        );
        assert!(
            !authz
                .is_allowed(&AuthorizationRequest::new("alice", "tenant-b", "employees", "read"))
                .await?
        );
        Ok(())
    }
}
