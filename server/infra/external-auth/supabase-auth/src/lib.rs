//! Supabase Auth-specific external identity administration adapter.
//!
//! Reusable authentication infrastructure depends only on the
//! `ExternalIdentityAdmin` contract. All Supabase Auth URLs, payloads, metadata,
//! identifiers, and HTTP error interpretation stay in this adapter.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use infra_auth::ext_service::auth_admin::{
    CreateExternalIdentityRequest, ExternalIdentity, ExternalIdentityAdmin, ExternalIdentityAdminError,
    ExternalIdentityStatus,
};
use reqwest::Url;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::json;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 5;
const CURRENT_MANAGED_BY: &str = "shepherd-supabase-auth-adapter";
const LEGACY_GOTRUE_MANAGED_BY: &str = "shepherd-gotrue-adapter";
const LEGACY_INFRA_MANAGED_BY: &str = "infra-auth";

#[derive(Clone)]
pub struct SupabaseAuthIdentityAdmin {
    client: reqwest::Client,
    base_url: String,
    admin_token: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SupabaseAuthIdentityAdminConfigError {
    #[error("AUTH_ADMIN_URL is required")]
    MissingUrl,
    #[error("AUTH_ADMIN_TOKEN is required")]
    MissingToken,
    #[error("AUTH_ADMIN_URL is invalid: {0}")]
    InvalidUrl(String),
    #[error("AUTH_ADMIN_URL must be an absolute HTTP(S) URL")]
    UnsupportedUrl,
    #[error("AUTH_ADMIN_HTTP_TIMEOUT_SECS must be a positive integer")]
    InvalidTimeout,
    #[error("failed to construct Supabase Auth administration HTTP client")]
    Client(#[source] reqwest::Error),
}

#[derive(Debug, thiserror::Error)]
enum SupabaseAuthError {
    #[error("Supabase Auth request failed")]
    Transport(#[source] reqwest::Error),
    #[error("Supabase Auth returned HTTP {status}: {message}")]
    Response { status: u16, message: String },
    #[error("Supabase Auth returned malformed JSON")]
    InvalidResponse(#[source] reqwest::Error),
}

#[derive(Clone, Debug, Deserialize)]
struct SupabaseAuthUser {
    id: Uuid,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_confirmed_at: Option<String>,
    #[serde(default)]
    last_sign_in_at: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    banned_until: Option<String>,
    #[serde(default)]
    app_metadata: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SupabaseAuthUserList {
    Envelope { users: Vec<SupabaseAuthUser> },
    Direct(Vec<SupabaseAuthUser>),
}

impl SupabaseAuthUserList {
    fn into_users(self) -> Vec<SupabaseAuthUser> {
        match self {
            Self::Envelope { users } | Self::Direct(users) => users,
        }
    }
}

impl SupabaseAuthIdentityAdmin {
    pub fn from_env() -> Result<Arc<Self>, SupabaseAuthIdentityAdminConfigError> {
        debug!("Loading Supabase Auth identity administration configuration");
        let raw_url: String = required_env("AUTH_ADMIN_URL").ok_or(SupabaseAuthIdentityAdminConfigError::MissingUrl)?;
        let parsed_url: Url = Url::parse(&raw_url).map_err(|configuration_error: url::ParseError| {
            SupabaseAuthIdentityAdminConfigError::InvalidUrl(configuration_error.to_string())
        })?;
        if !matches!(parsed_url.scheme(), "http" | "https") || parsed_url.host_str().is_none() {
            return Err(SupabaseAuthIdentityAdminConfigError::UnsupportedUrl);
        }
        let timeout_secs: u64 =
            std::env::var("AUTH_ADMIN_HTTP_TIMEOUT_SECS").map_or(Ok(DEFAULT_HTTP_TIMEOUT_SECS), |value: String| {
                value
                    .parse::<u64>()
                    .ok()
                    .filter(|parsed_value: &u64| *parsed_value > 0)
                    .ok_or(SupabaseAuthIdentityAdminConfigError::InvalidTimeout)
            })?;
        let client: reqwest::Client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .map_err(SupabaseAuthIdentityAdminConfigError::Client)?;
        let service: Arc<Self> = Arc::new(Self {
            client,
            base_url: raw_url.trim().trim_end_matches('/').to_owned(),
            admin_token: required_env("AUTH_ADMIN_TOKEN").ok_or(SupabaseAuthIdentityAdminConfigError::MissingToken)?,
        });
        info!(
            timeout_secs,
            "Supabase Auth identity administration adapter initialized"
        );
        Ok(service)
    }

    async fn get_user(&self, subject: &str) -> Result<Option<SupabaseAuthUser>, SupabaseAuthError> {
        let user_id: Uuid =
            Uuid::parse_str(subject).map_err(|_parse_error: uuid::Error| SupabaseAuthError::Response {
                status: 422,
                message: "The Supabase Auth user subject is invalid.".to_owned(),
            })?;
        trace!(auth_user_id = %user_id, "Supabase Auth user lookup accepted");
        let response: reqwest::Response = self
            .client
            .get(format!("{}/admin/users/{user_id}", self.base_url))
            .bearer_auth(&self.admin_token)
            .send()
            .await
            .map_err(SupabaseAuthError::Transport)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            debug!(auth_user_id = %user_id, "Supabase Auth user was not found");
            return Ok(None);
        }
        let user: SupabaseAuthUser = read_supabase_auth_response(response).await?;
        debug!(auth_user_id = %user_id, "Supabase Auth user loaded");
        Ok(Some(user))
    }

    async fn find_users(&self, normalized_email: &str) -> Result<Vec<SupabaseAuthUser>, SupabaseAuthError> {
        trace!("Searching Supabase Auth users by normalized email");
        let response: reqwest::Response = self
            .client
            .get(format!("{}/admin/users", self.base_url))
            .query(&[("filter", normalized_email)])
            .bearer_auth(&self.admin_token)
            .send()
            .await
            .map_err(SupabaseAuthError::Transport)?;
        read_supabase_auth_response::<SupabaseAuthUserList>(response)
            .await
            .map(SupabaseAuthUserList::into_users)
    }
}

#[async_trait]
impl ExternalIdentityAdmin for SupabaseAuthIdentityAdmin {
    async fn get_identity(&self, subject: &str) -> Result<Option<ExternalIdentity>, ExternalIdentityAdminError> {
        self.get_user(subject)
            .await
            .map(|user: Option<SupabaseAuthUser>| user.map(ExternalIdentity::from))
            .map_err(map_supabase_auth_error)
    }

    async fn find_identity_by_email(
        &self,
        normalized_email: &str,
    ) -> Result<Option<ExternalIdentity>, ExternalIdentityAdminError> {
        let identity: Option<ExternalIdentity> = self
            .find_users(normalized_email)
            .await
            .map_err(map_supabase_auth_error)?
            .into_iter()
            .find(|user: &SupabaseAuthUser| {
                user.email
                    .as_deref()
                    .is_some_and(|candidate: &str| candidate.eq_ignore_ascii_case(normalized_email))
            })
            .map(ExternalIdentity::from);
        debug!(
            found = identity.is_some(),
            "Supabase Auth normalized-email identity search completed"
        );
        Ok(identity)
    }

    async fn find_provisioned_identity(
        &self,
        normalized_email: &str,
        tenant_id: Uuid,
        idempotency_key: Uuid,
    ) -> Result<Option<ExternalIdentity>, ExternalIdentityAdminError> {
        let tenant_id_text: String = tenant_id.to_string();
        let idempotency_key_text: String = idempotency_key.to_string();
        let identity: Option<ExternalIdentity> = self
            .find_users(normalized_email)
            .await
            .map_err(map_supabase_auth_error)?
            .into_iter()
            .find(|user: &SupabaseAuthUser| {
                let managed_by: Option<&str> = user.app_metadata.get("managed_by").and_then(serde_json::Value::as_str);
                user.email
                    .as_deref()
                    .is_some_and(|candidate: &str| candidate.eq_ignore_ascii_case(normalized_email))
                    && matches!(
                        managed_by,
                        Some(CURRENT_MANAGED_BY | LEGACY_GOTRUE_MANAGED_BY | LEGACY_INFRA_MANAGED_BY)
                    )
                    && user.app_metadata.get("tenant_id").and_then(serde_json::Value::as_str)
                        == Some(tenant_id_text.as_str())
                    && user
                        .app_metadata
                        .get("provisioning_key")
                        .and_then(serde_json::Value::as_str)
                        == Some(idempotency_key_text.as_str())
            })
            .map(ExternalIdentity::from);
        debug!(
            tenant_id = %tenant_id,
            idempotency_key = %idempotency_key,
            recovered = identity.is_some(),
            "Recoverable Supabase Auth identity search completed"
        );
        Ok(identity)
    }

    async fn create_identity(
        &self,
        request: &CreateExternalIdentityRequest,
    ) -> Result<ExternalIdentity, ExternalIdentityAdminError> {
        trace!(
            tenant_id = %request.tenant_id,
            idempotency_key = %request.idempotency_key,
            password_supplied = request.password.is_some(),
            "Supabase Auth user creation accepted"
        );
        let mut attributes: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        attributes.insert("email".to_owned(), json!(request.email));
        attributes.insert("email_confirm".to_owned(), json!(true));
        attributes.insert("role".to_owned(), json!("authenticated"));
        attributes.insert("user_metadata".to_owned(), json!({ "username": request.username }));
        attributes.insert(
            "app_metadata".to_owned(),
            json!({
                "managed_by": CURRENT_MANAGED_BY,
                "tenant_id": request.tenant_id,
                "provisioning_key": request.idempotency_key,
            }),
        );
        if let Some(password) = request.password.as_ref() {
            attributes.insert("password".to_owned(), json!(password));
        }
        let response: reqwest::Response = self
            .client
            .post(format!("{}/admin/users", self.base_url))
            .bearer_auth(&self.admin_token)
            .json(&attributes)
            .send()
            .await
            .map_err(SupabaseAuthError::Transport)
            .map_err(map_supabase_auth_error)?;
        let user: SupabaseAuthUser = read_supabase_auth_response(response)
            .await
            .map_err(map_supabase_auth_error)?;
        info!(auth_user_id = %user.id, "Supabase Auth user created");
        Ok(user.into())
    }
}

impl From<SupabaseAuthUser> for ExternalIdentity {
    fn from(user: SupabaseAuthUser) -> Self {
        let status: ExternalIdentityStatus = if user_is_banned(&user) {
            ExternalIdentityStatus::Disabled
        } else {
            ExternalIdentityStatus::Active
        };
        Self {
            subject: user.id.to_string(),
            email: user.email,
            status,
            email_confirmed: user.email_confirmed_at.is_some(),
            created_at: user.created_at,
            last_sign_in_at: user.last_sign_in_at,
        }
    }
}

fn user_is_banned(user: &SupabaseAuthUser) -> bool {
    user.banned_until
        .as_deref()
        .and_then(|value: &str| DateTime::parse_from_rfc3339(value).ok())
        .is_some_and(|until: DateTime<chrono::FixedOffset>| until.with_timezone(&Utc) > Utc::now())
}

fn map_supabase_auth_error(error: SupabaseAuthError) -> ExternalIdentityAdminError {
    match error {
        SupabaseAuthError::Response {
            status: 400 | 422,
            message,
        } => ExternalIdentityAdminError::Validation(message),
        SupabaseAuthError::Response { status: 409, message } => ExternalIdentityAdminError::Conflict(message),
        SupabaseAuthError::Response { status: 404, message } => ExternalIdentityAdminError::NotFound(message),
        SupabaseAuthError::Transport(transport_error) => {
            error!(
                timeout = transport_error.is_timeout(),
                connect = transport_error.is_connect(),
                reason = %transport_error,
                "Supabase Auth administration transport failed"
            );
            ExternalIdentityAdminError::Unavailable("Supabase Auth transport failed".to_owned())
        }
        SupabaseAuthError::InvalidResponse(response_error) => {
            error!(reason = %response_error, "Supabase Auth administration returned malformed JSON");
            ExternalIdentityAdminError::Unavailable("Supabase Auth returned malformed JSON".to_owned())
        }
        SupabaseAuthError::Response { status, message } => {
            warn!(status, "Supabase Auth administration request was rejected");
            ExternalIdentityAdminError::Unavailable(message)
        }
    }
}

async fn read_supabase_auth_response<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, SupabaseAuthError> {
    let status: reqwest::StatusCode = response.status();
    if status.is_success() {
        return response.json::<T>().await.map_err(SupabaseAuthError::InvalidResponse);
    }
    let message: String = response
        .text()
        .await
        .map(|body: String| provider_message(&body))
        .unwrap_or_else(|_read_error: reqwest::Error| "Supabase Auth rejected the request".to_owned());
    Err(SupabaseAuthError::Response {
        status: status.as_u16(),
        message,
    })
}

fn provider_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value: serde_json::Value| {
            value
                .get("msg")
                .or_else(|| value.get("message"))
                .or_else(|| value.get("error_description"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "Supabase Auth rejected the request".to_owned())
        .chars()
        .take(300)
        .collect()
}

fn required_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value: String| value.trim().to_owned())
        .filter(|value: &String| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{SupabaseAuthUser, provider_message, user_is_banned};
    use uuid::Uuid;

    #[test]
    fn detects_future_ban_and_sanitizes_provider_message() {
        let user: SupabaseAuthUser = SupabaseAuthUser {
            id: Uuid::nil(),
            email: None,
            email_confirmed_at: None,
            last_sign_in_at: None,
            created_at: Some("2026-01-01T00:00:00Z".to_owned()),
            banned_until: Some("2099-01-01T00:00:00Z".to_owned()),
            app_metadata: serde_json::Value::Null,
        };
        assert!(user_is_banned(&user));
        assert_eq!(
            provider_message(r#"{"msg":"Email already exists"}"#),
            "Email already exists"
        );
    }
}
