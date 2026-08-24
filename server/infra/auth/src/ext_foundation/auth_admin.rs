use std::{ops::Deref, sync::Arc, time::Duration};

use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, put},
};
use chrono::{DateTime, Utc};
use infra_postgres::{TenantDbErr, TenantTransaction};
use reqwest::Url;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, postgres::PgQueryResult};
use tracing::{debug, error, info, trace, warn};
use ts_rs::TS;
use uuid::Uuid;

use crate::{
    AuthCodeError, AuthService, PermissionCode, RoleCode,
    ext_foundation::account::{AccountStatus, AuthenticatedUser},
};

const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 5;
const DISABLED_DURATION: &str = "876000h";
const IDEMPOTENCY_HEADER: &str = "idempotency-key";

/// Application-owned permission codes required by the reusable account routes.
#[derive(Clone, Debug)]
pub struct AuthAdminPolicy {
    pub read_permission: PermissionCode,
    pub create_permission: PermissionCode,
    pub update_permission: PermissionCode,
    pub disable_permission: PermissionCode,
    pub role_read_permission: PermissionCode,
    pub role_manage_permission: PermissionCode,
    pub branch_manage_permission: PermissionCode,
}

impl AuthAdminPolicy {
    pub fn try_new(
        read_permission: impl Into<String>,
        create_permission: impl Into<String>,
        update_permission: impl Into<String>,
        disable_permission: impl Into<String>,
        role_read_permission: impl Into<String>,
        role_manage_permission: impl Into<String>,
        branch_manage_permission: impl Into<String>,
    ) -> Result<Self, AuthCodeError> {
        Ok(Self {
            read_permission: PermissionCode::parse(read_permission)?,
            create_permission: PermissionCode::parse(create_permission)?,
            update_permission: PermissionCode::parse(update_permission)?,
            disable_permission: PermissionCode::parse(disable_permission)?,
            role_read_permission: PermissionCode::parse(role_read_permission)?,
            role_manage_permission: PermissionCode::parse(role_manage_permission)?,
            branch_manage_permission: PermissionCode::parse(branch_manage_permission)?,
        })
    }
}

struct AuthAdminContext {
    auth: Arc<AuthService>,
    policy: AuthAdminPolicy,
    provisioner: Arc<dyn AuthAccountProvisioner>,
}

impl Deref for AuthAdminContext {
    type Target = AuthService;

    fn deref(&self) -> &Self::Target {
        &self.auth
    }
}

#[derive(Clone)]
pub struct AuthAdminService {
    client: reqwest::Client,
    base_url: String,
    admin_token: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthAdminConfigError {
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
    #[error("failed to construct Auth administration HTTP client")]
    Client(#[source] reqwest::Error),
}

#[derive(Debug, thiserror::Error)]
enum ProviderError {
    #[error("Auth provider request failed")]
    Transport(#[source] reqwest::Error),
    #[error("Auth provider returned HTTP {status}: {message}")]
    Response { status: u16, message: String },
    #[error("Auth provider returned malformed JSON")]
    InvalidResponse(#[source] reqwest::Error),
}

#[derive(Debug)]
enum AdminApiError {
    Forbidden,
    Validation(String),
    Conflict(String),
    NotFound(String),
    ProviderUnavailable,
    Internal,
}

#[derive(Serialize)]
struct AdminErrorBody {
    code: &'static str,
    message: String,
}

impl IntoResponse for AdminApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "This account cannot administer users.".to_owned(),
            ),
            Self::Validation(message) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_failed", message),
            Self::Conflict(message) => (StatusCode::CONFLICT, "conflict", message),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, "not_found", message),
            Self::ProviderUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "auth_provider_unavailable",
                "The identity provider is temporarily unavailable.".to_owned(),
            ),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "The user administration operation could not be completed.".to_owned(),
            ),
        };
        (status, Json(AdminErrorBody { code, message })).into_response()
    }
}

#[derive(Clone, Debug, Deserialize)]
struct ExtProviderUser {
    id: Uuid,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_confirmed_at: Option<String>,
    #[serde(default)]
    last_sign_in_at: Option<String>,
    created_at: String,
    #[serde(default)]
    banned_until: Option<String>,
    #[serde(default)]
    app_metadata: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ExtProviderUserList {
    Envelope { users: Vec<ExtProviderUser> },
    Direct(Vec<ExtProviderUser>),
}

impl ExtProviderUserList {
    fn into_users(self) -> Vec<ExtProviderUser> {
        match self {
            Self::Envelope { users } | Self::Direct(users) => users,
        }
    }
}

#[derive(Clone, Debug)]
struct MappedAccount {
    subject: String,
    account_id: Uuid,
    username: String,
    email: Option<String>,
    account_status: AccountStatus,
    primary_role: RoleCode,
    branch_ids: Vec<Uuid>,
}

#[derive(Debug)]
struct MappedAccountRow {
    subject: String,
    account_id: Uuid,
    username: String,
    email: Option<String>,
    account_status: String,
    primary_role: String,
    branch_ids: Vec<Uuid>,
}

impl TryFrom<MappedAccountRow> for MappedAccount {
    type Error = AdminApiError;

    fn try_from(row: MappedAccountRow) -> Result<Self, Self::Error> {
        let account_status: AccountStatus = match row.account_status.as_str() {
            "active" => AccountStatus::Active,
            "disabled" => AccountStatus::Disabled,
            unsupported_status => {
                error!(
                    account_id = %row.account_id,
                    account_status = unsupported_status,
                    "Mapped Auth account has an unsupported application status"
                );
                return Err(AdminApiError::Internal);
            }
        };
        let primary_role: RoleCode = RoleCode::try_from(row.primary_role).map_err(|code_error| {
            error!(
                account_id = %row.account_id,
                reason = %code_error,
                "Mapped Auth account has an invalid primary role code"
            );
            AdminApiError::Internal
        })?;
        Ok(Self {
            subject: row.subject,
            account_id: row.account_id,
            username: row.username,
            email: row.email,
            account_status,
            primary_role,
            branch_ids: row.branch_ids,
        })
    }
}

struct ExistsRow {
    exists: bool,
}

struct BranchAssignmentRuleRow {
    min_assignments: i16,
    max_assignments: Option<i16>,
    valid_branch_count: i64,
}

#[derive(Clone, Debug)]
pub struct AuthAccountProvisioningContext {
    pub tenant_id: Uuid,
    pub actor_account_id: Uuid,
    pub account_id: Uuid,
    pub username: String,
    pub email: String,
    pub primary_role: RoleCode,
    pub branch_ids: Vec<Uuid>,
}

#[derive(Debug, thiserror::Error)]
#[error("application account provisioning failed: {code}")]
pub struct AuthAccountProvisioningError {
    code: &'static str,
}

impl AuthAccountProvisioningError {
    pub const fn new(code: &'static str) -> Self {
        Self { code }
    }

    const fn code(&self) -> &'static str {
        self.code
    }
}

#[async_trait]
pub trait AuthAccountProvisioner: Send + Sync {
    async fn provision(
        &self,
        connection: &mut PgConnection,
        context: &AuthAccountProvisioningContext,
    ) -> Result<(), AuthAccountProvisioningError>;
}

#[derive(Debug)]
struct NoopAuthAccountProvisioner;

#[async_trait]
impl AuthAccountProvisioner for NoopAuthAccountProvisioner {
    async fn provision(
        &self,
        _connection: &mut PgConnection,
        _context: &AuthAccountProvisioningContext,
    ) -> Result<(), AuthAccountProvisioningError> {
        Ok(())
    }
}

#[derive(Debug)]
struct ProvisioningStateRow {
    request_fingerprint: String,
    status: String,
    auth_user_id: Option<Uuid>,
    account_id: Option<Uuid>,
    retry_allowed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProvisioningRequestStatus {
    Processing,
    Completed,
    Failed,
}

impl ProvisioningRequestStatus {
    fn from_code(code: &str) -> Option<Self> {
        match code {
            "processing" => Some(Self::Processing),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug)]
enum ProvisioningClaim {
    Proceed { auth_user_id: Option<Uuid> },
    Replay { auth_user_id: Uuid, account_id: Uuid },
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct AuthUserSummary {
    pub auth_user_id: String,
    #[ts(type = "string")]
    pub account_id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub primary_role: RoleCode,
    #[ts(type = "Array<string>")]
    pub branch_ids: Vec<Uuid>,
    pub account_status: AccountStatus,
    pub provider_status: AuthProviderUserStatus,
    pub email_confirmed: bool,
    pub created_at: Option<String>,
    pub last_sign_in_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateAuthUserRequest {
    pub username: String,
    pub email: String,
    pub password: Option<String>,
    pub primary_role: RoleCode,
    #[ts(type = "Array<string>")]
    pub branch_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export)]
pub struct SetAuthUserStatusRequest {
    pub disabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AuthProviderUserStatus {
    Active,
    Disabled,
    Missing,
}

impl AuthAdminService {
    pub fn from_env() -> Result<Arc<Self>, AuthAdminConfigError> {
        debug!("Loading Auth administration provider configuration");
        let raw_url: String = required_env("AUTH_ADMIN_URL").ok_or(AuthAdminConfigError::MissingUrl)?;
        let parsed_url: Url =
            Url::parse(&raw_url).map_err(|error| AuthAdminConfigError::InvalidUrl(error.to_string()))?;
        if !matches!(parsed_url.scheme(), "http" | "https") || parsed_url.host_str().is_none() {
            return Err(AuthAdminConfigError::UnsupportedUrl);
        }
        let timeout_secs: u64 =
            std::env::var("AUTH_ADMIN_HTTP_TIMEOUT_SECS").map_or(Ok(DEFAULT_HTTP_TIMEOUT_SECS), |value| {
                value
                    .parse::<u64>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or(AuthAdminConfigError::InvalidTimeout)
            })?;
        let client: reqwest::Client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .map_err(AuthAdminConfigError::Client)?;

        let service: Arc<Self> = Arc::new(Self {
            client,
            base_url: raw_url.trim().trim_end_matches('/').to_owned(),
            admin_token: required_env("AUTH_ADMIN_TOKEN").ok_or(AuthAdminConfigError::MissingToken)?,
        });
        info!(timeout_secs, "Auth administration provider initialized");
        Ok(service)
    }

    async fn get_user(&self, user_id: Uuid) -> Result<Option<ExtProviderUser>, ProviderError> {
        trace!(auth_user_id = %user_id, "Auth provider user lookup accepted");
        let response: reqwest::Response = self
            .client
            .get(format!("{}/admin/users/{user_id}", self.base_url))
            .bearer_auth(&self.admin_token)
            .send()
            .await
            .map_err(ProviderError::Transport)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            debug!(auth_user_id = %user_id, "Auth provider user was not found");
            return Ok(None);
        }
        let user: ExtProviderUser = read_provider_response(response).await?;
        debug!(auth_user_id = %user_id, "Auth provider user loaded");
        Ok(Some(user))
    }

    async fn create_user(
        &self,
        request: &CreateAuthUserRequest,
        tenant_id: Uuid,
        idempotency_key: Uuid,
    ) -> Result<ExtProviderUser, ProviderError> {
        trace!(
            primary_role = %request.primary_role,
            password_supplied = request.password.is_some(),
            "Auth provider user creation accepted"
        );
        let mut attributes: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        attributes.insert("email".to_owned(), json!(request.email));
        attributes.insert("email_confirm".to_owned(), json!(true));
        attributes.insert("role".to_owned(), json!("authenticated"));
        attributes.insert("user_metadata".to_owned(), json!({ "username": request.username }));
        attributes.insert(
            "app_metadata".to_owned(),
            json!({
                "managed_by": "infra-auth",
                "tenant_id": tenant_id,
                "provisioning_key": idempotency_key,
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
            .map_err(ProviderError::Transport)?;
        let user: ExtProviderUser = read_provider_response(response).await?;
        info!(auth_user_id = %user.id, "Auth provider user created");
        Ok(user)
    }

    async fn find_provisioned_user(
        &self,
        email: &str,
        tenant_id: Uuid,
        idempotency_key: Uuid,
    ) -> Result<Option<ExtProviderUser>, ProviderError> {
        trace!(tenant_id = %tenant_id, idempotency_key = %idempotency_key, "Searching for recoverable Auth user");
        let response: reqwest::Response = self
            .client
            .get(format!("{}/admin/users?page=1&per_page=1000", self.base_url))
            .bearer_auth(&self.admin_token)
            .send()
            .await
            .map_err(ProviderError::Transport)?;
        let users: Vec<ExtProviderUser> = read_provider_response::<ExtProviderUserList>(response)
            .await?
            .into_users();
        let tenant_id_text: String = tenant_id.to_string();
        let idempotency_key_text: String = idempotency_key.to_string();
        let user: Option<ExtProviderUser> = users.into_iter().find(|user: &ExtProviderUser| {
            user.email
                .as_deref()
                .is_some_and(|candidate: &str| candidate.eq_ignore_ascii_case(email))
                && user.app_metadata.get("managed_by").and_then(serde_json::Value::as_str) == Some("infra-auth")
                && user.app_metadata.get("tenant_id").and_then(serde_json::Value::as_str)
                    == Some(tenant_id_text.as_str())
                && user
                    .app_metadata
                    .get("provisioning_key")
                    .and_then(serde_json::Value::as_str)
                    == Some(idempotency_key_text.as_str())
        });
        debug!(
            tenant_id = %tenant_id,
            idempotency_key = %idempotency_key,
            recovered = user.is_some(),
            "Recoverable Auth user search completed"
        );
        Ok(user)
    }

    async fn set_disabled(&self, user_id: Uuid, disabled: bool) -> Result<ExtProviderUser, ProviderError> {
        trace!(auth_user_id = %user_id, disabled, "Auth provider status change accepted");
        let response: reqwest::Response = self
            .client
            .put(format!("{}/admin/users/{user_id}", self.base_url))
            .bearer_auth(&self.admin_token)
            .json(&json!({
                "ban_duration": if disabled { DISABLED_DURATION } else { "none" }
            }))
            .send()
            .await
            .map_err(ProviderError::Transport)?;
        let user: ExtProviderUser = read_provider_response(response).await?;
        info!(auth_user_id = %user_id, disabled, "Auth provider status changed");
        Ok(user)
    }

    async fn delete_user_after_failed_link(&self, user_id: Uuid) -> Result<(), ProviderError> {
        let response: reqwest::Response = self
            .client
            .delete(format!("{}/admin/users/{user_id}", self.base_url))
            .bearer_auth(&self.admin_token)
            .send()
            .await
            .map_err(ProviderError::Transport)?;
        let status: reqwest::StatusCode = response.status();
        if status.is_success() || status == reqwest::StatusCode::NOT_FOUND {
            info!(auth_user_id = %user_id, status = status.as_u16(), "Compensated unlinked Auth user");
            Ok(())
        } else {
            let message: String = response
                .text()
                .await
                .map(|body: String| -> String { provider_message(&body) })
                .unwrap_or_else(|_error: reqwest::Error| -> String {
                    "Auth provider rejected compensation deletion".to_owned()
                });
            Err(ProviderError::Response {
                status: status.as_u16(),
                message,
            })
        }
    }
}

pub fn routes(auth: Arc<AuthService>, policy: AuthAdminPolicy) -> Router {
    routes_with_provisioner(auth, policy, Arc::new(NoopAuthAccountProvisioner))
}

pub fn routes_with_provisioner(
    auth: Arc<AuthService>,
    policy: AuthAdminPolicy,
    provisioner: Arc<dyn AuthAccountProvisioner>,
) -> Router {
    debug!(
        read_permission = %policy.read_permission,
        create_permission = %policy.create_permission,
        update_permission = %policy.update_permission,
        disable_permission = %policy.disable_permission,
        role_read_permission = %policy.role_read_permission,
        role_manage_permission = %policy.role_manage_permission,
        branch_manage_permission = %policy.branch_manage_permission,
        "Registering Auth administration routes"
    );
    let state: Arc<AuthAdminContext> = Arc::new(AuthAdminContext {
        auth,
        policy,
        provisioner,
    });
    Router::new()
        .route("/admin/auth-users", get(list_users).post(create_user))
        .route("/admin/auth-users/{auth_user_id}/status", put(set_user_status))
        .with_state(state)
}

async fn list_users(
    State(context): State<Arc<AuthAdminContext>>,
    Extension(actor): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<AuthUserSummary>>, AdminApiError> {
    info!(tenant_id = %actor.tenant_id, actor_id = %actor.account_id, "Auth user list request accepted");
    require_permission(&actor, &context.policy.read_permission)?;
    let accounts: Vec<MappedAccount> = load_mapped_accounts(&context, &actor).await?;
    let mut users: Vec<AuthUserSummary> = Vec::with_capacity(accounts.len());
    for account in accounts {
        let provider_user: Option<ExtProviderUser> = match Uuid::parse_str(&account.subject) {
            Ok(user_id) => context
                .admin
                .get_user(user_id)
                .await
                .map_err(|error| provider_failure("load Auth user", &actor, error))?,
            Err(error) => {
                error!(
                    "Mapped Auth subject is not a UUID: tenant_id={} account_id={} error={}",
                    actor.tenant_id, account.account_id, error
                );
                None
            }
        };
        users.push(summary(account, provider_user));
    }
    info!(
        tenant_id = %actor.tenant_id,
        actor_id = %actor.account_id,
        user_count = users.len(),
        "Auth user list request completed"
    );
    Ok(Json(users))
}

async fn create_user(
    State(context): State<Arc<AuthAdminContext>>,
    Extension(actor): Extension<AuthenticatedUser>,
    headers: HeaderMap,
    Json(mut request): Json<CreateAuthUserRequest>,
) -> Result<(StatusCode, Json<AuthUserSummary>), AdminApiError> {
    let idempotency_key: Uuid = parse_idempotency_key(&headers)?;
    info!(
        tenant_id = %actor.tenant_id,
        actor_id = %actor.account_id,
        idempotency_key = %idempotency_key,
        primary_role = %request.primary_role,
        "Auth user creation request accepted"
    );
    require_permission(&actor, &context.policy.create_permission)?;
    normalize_create_request(&mut request)?;
    ensure_role_grantable(&context, &actor, &request.primary_role).await?;
    ensure_branch_assignments_valid(&context, &actor, &request.primary_role, &request.branch_ids).await?;
    let request_fingerprint: String = provisioning_fingerprint(&request);
    let claim: ProvisioningClaim =
        claim_provisioning_request(&context, &actor, idempotency_key, &request_fingerprint).await?;
    if let ProvisioningClaim::Replay {
        auth_user_id,
        account_id,
    } = claim
    {
        let account: MappedAccount = load_mapped_account_by_id(&context, &actor, account_id).await?;
        let provider_user: ExtProviderUser = context
            .admin
            .get_user(auth_user_id)
            .await
            .map_err(|error: ProviderError| provider_failure("replay Auth user creation", &actor, error))?
            .ok_or_else(|| {
                error!(
                    tenant_id = %actor.tenant_id,
                    account_id = %account_id,
                    auth_user_id = %auth_user_id,
                    idempotency_key = %idempotency_key,
                    "Completed Auth provisioning references a missing provider user"
                );
                AdminApiError::Internal
            })?;
        info!(
            tenant_id = %actor.tenant_id,
            actor_id = %actor.account_id,
            account_id = %account_id,
            auth_user_id = %auth_user_id,
            idempotency_key = %idempotency_key,
            "Auth user creation replay returned the original result"
        );
        return Ok((StatusCode::OK, Json(summary(account, Some(provider_user)))));
    }

    let ProvisioningClaim::Proceed { auth_user_id } = claim else {
        return Err(AdminApiError::Internal);
    };
    if let Err(error) = ensure_username_available(&context, &actor, &request.username).await {
        mark_provisioning_failed(&context, &actor, idempotency_key, "username_unavailable", None).await;
        return Err(error);
    }

    let provider_result: Result<ExtProviderUser, ProviderError> =
        resolve_or_create_provider_user(&context, &request, actor.tenant_id, idempotency_key, auth_user_id).await;
    let provider_user: ExtProviderUser = match provider_result {
        Ok(provider_user) => provider_user,
        Err(error) => {
            mark_provisioning_failed(&context, &actor, idempotency_key, "provider_create_failed", None).await;
            return Err(provider_failure("create Auth user", &actor, error));
        }
    };
    if let Err(error) = record_provisioned_auth_user(&context, &actor, idempotency_key, provider_user.id).await {
        compensate_created_provider_user(
            &context,
            &actor,
            idempotency_key,
            provider_user.id,
            "provider_record_failed",
        )
        .await;
        return Err(error);
    }

    let account: MappedAccount = match link_created_user(
        &context,
        &actor,
        &request,
        provider_user.id,
        idempotency_key,
        &request_fingerprint,
    )
    .await
    {
        Ok(account) => account,
        Err(error) => {
            compensate_created_provider_user(
                &context,
                &actor,
                idempotency_key,
                provider_user.id,
                "account_link_failed",
            )
            .await;
            record_audit(
                "auth.user.create",
                "failed",
                Some(actor.tenant_id),
                Some(actor.account_id),
            );
            return Err(error);
        }
    };

    record_audit(
        "auth.user.create",
        "accepted",
        Some(actor.tenant_id),
        Some(actor.account_id),
    );
    info!(
        tenant_id = %actor.tenant_id,
        actor_id = %actor.account_id,
        account_id = %account.account_id,
        auth_user_id = %provider_user.id,
        idempotency_key = %idempotency_key,
        "Auth user creation request completed"
    );
    Ok((StatusCode::CREATED, Json(summary(account, Some(provider_user)))))
}

async fn set_user_status(
    State(context): State<Arc<AuthAdminContext>>,
    Extension(actor): Extension<AuthenticatedUser>,
    Path(auth_user_id): Path<String>,
    Json(request): Json<SetAuthUserStatusRequest>,
) -> Result<Json<AuthUserSummary>, AdminApiError> {
    info!(
        tenant_id = %actor.tenant_id,
        actor_id = %actor.account_id,
        disabled = request.disabled,
        "Auth user status request accepted"
    );
    require_permission(&actor, &context.policy.disable_permission)?;
    let user_id: Uuid = Uuid::parse_str(&auth_user_id)
        .map_err(|_| AdminApiError::Validation("The identity-provider user ID is invalid.".to_owned()))?;
    let account: MappedAccount = load_mapped_account(&context, &actor, &auth_user_id).await?;
    if account.account_id == actor.account_id && request.disabled {
        return Err(AdminApiError::Validation(
            "You cannot disable the account currently in use.".to_owned(),
        ));
    }

    invalidate_account_cache(&context, &actor, &auth_user_id, "before_status_change").await;
    let previously_disabled: bool = account.account_status == AccountStatus::Disabled;
    let provider_user: ExtProviderUser = context
        .admin
        .set_disabled(user_id, request.disabled)
        .await
        .map_err(|error| provider_failure("change Auth user status", &actor, error))?;
    if let Err(error) = update_account_status(&context, &actor, account.account_id, request.disabled).await {
        if let Err(compensation_error) = context.admin.set_disabled(user_id, previously_disabled).await {
            error!(
                "Failed to compensate Auth status change: auth_user_id={} error={}",
                user_id, compensation_error
            );
        }
        record_audit(
            "auth.user.status.change",
            "failed",
            Some(actor.tenant_id),
            Some(actor.account_id),
        );
        return Err(error);
    }
    invalidate_account_cache(&context, &actor, &auth_user_id, "after_status_change").await;

    record_audit(
        "auth.user.status.change",
        "accepted",
        Some(actor.tenant_id),
        Some(actor.account_id),
    );
    let mut updated_account: MappedAccount = account;
    updated_account.account_status = if request.disabled {
        AccountStatus::Disabled
    } else {
        AccountStatus::Active
    };
    info!(
        tenant_id = %actor.tenant_id,
        actor_id = %actor.account_id,
        account_id = %updated_account.account_id,
        auth_user_id = %user_id,
        disabled = request.disabled,
        "Auth user status request completed"
    );
    Ok(Json(summary(updated_account, Some(provider_user))))
}

async fn load_mapped_accounts(
    context: &AuthService,
    actor: &AuthenticatedUser,
) -> Result<Vec<MappedAccount>, AdminApiError> {
    let tenant_id: Uuid = actor.tenant_id;
    let actor_account_id: Uuid = actor.account_id;
    let actor_branch_ids: Vec<Uuid> = actor.branch_ids.clone();
    trace!(
        tenant_id = %tenant_id,
        actor_id = %actor_account_id,
        accessible_branch_count = actor_branch_ids.len(),
        "Loading branch-authorized mapped Auth accounts"
    );
    let rows: Vec<MappedAccountRow> = context
        .db
        .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
            sqlx::query_as!(
                MappedAccountRow,
                r#"
                SELECT identity.subject, account.id AS account_id, account.username, account.email,
                       account.status AS account_status, account.primary_role_code AS primary_role,
                       COALESCE(
                           array_agg(DISTINCT assignment.branch_id ORDER BY assignment.branch_id)
                               FILTER (WHERE assignment.branch_id IS NOT NULL),
                           ARRAY[]::UUID[]
                       ) AS "branch_ids!"
                FROM account_identities AS identity
                INNER JOIN accounts AS account
                    ON account.tenant_id = identity.tenant_id AND account.id = identity.account_id
                LEFT JOIN account_branch_assignments AS assignment
                    ON assignment.tenant_id = account.tenant_id
                   AND assignment.account_id = account.id
                WHERE identity.tenant_id = $1
                  AND EXISTS (
                      SELECT 1
                      FROM account_roles AS actor_role
                      INNER JOIN auth_role_assignment_grants AS role_grant
                          ON role_grant.grantor_role_code = actor_role.role_code
                      WHERE actor_role.tenant_id = $1
                        AND actor_role.account_id = $2
                        AND role_grant.target_role_code = account.primary_role_code
                  )
                  AND (
                      EXISTS (
                          SELECT 1
                          FROM account_roles AS tenant_wide_actor_role
                          INNER JOIN auth_role_branch_assignment_rules AS tenant_wide_rule
                              ON tenant_wide_rule.role_code = tenant_wide_actor_role.role_code
                          WHERE tenant_wide_actor_role.tenant_id = $1
                            AND tenant_wide_actor_role.account_id = $2
                            AND tenant_wide_rule.max_assignments = 0
                      )
                      OR EXISTS (
                          SELECT 1
                          FROM account_branch_assignments AS visible_assignment
                          WHERE visible_assignment.tenant_id = account.tenant_id
                            AND visible_assignment.account_id = account.id
                            AND visible_assignment.branch_id = ANY($3)
                      )
                  )
                GROUP BY identity.subject, account.id, account.username, account.email,
                         account.status, account.primary_role_code
                ORDER BY lower(account.username), account.id
                "#,
                tenant_id,
                actor_account_id,
                &actor_branch_ids,
            )
            .fetch_all(connection)
            .await
        })
        .await
        .map_err(|error: TenantDbErr| {
            error!(tenant_id = %tenant_id, error = %error, "Auth account list tenant operation failed");
            AdminApiError::Internal
        })?;

    let accounts: Vec<MappedAccount> = rows
        .into_iter()
        .map(MappedAccount::try_from)
        .collect::<Result<Vec<MappedAccount>, AdminApiError>>()?;
    debug!(tenant_id = %tenant_id, account_count = accounts.len(), "Mapped Auth accounts loaded");
    Ok(accounts)
}

async fn load_mapped_account(
    context: &AuthService,
    actor: &AuthenticatedUser,
    subject: &str,
) -> Result<MappedAccount, AdminApiError> {
    load_mapped_accounts(context, actor)
        .await?
        .into_iter()
        .find(|account| account.subject == subject)
        .ok_or_else(|| AdminApiError::NotFound("The user was not found in this tenant.".to_owned()))
}

async fn load_mapped_account_by_id(
    context: &AuthService,
    actor: &AuthenticatedUser,
    account_id: Uuid,
) -> Result<MappedAccount, AdminApiError> {
    let accounts: Vec<MappedAccount> = load_mapped_accounts(context, actor).await?;
    accounts
        .into_iter()
        .find(|account: &MappedAccount| account.account_id == account_id)
        .ok_or_else(|| AdminApiError::NotFound("The provisioned account was not found in this tenant.".to_owned()))
}

fn parse_idempotency_key(headers: &HeaderMap) -> Result<Uuid, AdminApiError> {
    let raw_value: &str = headers
        .get(IDEMPOTENCY_HEADER)
        .and_then(|value: &axum::http::HeaderValue| value.to_str().ok())
        .ok_or_else(|| {
            AdminApiError::Validation("A valid Idempotency-Key header is required for user creation.".to_owned())
        })?;
    Uuid::parse_str(raw_value.trim()).map_err(|_error: uuid::Error| {
        AdminApiError::Validation("A valid Idempotency-Key header is required for user creation.".to_owned())
    })
}

fn provisioning_fingerprint(request: &CreateAuthUserRequest) -> String {
    let mut hasher: Sha256 = Sha256::new();
    update_fingerprint_field(&mut hasher, &request.username);
    update_fingerprint_field(&mut hasher, &request.email);
    update_fingerprint_field(&mut hasher, request.primary_role.as_str());
    for branch_id in &request.branch_ids {
        update_fingerprint_field(&mut hasher, &branch_id.to_string());
    }
    match request.password.as_deref() {
        Some(password) => {
            hasher.update([1_u8]);
            update_fingerprint_field(&mut hasher, password);
        }
        None => hasher.update([0_u8]),
    }
    let digest: sha2::digest::Output<Sha256> = hasher.finalize();
    let mut fingerprint: String = String::with_capacity(64);
    for byte in digest {
        let encoded_byte: String = format!("{byte:02x}");
        fingerprint.push_str(&encoded_byte);
    }
    fingerprint
}

fn update_fingerprint_field(hasher: &mut Sha256, value: &str) {
    let length: u64 = u64::try_from(value.len()).unwrap_or(u64::MAX);
    hasher.update(length.to_be_bytes());
    hasher.update(value.as_bytes());
}

async fn claim_provisioning_request(
    context: &AuthService,
    actor: &AuthenticatedUser,
    idempotency_key: Uuid,
    request_fingerprint: &str,
) -> Result<ProvisioningClaim, AdminApiError> {
    debug!(
        tenant_id = %actor.tenant_id,
        actor_id = %actor.account_id,
        idempotency_key = %idempotency_key,
        "Claiming Auth account provisioning request"
    );
    let mut transaction: TenantTransaction = context.db.begin_tenant(actor.tenant_id).await.map_err(|error| {
        error!(
            tenant_id = %actor.tenant_id,
            actor_id = %actor.account_id,
            idempotency_key = %idempotency_key,
            error = %error,
            "Auth provisioning claim transaction failed"
        );
        AdminApiError::Internal
    })?;
    let insert_result: PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO auth_account_provisioning_requests (
            tenant_id, idempotency_key, request_fingerprint, requested_by_account_id
        ) VALUES ($1, $2, $3, $4)
        ON CONFLICT (tenant_id, idempotency_key) DO NOTHING
        "#,
        actor.tenant_id,
        idempotency_key,
        request_fingerprint,
        actor.account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error: sqlx::Error| {
        error!(
            tenant_id = %actor.tenant_id,
            idempotency_key = %idempotency_key,
            error = %error,
            "Auth provisioning claim insert failed"
        );
        AdminApiError::Internal
    })?;
    if insert_result.rows_affected() == 1 {
        transaction.commit().await.map_err(|error: sqlx::Error| {
            error!(
                tenant_id = %actor.tenant_id,
                idempotency_key = %idempotency_key,
                error = %error,
                "Auth provisioning claim commit failed"
            );
            AdminApiError::Internal
        })?;
        return Ok(ProvisioningClaim::Proceed { auth_user_id: None });
    }

    let state: ProvisioningStateRow = sqlx::query_as!(
        ProvisioningStateRow,
        r#"
        SELECT request_fingerprint, status, auth_user_id, account_id,
               (status = 'failed' OR locked_at <= CURRENT_TIMESTAMP - INTERVAL '5 minutes') AS "retry_allowed!"
        FROM auth_account_provisioning_requests
        WHERE tenant_id = $1 AND idempotency_key = $2
        FOR UPDATE
        "#,
        actor.tenant_id,
        idempotency_key,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(|error: sqlx::Error| {
        error!(
            tenant_id = %actor.tenant_id,
            idempotency_key = %idempotency_key,
            error = %error,
            "Auth provisioning claim load failed"
        );
        AdminApiError::Internal
    })?;
    if state.request_fingerprint != request_fingerprint {
        warn!(
            tenant_id = %actor.tenant_id,
            actor_id = %actor.account_id,
            idempotency_key = %idempotency_key,
            "Auth provisioning idempotency key was reused with different input"
        );
        return Err(AdminApiError::Conflict(
            "This idempotency key was already used for a different user request.".to_owned(),
        ));
    }

    let provisioning_status: ProvisioningRequestStatus = ProvisioningRequestStatus::from_code(&state.status)
        .ok_or_else(|| {
            error!(
                tenant_id = %actor.tenant_id,
                idempotency_key = %idempotency_key,
                provisioning_status = %state.status,
                "Auth provisioning request has an unsupported status"
            );
            AdminApiError::Internal
        })?;
    let claim: ProvisioningClaim = if provisioning_status == ProvisioningRequestStatus::Completed {
        let auth_user_id: Uuid = state.auth_user_id.ok_or(AdminApiError::Internal)?;
        let account_id: Uuid = state.account_id.ok_or(AdminApiError::Internal)?;
        ProvisioningClaim::Replay {
            auth_user_id,
            account_id,
        }
    } else if state.retry_allowed {
        let retry_result: PgQueryResult = sqlx::query!(
            r#"
            UPDATE auth_account_provisioning_requests
            SET status = 'processing', locked_at = CURRENT_TIMESTAMP,
                completed_at = NULL, last_error_code = NULL, updated_at = CURRENT_TIMESTAMP
            WHERE tenant_id = $1 AND idempotency_key = $2
            "#,
            actor.tenant_id,
            idempotency_key,
        )
        .execute(transaction.connection())
        .await
        .map_err(|error: sqlx::Error| {
            error!(
                tenant_id = %actor.tenant_id,
                idempotency_key = %idempotency_key,
                error = %error,
                "Auth provisioning retry claim failed"
            );
            AdminApiError::Internal
        })?;
        trace!(
            tenant_id = %actor.tenant_id,
            idempotency_key = %idempotency_key,
            rows_affected = retry_result.rows_affected(),
            "Auth provisioning retry claimed"
        );
        ProvisioningClaim::Proceed {
            auth_user_id: state.auth_user_id,
        }
    } else {
        warn!(
            tenant_id = %actor.tenant_id,
            actor_id = %actor.account_id,
            idempotency_key = %idempotency_key,
            "Auth provisioning request is already processing"
        );
        return Err(AdminApiError::Conflict(
            "This user provisioning request is already being processed.".to_owned(),
        ));
    };
    transaction.commit().await.map_err(|error: sqlx::Error| {
        error!(
            tenant_id = %actor.tenant_id,
            idempotency_key = %idempotency_key,
            error = %error,
            "Auth provisioning claim completion failed"
        );
        AdminApiError::Internal
    })?;
    Ok(claim)
}

async fn resolve_or_create_provider_user(
    context: &AuthService,
    request: &CreateAuthUserRequest,
    tenant_id: Uuid,
    idempotency_key: Uuid,
    known_auth_user_id: Option<Uuid>,
) -> Result<ExtProviderUser, ProviderError> {
    if let Some(auth_user_id) = known_auth_user_id
        && let Some(user) = context.admin.get_user(auth_user_id).await?
    {
        debug!(tenant_id = %tenant_id, auth_user_id = %auth_user_id, idempotency_key = %idempotency_key, "Recovered Auth user by persisted ID");
        return Ok(user);
    }
    if let Some(user) = context
        .admin
        .find_provisioned_user(&request.email, tenant_id, idempotency_key)
        .await?
    {
        return Ok(user);
    }
    context.admin.create_user(request, tenant_id, idempotency_key).await
}

async fn record_provisioned_auth_user(
    context: &AuthService,
    actor: &AuthenticatedUser,
    idempotency_key: Uuid,
    auth_user_id: Uuid,
) -> Result<(), AdminApiError> {
    let result: PgQueryResult = context
        .db
        .run_with_tenant(actor.tenant_id, async move |connection: &mut PgConnection| {
            sqlx::query!(
                r#"
                UPDATE auth_account_provisioning_requests
                SET auth_user_id = $3, updated_at = CURRENT_TIMESTAMP
                WHERE tenant_id = $1 AND idempotency_key = $2 AND status = 'processing'
                "#,
                actor.tenant_id,
                idempotency_key,
                auth_user_id,
            )
            .execute(connection)
            .await
        })
        .await
        .map_err(|error| {
            error!(
                tenant_id = %actor.tenant_id,
                idempotency_key = %idempotency_key,
                auth_user_id = %auth_user_id,
                error = %error,
                "Auth provisioning provider ID persistence failed"
            );
            AdminApiError::Internal
        })?;
    if result.rows_affected() != 1 {
        error!(tenant_id = %actor.tenant_id, idempotency_key = %idempotency_key, "Auth provisioning provider ID target was not processing");
        return Err(AdminApiError::Internal);
    }
    debug!(tenant_id = %actor.tenant_id, idempotency_key = %idempotency_key, auth_user_id = %auth_user_id, "Auth provisioning provider ID persisted");
    Ok(())
}

async fn mark_provisioning_failed(
    context: &AuthService,
    actor: &AuthenticatedUser,
    idempotency_key: Uuid,
    error_code: &'static str,
    retained_auth_user_id: Option<Uuid>,
) {
    let result: Result<PgQueryResult, infra_postgres::TenantDbErr> = context
        .db
        .run_with_tenant(actor.tenant_id, async move |connection: &mut PgConnection| {
            sqlx::query!(
                r#"
                UPDATE auth_account_provisioning_requests
                SET status = 'failed', auth_user_id = $3, account_id = NULL,
                    completed_at = NULL, last_error_code = $4, updated_at = CURRENT_TIMESTAMP
                WHERE tenant_id = $1 AND idempotency_key = $2
                "#,
                actor.tenant_id,
                idempotency_key,
                retained_auth_user_id,
                error_code,
            )
            .execute(connection)
            .await
        })
        .await;
    match result {
        Ok(update_result) => debug!(
            tenant_id = %actor.tenant_id,
            idempotency_key = %idempotency_key,
            rows_affected = update_result.rows_affected(),
            error_code,
            "Auth provisioning failure persisted"
        ),
        Err(error) => error!(
            tenant_id = %actor.tenant_id,
            idempotency_key = %idempotency_key,
            error_code,
            error = %error,
            "Auth provisioning failure persistence failed"
        ),
    }
}

async fn compensate_created_provider_user(
    context: &AuthService,
    actor: &AuthenticatedUser,
    idempotency_key: Uuid,
    auth_user_id: Uuid,
    error_code: &'static str,
) {
    let compensation_result: Result<(), ProviderError> =
        context.admin.delete_user_after_failed_link(auth_user_id).await;
    let retained_auth_user_id: Option<Uuid> = match compensation_result {
        Ok(()) => None,
        Err(error) => {
            error!(
                tenant_id = %actor.tenant_id,
                actor_id = %actor.account_id,
                idempotency_key = %idempotency_key,
                auth_user_id = %auth_user_id,
                error = %error,
                "Failed to compensate unlinked Auth user"
            );
            Some(auth_user_id)
        }
    };
    mark_provisioning_failed(context, actor, idempotency_key, error_code, retained_auth_user_id).await;
}

async fn ensure_username_available(
    context: &AuthService,
    actor: &AuthenticatedUser,
    username: &str,
) -> Result<(), AdminApiError> {
    trace!(tenant_id = %actor.tenant_id, "Checking Auth username availability");
    let tenant_id: Uuid = actor.tenant_id;
    let row: ExistsRow = context
        .db
        .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
            sqlx::query_as!(
                ExistsRow,
                r#"SELECT EXISTS (
                    SELECT 1 FROM accounts WHERE tenant_id = $1 AND lower(username) = lower($2)
                ) AS "exists!""#,
                tenant_id,
                username,
            )
            .fetch_one(connection)
            .await
        })
        .await
        .map_err(|error: TenantDbErr| {
            error!(tenant_id = %tenant_id, error = %error, "Auth username tenant operation failed");
            AdminApiError::Internal
        })?;
    if row.exists {
        warn!(tenant_id = %actor.tenant_id, "Auth username availability check rejected a duplicate");
        Err(AdminApiError::Conflict("The username is already in use.".to_owned()))
    } else {
        debug!(tenant_id = %actor.tenant_id, "Auth username is available");
        Ok(())
    }
}

async fn ensure_role_grantable(
    context: &AuthService,
    actor: &AuthenticatedUser,
    role: &RoleCode,
) -> Result<(), AdminApiError> {
    trace!(tenant_id = %actor.tenant_id, actor_id = %actor.account_id, role = %role, "Checking Auth role assignment grant");
    let tenant_id: Uuid = actor.tenant_id;
    let actor_account_id: Uuid = actor.account_id;
    let row: ExistsRow = context
        .db
        .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
            sqlx::query_as!(
                ExistsRow,
                r#"SELECT EXISTS (
                    SELECT 1
                    FROM account_roles AS actor_role
                    INNER JOIN auth_role_assignment_grants AS role_grant
                        ON role_grant.grantor_role_code = actor_role.role_code
                    INNER JOIN roles AS target_role
                        ON target_role.code = role_grant.target_role_code
                       AND target_role.is_active
                    WHERE actor_role.tenant_id = $1
                      AND actor_role.account_id = $2
                      AND role_grant.target_role_code = $3
                ) AS "exists!""#,
                tenant_id,
                actor_account_id,
                role.as_str(),
            )
            .fetch_one(connection)
            .await
        })
        .await
        .map_err(|error: TenantDbErr| {
            error!(tenant_id = %tenant_id, actor_id = %actor_account_id, role = %role, error = %error, "Auth role assignment tenant operation failed");
            AdminApiError::Internal
        })?;

    if row.exists {
        debug!(tenant_id = %actor.tenant_id, actor_id = %actor.account_id, role = %role, "Auth role assignment grant accepted");
        Ok(())
    } else {
        warn!(tenant_id = %actor.tenant_id, actor_id = %actor.account_id, role = %role, "Auth role assignment grant rejected");
        Err(AdminApiError::Forbidden)
    }
}

async fn ensure_branch_assignments_valid(
    context: &AuthService,
    actor: &AuthenticatedUser,
    role: &RoleCode,
    branch_ids: &[Uuid],
) -> Result<(), AdminApiError> {
    let requested_count: i16 = i16::try_from(branch_ids.len()).map_err(|_error: std::num::TryFromIntError| {
        AdminApiError::Validation("Too many branch assignments were requested.".to_owned())
    })?;
    let tenant_id: Uuid = actor.tenant_id;
    let requested_branch_ids: Vec<Uuid> = branch_ids.to_vec();
    let actor_branch_ids: Vec<Uuid> = actor.branch_ids.clone();
    let role_code: String = role.as_str().to_owned();
    trace!(
        tenant_id = %tenant_id,
        actor_id = %actor.account_id,
        role = %role,
        requested_branch_count = requested_count,
        "Checking branch assignments for Auth account provisioning"
    );
    let rule: Option<BranchAssignmentRuleRow> = context
        .db
        .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
            sqlx::query_as!(
                BranchAssignmentRuleRow,
                r#"
                SELECT rule.min_assignments,
                       rule.max_assignments,
                       COUNT(branch.id)::BIGINT AS "valid_branch_count!"
                FROM auth_role_branch_assignment_rules AS rule
                LEFT JOIN branches AS branch
                    ON branch.tenant_id = $1
                   AND branch.id = ANY($2)
                   AND branch.status = 'active'
                   AND branch.id = ANY($3)
                WHERE rule.role_code = $4
                GROUP BY rule.role_code, rule.min_assignments, rule.max_assignments
                "#,
                tenant_id,
                &requested_branch_ids,
                &actor_branch_ids,
                role_code,
            )
            .fetch_optional(connection)
            .await
        })
        .await
        .map_err(|error: TenantDbErr| {
            error!(
                tenant_id = %actor.tenant_id,
                actor_id = %actor.account_id,
                role = %role,
                error = %error,
                "Auth branch-assignment validation tenant operation failed"
            );
            AdminApiError::Internal
        })?;
    let rule: BranchAssignmentRuleRow = rule.ok_or_else(|| {
        warn!(
            tenant_id = %actor.tenant_id,
            actor_id = %actor.account_id,
            role = %role,
            "Auth role has no configured branch-assignment rule"
        );
        AdminApiError::Validation("The selected role has no branch-assignment policy.".to_owned())
    })?;
    if requested_count < rule.min_assignments
        || rule
            .max_assignments
            .is_some_and(|maximum_assignments: i16| requested_count > maximum_assignments)
    {
        warn!(
            tenant_id = %actor.tenant_id,
            actor_id = %actor.account_id,
            role = %role,
            requested_branch_count = requested_count,
            minimum_branch_count = rule.min_assignments,
            maximum_branch_count = ?rule.max_assignments,
            "Auth branch-assignment cardinality rejected"
        );
        return Err(AdminApiError::Validation(
            "The selected branches do not satisfy the selected role's branch policy.".to_owned(),
        ));
    }
    if rule.valid_branch_count != i64::from(requested_count) {
        warn!(
            tenant_id = %actor.tenant_id,
            actor_id = %actor.account_id,
            role = %role,
            requested_branch_count = requested_count,
            valid_branch_count = rule.valid_branch_count,
            "Auth branch assignment includes an inactive or unauthorized branch"
        );
        return Err(AdminApiError::Forbidden);
    }
    debug!(
        tenant_id = %actor.tenant_id,
        actor_id = %actor.account_id,
        role = %role,
        requested_branch_count = requested_count,
        "Auth branch assignments accepted"
    );
    Ok(())
}

async fn link_created_user(
    context: &AuthAdminContext,
    actor: &AuthenticatedUser,
    request: &CreateAuthUserRequest,
    auth_user_id: Uuid,
    idempotency_key: Uuid,
    request_fingerprint: &str,
) -> Result<MappedAccount, AdminApiError> {
    let account_id: Uuid = Uuid::new_v4();
    debug!(
        tenant_id = %actor.tenant_id,
        actor_id = %actor.account_id,
        account_id = %account_id,
        auth_user_id = %auth_user_id,
        "Linking created Auth user to application account"
    );
    let mut transaction: TenantTransaction = context.db.begin_tenant(actor.tenant_id).await.map_err(|error| {
        error!(
            "Auth account create transaction failed: tenant_id={} error={}",
            actor.tenant_id, error
        );
        AdminApiError::Internal
    })?;
    let account_insert: PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO accounts (
            id, tenant_id, username, email, status, primary_role_code,
            created_by_account_id, updated_by_account_id
        )
        VALUES ($1, $2, $3, $4, 'active', $5, $6, $6)
        "#,
        account_id,
        actor.tenant_id,
        request.username,
        request.email,
        request.primary_role.as_str(),
        actor.account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error| account_create_error("insert account", actor, error))?;
    trace!(rows_affected = account_insert.rows_affected(), account_id = %account_id, "Application account inserted");
    let role_insert: PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO account_roles (tenant_id, account_id, role_code, assigned_by_account_id)
        VALUES ($1, $2, $3, $4)
        "#,
        actor.tenant_id,
        account_id,
        request.primary_role.as_str(),
        actor.account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error: sqlx::Error| account_create_error("assign primary role", actor, error))?;
    trace!(rows_affected = role_insert.rows_affected(), account_id = %account_id, "Primary Auth role assigned");
    for branch_id in &request.branch_ids {
        let assignment_insert: PgQueryResult = sqlx::query!(
            r#"
            INSERT INTO account_branch_assignments (
                tenant_id, account_id, branch_id, assigned_by_account_id
            )
            VALUES ($1, $2, $3, $4)
            "#,
            actor.tenant_id,
            account_id,
            branch_id,
            actor.account_id,
        )
        .execute(transaction.connection())
        .await
        .map_err(|error: sqlx::Error| account_create_error("assign account branch", actor, error))?;
        trace!(
            rows_affected = assignment_insert.rows_affected(),
            account_id = %account_id,
            branch_id = %branch_id,
            "Auth account branch assigned"
        );
    }
    let identity_insert: PgQueryResult = sqlx::query!(
        r#"
        INSERT INTO account_identities (issuer, subject, tenant_id, account_id)
        VALUES ($1, $2, $3, $4)
        "#,
        context.provider.config().issuer,
        auth_user_id.to_string(),
        actor.tenant_id,
        account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error| account_create_error("link Auth identity", actor, error))?;
    trace!(rows_affected = identity_insert.rows_affected(), account_id = %account_id, "External Auth identity linked");
    let provisioning_context: AuthAccountProvisioningContext = AuthAccountProvisioningContext {
        tenant_id: actor.tenant_id,
        actor_account_id: actor.account_id,
        account_id,
        username: request.username.clone(),
        email: request.email.clone(),
        primary_role: request.primary_role.clone(),
        branch_ids: request.branch_ids.clone(),
    };
    context
        .provisioner
        .provision(transaction.connection(), &provisioning_context)
        .await
        .map_err(|error: AuthAccountProvisioningError| {
            error!(
                tenant_id = %actor.tenant_id,
                actor_id = %actor.account_id,
                account_id = %account_id,
                primary_role = %request.primary_role,
                provisioning_error_code = error.code(),
                "Application-specific account provisioning failed"
            );
            AdminApiError::Internal
        })?;
    let completion_result: PgQueryResult = sqlx::query!(
        r#"
        UPDATE auth_account_provisioning_requests
        SET status = 'completed', auth_user_id = $3, account_id = $4,
            completed_at = CURRENT_TIMESTAMP, last_error_code = NULL,
            updated_at = CURRENT_TIMESTAMP
        WHERE tenant_id = $1 AND idempotency_key = $2
          AND request_fingerprint = $5 AND status = 'processing'
        "#,
        actor.tenant_id,
        idempotency_key,
        auth_user_id,
        account_id,
        request_fingerprint,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error: sqlx::Error| account_create_error("complete provisioning request", actor, error))?;
    if completion_result.rows_affected() != 1 {
        error!(
            tenant_id = %actor.tenant_id,
            actor_id = %actor.account_id,
            account_id = %account_id,
            idempotency_key = %idempotency_key,
            "Auth provisioning completion target was not processing"
        );
        return Err(AdminApiError::Internal);
    }
    trace!(
        tenant_id = %actor.tenant_id,
        account_id = %account_id,
        idempotency_key = %idempotency_key,
        "Auth provisioning ledger completed"
    );
    transaction.commit().await.map_err(|error| {
        error!(
            "Auth account create commit failed: tenant_id={} actor_id={} error={}",
            actor.tenant_id, actor.account_id, error
        );
        AdminApiError::Internal
    })?;

    info!(
        tenant_id = %actor.tenant_id,
        account_id = %account_id,
        auth_user_id = %auth_user_id,
        "Created Auth user linked to application account"
    );
    Ok(MappedAccount {
        subject: auth_user_id.to_string(),
        account_id,
        username: request.username.clone(),
        account_status: AccountStatus::Active,
        email: Some(request.email.clone()),
        primary_role: request.primary_role.clone(),
        branch_ids: request.branch_ids.clone(),
    })
}

async fn invalidate_account_cache(context: &AuthService, actor: &AuthenticatedUser, subject: &str, phase: &str) {
    let invalidation_result: Result<(), crate::ext_foundation::account_cache::AuthenticatedUserCacheError> = context
        .account_cache
        .invalidate(&context.provider.config().issuer, subject)
        .await;
    if let Err(cache_error) = invalidation_result {
        warn!(
            operation = "invalidate_auth_admin_account_cache",
            tenant_id = %actor.tenant_id,
            actor_id = %actor.account_id,
            phase,
            reason = %cache_error,
            "Auth administration cache invalidation failed; mandatory cache expiry limits stale access"
        );
    }
}

async fn update_account_status(
    context: &AuthService,
    actor: &AuthenticatedUser,
    account_id: Uuid,
    disabled: bool,
) -> Result<(), AdminApiError> {
    let account_status: AccountStatus = if disabled {
        AccountStatus::Disabled
    } else {
        AccountStatus::Active
    };
    let status: &str = account_status.as_code();
    debug!(
        tenant_id = %actor.tenant_id,
        actor_id = %actor.account_id,
        account_id = %account_id,
        status,
        "Updating application account status"
    );
    let tenant_id: Uuid = actor.tenant_id;
    let actor_account_id: Uuid = actor.account_id;
    let result: PgQueryResult = context
        .db
        .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
            sqlx::query!(
                r#"
                UPDATE accounts
                SET status = $3, updated_at = CURRENT_TIMESTAMP, updated_by_account_id = $4
                WHERE tenant_id = $1 AND id = $2
                "#,
                tenant_id,
                account_id,
                status,
                actor_account_id,
            )
            .execute(connection)
            .await
        })
        .await
        .map_err(|error: TenantDbErr| {
            error!(tenant_id = %tenant_id, account_id = %account_id, error = %error, "Auth account status tenant operation failed");
            AdminApiError::Internal
        })?;
    if result.rows_affected() != 1 {
        warn!(tenant_id = %actor.tenant_id, account_id = %account_id, "Application account status target was not found");
        return Err(AdminApiError::NotFound(
            "The account to update was not found.".to_owned(),
        ));
    }
    info!(tenant_id = %actor.tenant_id, account_id = %account_id, status, "Application account status updated");
    Ok(())
}

fn normalize_create_request(request: &mut CreateAuthUserRequest) -> Result<(), AdminApiError> {
    request.username = request.username.trim().to_owned();
    request.email = request.email.trim().to_ascii_lowercase();
    request.password = request.password.take().filter(|password| !password.is_empty());
    request.branch_ids.sort_unstable();
    request.branch_ids.dedup();

    if !(3..=128).contains(&request.username.chars().count()) {
        return Err(AdminApiError::Validation(
            "Username must contain between 3 and 128 characters.".to_owned(),
        ));
    }
    if request.email.len() > 320
        || !request
            .email
            .split_once('@')
            .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.') && !domain.ends_with('.'))
    {
        return Err(AdminApiError::Validation("Email is invalid.".to_owned()));
    }
    if request
        .password
        .as_ref()
        .is_some_and(|password| password.chars().count() < 8)
    {
        return Err(AdminApiError::Validation(
            "Password must contain at least 8 characters.".to_owned(),
        ));
    }
    Ok(())
}

fn require_permission(actor: &AuthenticatedUser, permission: &PermissionCode) -> Result<(), AdminApiError> {
    if actor.has_permission(permission.as_str()) {
        trace!(tenant_id = %actor.tenant_id, actor_id = %actor.account_id, permission = %permission, "Auth administration permission accepted");
        Ok(())
    } else {
        warn!(tenant_id = %actor.tenant_id, actor_id = %actor.account_id, permission = %permission, "Auth administration permission rejected");
        Err(AdminApiError::Forbidden)
    }
}

fn summary(account: MappedAccount, provider_user: Option<ExtProviderUser>) -> AuthUserSummary {
    let provider_status: AuthProviderUserStatus =
        provider_user
            .as_ref()
            .map_or(AuthProviderUserStatus::Missing, |user: &ExtProviderUser| {
                if user_is_banned(user) {
                    AuthProviderUserStatus::Disabled
                } else {
                    AuthProviderUserStatus::Active
                }
            });
    AuthUserSummary {
        auth_user_id: account.subject,
        account_id: account.account_id,
        username: account.username,
        email: account.email,
        primary_role: account.primary_role,
        branch_ids: account.branch_ids,
        account_status: account.account_status,
        provider_status,
        email_confirmed: provider_user
            .as_ref()
            .and_then(|user| user.email_confirmed_at.as_ref())
            .is_some(),
        created_at: provider_user.as_ref().map(|user| user.created_at.clone()),
        last_sign_in_at: provider_user.and_then(|user| user.last_sign_in_at),
    }
}

fn user_is_banned(user: &ExtProviderUser) -> bool {
    user.banned_until
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .is_some_and(|until| until.with_timezone(&Utc) > Utc::now())
}

fn account_create_error(operation: &str, actor: &AuthenticatedUser, error: sqlx::Error) -> AdminApiError {
    error!(
        "Auth account create step failed: operation={} tenant_id={} actor_id={} error={}",
        operation, actor.tenant_id, actor.account_id, error
    );
    if error
        .as_database_error()
        .is_some_and(|database_error| database_error.is_unique_violation())
    {
        AdminApiError::Conflict("The email or username is already in use.".to_owned())
    } else {
        AdminApiError::Internal
    }
}

fn provider_failure(operation: &str, actor: &AuthenticatedUser, error: ProviderError) -> AdminApiError {
    match &error {
        ProviderError::Transport(transport_error) => error!(
            operation,
            tenant_id = %actor.tenant_id,
            actor_id = %actor.account_id,
            timeout = transport_error.is_timeout(),
            connect = transport_error.is_connect(),
            "Auth provider administration transport failed"
        ),
        ProviderError::Response { status, .. } => warn!(
            operation,
            tenant_id = %actor.tenant_id,
            actor_id = %actor.account_id,
            status,
            "Auth provider administration request was rejected"
        ),
        ProviderError::InvalidResponse(response_error) => error!(
            operation,
            tenant_id = %actor.tenant_id,
            actor_id = %actor.account_id,
            decode = response_error.is_decode(),
            "Auth provider administration returned an invalid response"
        ),
    }
    match error {
        ProviderError::Response {
            status: 400 | 422,
            message,
        } => AdminApiError::Validation(message),
        ProviderError::Response { status: 409, message } => AdminApiError::Conflict(message),
        ProviderError::Response { status: 404, message } => AdminApiError::NotFound(message),
        ProviderError::Transport(_) | ProviderError::InvalidResponse(_) | ProviderError::Response { .. } => {
            AdminApiError::ProviderUnavailable
        }
    }
}

async fn read_provider_response<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, ProviderError> {
    let status = response.status();
    if status.is_success() {
        return response.json::<T>().await.map_err(ProviderError::InvalidResponse);
    }
    let message = response
        .text()
        .await
        .map(|body| provider_message(&body))
        .unwrap_or_else(|_| "Auth provider rejected the request".to_owned());
    Err(ProviderError::Response {
        status: status.as_u16(),
        message,
    })
}

fn provider_message(body: &str) -> String {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("msg")
                .or_else(|| value.get("message"))
                .or_else(|| value.get("error_description"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "Auth provider rejected the request".to_owned());
    message.chars().take(300).collect()
}

fn record_audit(action: &str, outcome: &str, tenant_id: Option<Uuid>, actor_id: Option<Uuid>) {
    tracing::info!(
        audit = true,
        action,
        outcome,
        tenant_id = ?tenant_id,
        actor_id = ?actor_id,
        "audit event"
    );
}

fn required_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use crate::RoleCode;

    use super::{
        CreateAuthUserRequest, ExtProviderUser, normalize_create_request, provider_message, provisioning_fingerprint,
        user_is_banned,
    };
    use uuid::Uuid;

    #[test]
    fn normalizes_valid_create_request() {
        let mut request: CreateAuthUserRequest = CreateAuthUserRequest {
            username: "  Linh Nguyen  ".to_owned(),
            email: " LINH@EXAMPLE.COM ".to_owned(),
            password: Some("correct-horse".to_owned()),
            primary_role: RoleCode::parse("custom_role").expect("valid test role code"),
            branch_ids: vec![
                Uuid::parse_str("00000000-0000-4000-8000-000000000002").expect("valid branch ID"),
                Uuid::parse_str("00000000-0000-4000-8000-000000000001").expect("valid branch ID"),
                Uuid::parse_str("00000000-0000-4000-8000-000000000001").expect("valid branch ID"),
            ],
        };
        assert!(normalize_create_request(&mut request).is_ok());
        assert_eq!(request.username, "Linh Nguyen");
        assert_eq!(request.email, "linh@example.com");
        assert_eq!(request.branch_ids.len(), 2);
        assert!(request.branch_ids.is_sorted());
    }

    #[test]
    fn accepts_social_only_user_without_password() {
        let mut request: CreateAuthUserRequest = CreateAuthUserRequest {
            username: "linh".to_owned(),
            email: "linh@example.com".to_owned(),
            password: Some(String::new()),
            primary_role: RoleCode::parse("custom_role").expect("valid test role code"),
            branch_ids: Vec::new(),
        };
        assert!(normalize_create_request(&mut request).is_ok());
        assert!(request.password.is_none());
    }

    #[test]
    fn detects_future_ban_and_sanitizes_provider_message() {
        let user: ExtProviderUser = ExtProviderUser {
            id: Uuid::nil(),
            email: None,
            email_confirmed_at: None,
            last_sign_in_at: None,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
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
    fn provisioning_fingerprint_is_stable_and_covers_password() {
        let request: CreateAuthUserRequest = CreateAuthUserRequest {
            username: "linh".to_owned(),
            email: "linh@example.com".to_owned(),
            password: Some("first-password".to_owned()),
            primary_role: RoleCode::parse("staff").expect("valid test role code"),
            branch_ids: vec![Uuid::parse_str("00000000-0000-4000-8000-000000000001").expect("valid branch ID")],
        };
        let same_fingerprint: String = provisioning_fingerprint(&request);
        let mut changed_request: CreateAuthUserRequest = request.clone();
        changed_request.password = Some("second-password".to_owned());
        let changed_fingerprint: String = provisioning_fingerprint(&changed_request);
        let mut changed_branch_request: CreateAuthUserRequest = request.clone();
        changed_branch_request.branch_ids =
            vec![Uuid::parse_str("00000000-0000-4000-8000-000000000002").expect("valid branch ID")];

        assert_eq!(same_fingerprint, provisioning_fingerprint(&request));
        assert_ne!(same_fingerprint, changed_fingerprint);
        assert_ne!(same_fingerprint, provisioning_fingerprint(&changed_branch_request));
    }
}
