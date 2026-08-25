use std::sync::Arc;

use axum::Router;

pub use infra_auth::{AuthService, PermissionCode, RoleCode};
pub use infra_auth::ext_service::account::{AccountStatus, AuthenticatedUser, CurrentUserProfile, TenantMembershipSummary};
pub use infra_auth::ext_service::access_control::{
    AccessControlAuditEntry, AccessControlBranch, AccessControlPermission, AccessControlRole, AccessControlSnapshot,
    AccessControlUser, AccessRoleScope, AccountPermissionOverrideContract, AccountRoleAssignmentContract,
    CreateAccessControlBranchRequest, CreateAccessControlRoleRequest, PermissionOverrideEffect,
    UpdateAccessControlBranchRequest, UpdateAccessControlRoleRequest, UpdateAccountAccessRequest,
};
pub use infra_auth::ext_service::auth_admin::{
    AuthAdminPolicy, AuthProviderUserStatus, AuthUserSummary, CreateAuthUserRequest, SetAuthUserStatusRequest,
};
pub use infra_auth::ext_service::{account::resolve_application_account, middleware::require_authenticated};

fn admin_policy() -> AuthAdminPolicy {
    AuthAdminPolicy::try_new(
        "auth.accounts.read",
        "auth.accounts.create",
        "auth.accounts.update",
        "auth.accounts.disable",
        "auth.roles.read",
        "auth.roles.manage",
        "business.branches.manage",
    )
    .unwrap_or_else(|code_error| panic!("Shepherd Auth administration permission code is invalid: {code_error}"))
}

pub fn routes(auth: Arc<AuthService>) -> Router {
    let provisioner: Arc<dyn infra_auth::ext_service::auth_admin::AuthAccountProvisioner> =
        Arc::new(crate::auth_provisioning::ShepherdAuthAccountProvisioner);
    infra_auth::ext_service::routes_with_provisioner(auth, admin_policy(), provisioner)
}

pub fn identity_routes(auth: Arc<AuthService>) -> Router {
    infra_auth::ext_service::identity_routes(auth)
}
