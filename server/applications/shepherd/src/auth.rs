use std::sync::Arc;

use axum::Router;

pub use infra_auth::{AuthService, PermissionCode, RoleCode};
pub use infra_auth::ext_foundation::account::{AccountStatus, AuthenticatedUser, CurrentUserProfile};
pub use infra_auth::ext_foundation::auth_admin::{
    AuthAdminPolicy, AuthProviderUserStatus, AuthUserSummary, CreateAuthUserRequest, SetAuthUserStatusRequest,
};
pub use infra_auth::ext_foundation::{account::resolve_application_account, middleware::require_authenticated};

fn admin_policy() -> AuthAdminPolicy {
    AuthAdminPolicy::try_new("auth.accounts.read", "auth.accounts.create", "auth.accounts.disable")
        .unwrap_or_else(|code_error| panic!("Shepherd Auth administration permission code is invalid: {code_error}"))
}

pub fn routes(auth: Arc<AuthService>) -> Router {
    let provisioner: Arc<dyn infra_auth::ext_foundation::auth_admin::AuthAccountProvisioner> =
        Arc::new(crate::auth_provisioning::ShepherdAuthAccountProvisioner);
    infra_auth::ext_foundation::routes_with_provisioner(auth, admin_policy(), provisioner)
}
