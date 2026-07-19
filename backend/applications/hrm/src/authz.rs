use std::sync::Arc;

use foundation_authz::{AuthzError, AuthzService};

pub mod action {
    pub const APPROVE: &str = "approve";
    pub const READ: &str = "read";
}

pub mod resource {
    pub const EMPLOYEES: &str = "hrm.employees";
    pub const PAYROLL: &str = "hrm.payroll";
}

/// HRM owns this Casbin model. Foundation-authz only evaluates its four
/// opaque request fields: account, tenant, HRM resource, and action.
pub const MODEL: &str = include_str!("authz/model.conf");

/// Demonstration-only policy showing tenant-scoped HRM roles. Runtime policy
/// will be loaded from the persistent adapter rather than this file.
pub const DEMO_POLICY: &str = include_str!("authz/demo_policy.csv");

pub async fn demo_authz() -> Result<Arc<AuthzService>, AuthzError> {
    AuthzService::from_model_and_policy(MODEL, DEMO_POLICY).await
}

#[cfg(test)]
mod tests {
    use foundation_authz::AuthorizationRequest;

    use super::{action, demo_authz, resource};

    const ACME_TENANT: &str = "00000000-0000-4000-8000-000000000001";
    const OTHER_TENANT: &str = "00000000-0000-4000-8000-000000000002";
    const SUPERVISOR_ACCOUNT: &str = "00000000-0000-4000-8000-000000000101";

    #[tokio::test]
    async fn demo_supervisor_can_read_employees_only_in_acme() -> Result<(), Box<dyn std::error::Error>> {
        let authz = demo_authz().await?;

        assert!(
            authz
                .is_allowed(&AuthorizationRequest::new(
                    SUPERVISOR_ACCOUNT,
                    ACME_TENANT,
                    resource::EMPLOYEES,
                    action::READ,
                ))
                .await?
        );
        assert!(
            !authz
                .is_allowed(&AuthorizationRequest::new(
                    SUPERVISOR_ACCOUNT,
                    OTHER_TENANT,
                    resource::EMPLOYEES,
                    action::READ,
                ))
                .await?
        );
        assert!(
            !authz
                .is_allowed(&AuthorizationRequest::new(
                    SUPERVISOR_ACCOUNT,
                    ACME_TENANT,
                    resource::PAYROLL,
                    action::APPROVE,
                ))
                .await?
        );
        Ok(())
    }
}
