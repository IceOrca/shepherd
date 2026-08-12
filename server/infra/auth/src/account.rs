use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(feature = "password-auth")]
use chrono::{DateTime, Utc};
#[cfg(feature = "password-auth")]
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "password-auth", derive(TS))]
#[serde(rename_all = "snake_case")]
pub enum Role {
    TenantOwner,
    Supervisor,
    Employee,
}

impl Role {
    pub fn as_code(&self) -> &'static str {
        match self {
            Self::TenantOwner => "tenant_owner",
            Self::Supervisor => "supervisor",
            Self::Employee => "employee",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "tenant_owner" => Some(Self::TenantOwner),
            "supervisor" => Some(Self::Supervisor),
            "employee" => Some(Self::Employee),
            _ => None,
        }
    }
}

#[cfg(feature = "password-auth")]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    Active,
    Locked,
    Disabled,
}

#[cfg(feature = "password-auth")]
impl AccountStatus {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Locked => "locked",
            Self::Disabled => "disabled",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "active" => Some(Self::Active),
            "locked" => Some(Self::Locked),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }
}

#[cfg(feature = "password-auth")]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
pub enum PermissionEffect {
    Allow,
    Deny,
}

#[cfg(feature = "password-auth")]
impl PermissionEffect {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

#[cfg(feature = "password-auth")]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct AccountPermission {
    pub code: String,
    pub effect: PermissionEffect,
    pub expires_at: Option<DateTime<Utc>>,
}

#[cfg(feature = "password-auth")]
#[derive(Debug, Clone, Serialize, TS)]
pub struct AccountSummary {
    pub id: Uuid,
    pub username: String,
    pub status: AccountStatus,
    /// Built-in role used for authentication policy such as JWT lifetime.
    pub primary_role: Role,
    /// Every active role assigned to the account, including custom roles.
    pub roles: Vec<String>,
    pub auth_version: i64,
    pub password_changed_at: DateTime<Utc>,
    pub last_authenticated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(feature = "password-auth")]
#[derive(Debug, Clone, Serialize, TS)]
pub struct RoleSummary {
    pub code: String,
    pub display_name: String,
    pub description: Option<String>,
    pub is_system: bool,
    pub is_active: bool,
    pub permissions: Vec<String>,
}

#[cfg(feature = "password-auth")]
#[derive(Debug, Clone, Serialize, TS)]
pub struct PermissionSummary {
    pub code: String,
    pub description: String,
}

#[cfg(feature = "password-auth")]
#[derive(Debug, Clone, Serialize, TS)]
pub struct AuthorizationCatalog {
    pub roles: Vec<RoleSummary>,
    pub permissions: Vec<PermissionSummary>,
}

#[cfg(feature = "password-auth")]
#[derive(Clone, Debug)]
pub struct UserAccount {
    pub id: Uuid,
    pub username: String,
    pub passphrase_key: String,
    /// Built-in role used for authentication policy such as JWT lifetime.
    pub role: Role,
    /// Every active role assigned to the account, including custom roles.
    pub roles: Vec<String>,
    pub active: bool,
    pub auth_version: i64,
    pub permissions: Vec<String>,
}
