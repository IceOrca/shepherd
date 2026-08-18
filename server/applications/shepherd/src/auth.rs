use std::sync::Arc;

use axum::Router;

pub use infra_auth::AuthService;
pub use infra_auth::ext_foundation::account::{AuthenticatedUser, CurrentUserProfile};
pub use infra_auth::ext_foundation::auth_admin::{
    AuthAdminPolicy, AuthUserSummary, CreateAuthUserRequest, SetAuthUserStatusRequest,
};
pub use infra_auth::ext_foundation::{account::resolve_application_account, middleware::require_authenticated};

pub const ADMIN_POLICY: AuthAdminPolicy = AuthAdminPolicy {
    read_permission: "auth.accounts.read",
    create_permission: "auth.accounts.create",
    disable_permission: "auth.accounts.disable",
};

pub fn routes(auth: Arc<AuthService>) -> Router {
    let provisioner: Arc<dyn infra_auth::ext_foundation::auth_admin::AuthAccountProvisioner> =
        Arc::new(crate::auth_provisioning::ShepherdAuthAccountProvisioner);
    infra_auth::ext_foundation::routes_with_provisioner(auth, ADMIN_POLICY, provisioner)
}
