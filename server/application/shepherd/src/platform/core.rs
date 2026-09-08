use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;
use tracing::{error, warn, info, debug, trace};
#[derive(Clone, Deserialize, Serialize, TS)]
pub struct TenantBootstrapOwner {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Clone, Deserialize, Serialize, TS)]
pub struct TenantBootstrapRequest {
    #[ts(type = "string")]
    pub tenant_id: Uuid,
    pub tenant_slug: String,
    pub tenant_display_name: String,
    #[ts(type = "string")]
    pub idempotency_key: Uuid,
    pub owners: Vec<TenantBootstrapOwner>,
}

impl TenantBootstrapRequest {
    pub fn normalize(&mut self) -> Result<(), String> {
        self.tenant_slug = self.tenant_slug.trim().to_owned();
        self.tenant_display_name = self.tenant_display_name.trim().to_owned();
        if self.tenant_id.is_nil()
            || self.idempotency_key.is_nil()
            || !(2..=63).contains(&self.tenant_slug.len())
            || self.tenant_slug.starts_with('-')
            || self.tenant_slug.ends_with('-')
            || !self
                .tenant_slug
                .bytes()
                .all(|b: u8| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || self.tenant_display_name.is_empty()
            || self.tenant_display_name.len() > 200
        {
            return Err("Tên doanh nghiệp hoặc mã doanh nghiệp không hợp lệ.".to_owned());
        }
        if self.owners.is_empty() || self.owners.len() > 10 {
            return Err("Cần từ 1 đến 10 chủ doanh nghiệp ban đầu.".to_owned());
        }
        let mut emails: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut usernames: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for owner in &mut self.owners {
            owner.username = owner.username.trim().to_owned();
            owner.email = owner.email.trim().to_lowercase();
            if !(3..=128).contains(&owner.username.len())
                || owner.username.chars().any(char::is_control)
                || owner.email.len() > 320
                || owner.email.chars().any(char::is_whitespace)
                || !owner
                    .email
                    .split_once('@')
                    .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.') && !domain.contains('@'))
                || !(8..=1024).contains(&owner.password.len())
                || !emails.insert(owner.email.clone())
                || !usernames.insert(owner.username.to_lowercase())
            {
                return Err(
                    "Thông tin chủ doanh nghiệp không hợp lệ hoặc bị trùng; mật khẩu cần ít nhất 8 ký tự.".to_owned(),
                );
            }
        }
        Ok(())
    }
}

#[derive(Serialize, TS)]
pub struct TenantBootstrapResult {
    #[ts(type = "string")]
    pub tenant_id: Uuid,
    pub tenant_slug: String,
    pub owner_count: usize,
    pub replayed: bool,
}

#[derive(Clone, Serialize, TS)]
pub struct PlatformProfile {
    pub username: String,
    pub email: String,
}

#[derive(Serialize, TS)]
pub struct PlatformSession {
    pub administrator: Option<PlatformProfile>,
}

#[derive(Clone, Copy, Deserialize, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum ServerLogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}
impl ServerLogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}
#[derive(Deserialize, TS)]
pub struct ServerLogLevelRequest {
    pub level: ServerLogLevel,
}
#[derive(Serialize, TS)]
pub struct ServerLogFilter {
    pub filter: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> TenantBootstrapRequest {
        TenantBootstrapRequest {
            tenant_id: Uuid::new_v4(),
            tenant_slug: "test-company".to_owned(),
            tenant_display_name: "Test company".to_owned(),
            idempotency_key: Uuid::new_v4(),
            owners: vec![TenantBootstrapOwner {
                username: "test_owner".to_owned(),
                email: "owner@example.test".to_owned(),
                password: "test-password-123".to_owned(),
            }],
        }
    }

    #[test]
    fn bootstrap_rejects_duplicate_owners_and_invalid_slug() {
        let mut duplicate: TenantBootstrapRequest = request();
        duplicate
            .owners
            .push(duplicate.owners.first().expect("fixture owner").clone());
        assert!(duplicate.normalize().is_err());
        let mut invalid: TenantBootstrapRequest = request();
        invalid.tenant_slug = "-unsafe-".to_owned();
        assert!(invalid.normalize().is_err());
        let mut empty: TenantBootstrapRequest = request();
        empty.owners.clear();
        assert!(empty.normalize().is_err());
    }

    #[test]
    fn bootstrap_normalizes_identity_without_trimming_password() {
        let mut input: TenantBootstrapRequest = request();
        let owner: &mut TenantBootstrapOwner = input.owners.first_mut().expect("fixture owner");
        owner.email = " Owner@Example.Test ".to_owned();
        owner.password = "  deliberate spaces  ".to_owned();
        input.normalize().expect("valid owner input");
        let owner: &TenantBootstrapOwner = input.owners.first().expect("fixture owner");
        assert_eq!(owner.email, "owner@example.test");
        assert_eq!(owner.password, "  deliberate spaces  ");
    }
}
