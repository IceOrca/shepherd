use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::handler::login,
        crate::handler::refresh_session,
        crate::handler::get_profile,
        crate::handler::logout,
        crate::handler::logout_all,
        crate::handler::register_new_user,
        crate::handler::list_accounts,
        crate::handler::get_authorization_catalog,
        crate::handler::change_own_password,
        crate::handler::reset_account_password,
        crate::handler::update_account_status,
        crate::handler::update_account_roles,
        crate::handler::update_account_permissions
    ),
    components(schemas(
        crate::dto::AccessClaims,
        crate::dto::AuthProfileResponse,
        crate::dto::AuthRequest,
        crate::dto::AuthResponse,
        crate::dto::InvalidCredentialsResponse,
        crate::dto::MessageResponse,
        crate::dto::RegisterUserRequest,
        crate::dto::ChangePasswordRequest,
        crate::dto::ResetPasswordRequest,
        crate::dto::UpdateAccountStatusRequest,
        crate::dto::UpdateAccountRolesRequest,
        crate::dto::UpdateAccountPermissionsRequest,
        crate::account::Role,
        crate::account::AccountStatus,
        crate::account::AccountSummary,
        crate::account::AccountPermission,
        crate::account::PermissionEffect,
        crate::account::AuthorizationCatalog,
        crate::account::RoleSummary,
        crate::account::PermissionSummary
    )),
    tags((name = "auth", description = "Tenant-scoped authentication and session APIs"))
)]
pub struct AuthApiDoc;
