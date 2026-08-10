use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::account::{AccountPermission, AccountStatus, Role};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Validate, ToSchema)]
pub struct AuthRequest {
    /// Human-readable workspace slug from the platform tenant registry.
    #[validate(length(min = 2, max = 63, message = "Tenant must be between 2 and 63 characters"))]
    pub tenant: String,
    #[validate(length(min = 3, max = 128, message = "Username must be between 3 and 128 characters"))]
    pub username: String,
    #[validate(length(min = 8, max = 256, message = "Password must be between 8 and 256 characters"))]
    pub passphrase: String,
}

impl AuthRequest {
    pub fn normalized_tenant(&self) -> Option<String> {
        normalize_tenant(&self.tenant)
    }

    pub fn username_is_valid(&self) -> bool {
        username_is_valid(&self.username)
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AuthResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct AccessClaims {
    /// Account UUID from the shared accounts table.
    pub sub: String,
    /// Tenant UUID from the shared tenants table.
    pub tid: String,
    pub iss: String,
    pub aud: String,
    pub exp: usize,
    pub nbf: usize,
    pub iat: usize,
    pub jti: String,
    pub sid: String,
    pub username: String,
    pub role: Role,
    pub roles: Vec<String>,
    /// Account authorization version at token issuance.
    pub ver: i64,
    pub permissions: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct RegisterUserRequest {
    #[validate(length(min = 3, max = 128, message = "Username must be between 3 and 128 characters"))]
    pub username: String,
    #[validate(length(min = 8, max = 256, message = "Password must be between 8 and 256 characters"))]
    pub passphrase: String,
    pub role: Role,
}

impl RegisterUserRequest {
    pub fn username_is_blank(&self) -> bool {
        self.username.trim().is_empty()
    }

    pub fn username_is_valid(&self) -> bool {
        username_is_valid(&self.username)
    }
}

fn username_is_valid(username: &str) -> bool {
    let normalized: &str = username.trim();
    (3..=128).contains(&normalized.len())
        && normalized
            .chars()
            .all(|character: char| !character.is_whitespace() && !character.is_control())
}

fn normalize_tenant(tenant: &str) -> Option<String> {
    let normalized: String = tenant.trim().to_ascii_lowercase();
    if !(2..=63).contains(&normalized.len())
        || !normalized
            .bytes()
            .all(|byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || !normalized
            .as_bytes()
            .first()
            .is_some_and(|byte: &u8| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        || !normalized
            .as_bytes()
            .last()
            .is_some_and(|byte: &u8| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return None;
    }

    Some(normalized)
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MessageResponse {
    pub msg: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthProfileResponse {
    pub tenant_id: String,
    pub account_id: String,
    pub username: String,
    pub role: Role,
    pub roles: Vec<String>,
    pub auth_version: i64,
    pub permissions: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateAccountStatusRequest {
    pub status: AccountStatus,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateAccountRolesRequest {
    pub primary_role: Role,
    #[schema(max_items = 64)]
    pub roles: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateAccountPermissionsRequest {
    #[schema(max_items = 256)]
    pub permissions: Vec<AccountPermission>,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ChangePasswordRequest {
    #[validate(length(min = 8, max = 256))]
    pub current_passphrase: String,
    #[validate(length(min = 8, max = 256))]
    pub new_passphrase: String,
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct ResetPasswordRequest {
    #[validate(length(min = 8, max = 256))]
    pub new_passphrase: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InvalidCredentialsResponse {
    pub error: String,
    pub remaining_attempts: u32,
}

#[cfg(test)]
mod tests {
    use validator::Validate;

    use super::AuthRequest;

    #[test]
    fn login_rejects_an_invalid_tenant_slug() {
        let payload = AuthRequest {
            tenant: "not a tenant".to_owned(),
            username: "alice".to_owned(),
            passphrase: "valid-password".to_owned(),
        };

        assert!(payload.validate().is_ok());
        assert!(payload.normalized_tenant().is_none());
    }

    #[test]
    fn login_validates_the_trimmed_username_length() {
        let payload = AuthRequest {
            tenant: "acme1".to_owned(),
            username: " a ".to_owned(),
            passphrase: "valid-password".to_owned(),
        };

        assert!(payload.validate().is_ok());
        assert!(!payload.username_is_valid());
    }

    #[test]
    fn login_normalizes_a_human_readable_tenant_slug() {
        let payload = AuthRequest {
            tenant: "  Acme-1  ".to_owned(),
            username: "alice".to_owned(),
            passphrase: "valid-password".to_owned(),
        };

        assert_eq!(payload.normalized_tenant().as_deref(), Some("acme-1"));
    }
}
