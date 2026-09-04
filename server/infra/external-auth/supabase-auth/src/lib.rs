//! Supabase Auth-specific external identity administration adapter.
//!
//! Reusable authentication infrastructure depends only on the
//! `ExtAuthAdmin` contract. All Supabase Auth URLs, payloads, metadata,
//! identifiers, and HTTP error interpretation stay in this adapter.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use infra_auth::ext_service::auth_admin::{
    CreateExternalIdentityRequest, ExternalIdentity, ExtAuthAdmin, ExtAdminErr, ExternalIdentityStatus,
};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use reqwest::Url;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use tracing::{debug, error, info, trace, warn};
use uuid::Uuid;

const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 5;
const CURRENT_MANAGED_BY: &str = "shepherd-supabase-auth-adapter";
const LEGACY_GOTRUE_MANAGED_BY: &str = "shepherd-gotrue-adapter";
const LEGACY_INFRA_MANAGED_BY: &str = "infra-auth";

#[derive(Clone)]
pub struct SupabaseAuthAdmin {
    client: reqwest::Client,
    base_url: String,
    admin_token_signer: AdminTokenSigner,
}

#[derive(Clone)]
struct AdminTokenSigner {
    encoding_key: EncodingKey,
    key_id: String,
    issuer: String,
    audience: String,
    role: String,
    expiry_secs: i64,
}

#[derive(Serialize)]
struct AdminTokenClaims<'a> {
    role: &'a str,
    iss: &'a str,
    aud: &'a str,
    iat: i64,
    exp: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigErr {
    #[error("AUTH_ADMIN_URL is required")]
    MissingUrl,
    #[error("{0} is required")]
    MissingSetting(&'static str),
    #[error("AUTH_ADMIN_URL is invalid: {0}")]
    InvalidUrl(String),
    #[error("AUTH_ADMIN_URL must be an absolute HTTP(S) URL")]
    UnsupportedUrl,
    #[error("AUTH_ADMIN_HTTP_TIMEOUT_SECS must be a positive integer")]
    InvalidTimeout,
    #[error("AUTH_ADMIN_JWT_ALGORITHM must be ES256")]
    InvalidAdminAlgorithm,
    #[error("AUTH_ADMIN_JWT_EXPIRY_SECS must be a positive integer no greater than 3600")]
    InvalidAdminTokenExpiry,
    #[error("AUTH_ADMIN_JWT_PRIVATE_KEY_BASE64 must contain base64-encoded PKCS#8 PEM")]
    InvalidAdminPrivateKeyEncoding(#[source] base64::DecodeError),
    #[error("AUTH_ADMIN_JWT_PRIVATE_KEY_BASE64 does not contain a valid ES256 PKCS#8 private key")]
    InvalidAdminPrivateKey(#[source] jsonwebtoken::errors::Error),
    #[error("failed to construct Supabase Auth administration HTTP client")]
    Client(#[source] reqwest::Error),
}

#[derive(Debug, thiserror::Error)]
enum SupabaseAuthError {
    #[error("failed to sign the Supabase Auth administration token")]
    AdminToken(#[source] jsonwebtoken::errors::Error),
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

impl SupabaseAuthAdmin {
    pub fn from_env() -> Result<Arc<Self>, ConfigErr> {
        debug!("Loading Supabase Auth identity administration configuration");
        let raw_url: String = required_env("AUTH_ADMIN_URL").ok_or(ConfigErr::MissingUrl)?;
        let parsed_url: Url = Url::parse(&raw_url)
            .map_err(|configuration_error: url::ParseError| ConfigErr::InvalidUrl(configuration_error.to_string()))?;
        if !matches!(parsed_url.scheme(), "http" | "https") || parsed_url.host_str().is_none() {
            return Err(ConfigErr::UnsupportedUrl);
        }
        let timeout_secs: u64 =
            std::env::var("AUTH_ADMIN_HTTP_TIMEOUT_SECS").map_or(Ok(DEFAULT_HTTP_TIMEOUT_SECS), |value: String| {
                value
                    .parse::<u64>()
                    .ok()
                    .filter(|parsed_value: &u64| *parsed_value > 0)
                    .ok_or(ConfigErr::InvalidTimeout)
            })?;
        let client: reqwest::Client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .map_err(ConfigErr::Client)?;
        let admin_token_signer: AdminTokenSigner = AdminTokenSigner::from_env()?;
        let service: Arc<Self> = Arc::new(Self {
            client,
            base_url: raw_url.trim().trim_end_matches('/').to_owned(),
            admin_token_signer,
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
        let admin_token: String = self.admin_token_signer.sign()?;
        let response: reqwest::Response = self
            .client
            .get(format!("{}/admin/users/{user_id}", self.base_url))
            .bearer_auth(admin_token)
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
        let admin_token: String = self.admin_token_signer.sign()?;
        let response: reqwest::Response = self
            .client
            .get(format!("{}/admin/users", self.base_url))
            .query(&[("filter", normalized_email)])
            .bearer_auth(admin_token)
            .send()
            .await
            .map_err(SupabaseAuthError::Transport)?;
        read_supabase_auth_response::<SupabaseAuthUserList>(response)
            .await
            .map(SupabaseAuthUserList::into_users)
    }
}

#[async_trait]
impl ExtAuthAdmin for SupabaseAuthAdmin {
    async fn get_identity(&self, subject: &str) -> Result<Option<ExternalIdentity>, ExtAdminErr> {
        self.get_user(subject)
            .await
            .map(|user: Option<SupabaseAuthUser>| user.map(ExternalIdentity::from))
            .map_err(map_supabase_auth_error)
    }

    async fn find_identity_by_email(&self, normalized_email: &str) -> Result<Option<ExternalIdentity>, ExtAdminErr> {
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
    ) -> Result<Option<ExternalIdentity>, ExtAdminErr> {
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

    async fn create_identity(&self, request: &CreateExternalIdentityRequest) -> Result<ExternalIdentity, ExtAdminErr> {
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
        let admin_token: String = self.admin_token_signer.sign().map_err(map_supabase_auth_error)?;
        let response: reqwest::Response = self
            .client
            .post(format!("{}/admin/users", self.base_url))
            .bearer_auth(admin_token)
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

impl AdminTokenSigner {
    fn from_env() -> Result<Self, ConfigErr> {
        let algorithm: String = required_admin_env("AUTH_ADMIN_JWT_ALGORITHM")?;
        if algorithm != "ES256" {
            return Err(ConfigErr::InvalidAdminAlgorithm);
        }
        let expiry_secs: i64 = required_admin_env("AUTH_ADMIN_JWT_EXPIRY_SECS")?
            .parse::<i64>()
            .ok()
            .filter(|value: &i64| (1..=3600).contains(value))
            .ok_or(ConfigErr::InvalidAdminTokenExpiry)?;
        let private_key_base64: String = required_admin_env("AUTH_ADMIN_JWT_PRIVATE_KEY_BASE64")?;
        let private_key_pem: Vec<u8> = STANDARD
            .decode(private_key_base64)
            .map_err(ConfigErr::InvalidAdminPrivateKeyEncoding)?;
        let encoding_key: EncodingKey =
            EncodingKey::from_ec_pem(&private_key_pem).map_err(ConfigErr::InvalidAdminPrivateKey)?;
        Ok(Self {
            encoding_key,
            key_id: required_admin_env("AUTH_ADMIN_JWT_KEY_ID")?,
            issuer: required_admin_env("AUTH_ADMIN_JWT_ISSUER")?,
            audience: required_admin_env("AUTH_ADMIN_JWT_AUDIENCE")?,
            role: required_admin_env("AUTH_ADMIN_JWT_ROLE")?,
            expiry_secs,
        })
    }

    fn sign(&self) -> Result<String, SupabaseAuthError> {
        self.sign_at(Utc::now().timestamp())
    }

    fn sign_at(&self, issued_at: i64) -> Result<String, SupabaseAuthError> {
        let mut header: Header = Header::new(Algorithm::ES256);
        header.kid = Some(self.key_id.clone());
        let claims = AdminTokenClaims {
            role: &self.role,
            iss: &self.issuer,
            aud: &self.audience,
            iat: issued_at,
            exp: issued_at.saturating_add(self.expiry_secs),
        };
        encode(&header, &claims, &self.encoding_key).map_err(SupabaseAuthError::AdminToken)
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

fn map_supabase_auth_error(error: SupabaseAuthError) -> ExtAdminErr {
    match error {
        SupabaseAuthError::Response {
            status: 400 | 422,
            message,
        } => ExtAdminErr::Validation(message),
        SupabaseAuthError::Response { status: 409, message } => ExtAdminErr::Conflict(message),
        SupabaseAuthError::Response { status: 404, message } => ExtAdminErr::NotFound(message),
        SupabaseAuthError::Transport(transport_error) => {
            error!(
                timeout = transport_error.is_timeout(),
                connect = transport_error.is_connect(),
                reason = %transport_error,
                "Supabase Auth administration transport failed"
            );
            ExtAdminErr::Unavailable("Supabase Auth transport failed".to_owned())
        }
        SupabaseAuthError::AdminToken(signing_error) => {
            error!(reason = %signing_error, "Supabase Auth administration token signing failed");
            ExtAdminErr::Unavailable("Supabase Auth administration credential is unavailable".to_owned())
        }
        SupabaseAuthError::InvalidResponse(response_error) => {
            error!(reason = %response_error, "Supabase Auth administration returned malformed JSON");
            ExtAdminErr::Unavailable("Supabase Auth returned malformed JSON".to_owned())
        }
        SupabaseAuthError::Response { status, message } => {
            warn!(status, "Supabase Auth administration request was rejected");
            ExtAdminErr::Unavailable(message)
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

fn required_admin_env(name: &'static str) -> Result<String, ConfigErr> {
    required_env(name).ok_or(ConfigErr::MissingSetting(name))
}

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use jsonwebtoken::{Algorithm, EncodingKey, decode_header};

    use super::{AdminTokenSigner, SupabaseAuthUser, provider_message, user_is_banned};
    use uuid::Uuid;

    const TEST_ES256_PRIVATE_KEY: &[u8] = br#"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgBTB80Tj8f1KY+uhC
VlQPYibNEmprjMZ7XkU7Imc906uhRANCAAT5mybr9SFvCNf8gNtL03QzgLwohOvY
goJNLXyZySwuRTAsDkwzkYc8/FBa6AfD99PAXvKZc99tqRuc9GSjNv89
-----END PRIVATE KEY-----
"#;

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

    #[test]
    fn signs_short_lived_es256_admin_token_with_configured_claims() {
        let encoding_key: EncodingKey = EncodingKey::from_ec_pem(TEST_ES256_PRIVATE_KEY)
            .unwrap_or_else(|error| panic!("test ES256 key must be valid: {error}"));
        let signer = AdminTokenSigner {
            encoding_key,
            key_id: "admin-test-key".to_owned(),
            issuer: "https://auth.example.com/auth/v1".to_owned(),
            audience: "authenticated".to_owned(),
            role: "service_role".to_owned(),
            expiry_secs: 600,
        };

        let token: String = signer
            .sign_at(1_700_000_000)
            .unwrap_or_else(|error| panic!("test token signing must succeed: {error}"));
        let header = decode_header(&token).unwrap_or_else(|error| panic!("test header must decode: {error}"));
        assert_eq!(header.alg, Algorithm::ES256);
        assert_eq!(header.kid.as_deref(), Some("admin-test-key"));

        let mut segments = token.split('.');
        let _header_segment: &str = segments.next().unwrap_or_default();
        let claims_segment: &str = segments.next().unwrap_or_default();
        let _signature_segment: &str = segments.next().unwrap_or_default();
        assert!(segments.next().is_none());
        let claims_bytes: Vec<u8> = URL_SAFE_NO_PAD
            .decode(claims_segment)
            .unwrap_or_else(|error| panic!("test claims must be base64url: {error}"));
        let claims: serde_json::Value =
            serde_json::from_slice(&claims_bytes).unwrap_or_else(|error| panic!("test claims must be JSON: {error}"));
        assert_eq!(
            claims.get("role").and_then(serde_json::Value::as_str),
            Some("service_role")
        );
        assert_eq!(
            claims.get("iss").and_then(serde_json::Value::as_str),
            Some("https://auth.example.com/auth/v1")
        );
        assert_eq!(
            claims.get("iat").and_then(serde_json::Value::as_i64),
            Some(1_700_000_000)
        );
        assert_eq!(
            claims.get("exp").and_then(serde_json::Value::as_i64),
            Some(1_700_000_600)
        );
    }
}
