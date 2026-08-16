use std::{ops::Deref, sync::Arc, time::Duration};

use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, put},
};
use chrono::{DateTime, Utc};
use infra_kernel::debug::*;
use reqwest::Url;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use ts_rs::TS;
use uuid::Uuid;

use crate::{AuthService, ext_foundation::account::AuthenticatedUser};

const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 5;
const DISABLED_DURATION: &str = "876000h";

/// Application-owned permission codes required by the reusable account routes.
#[derive(Clone, Copy, Debug)]
pub struct AuthAdminPolicy {
    pub read_permission: &'static str,
    pub create_permission: &'static str,
    pub disable_permission: &'static str,
}

struct AuthAdminContext {
    auth: Arc<AuthService>,
    policy: AuthAdminPolicy,
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
}

#[derive(Clone, Debug)]
struct MappedAccount {
    subject: String,
    account_id: Uuid,
    username: String,
    account_status: String,
    primary_role: String,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct AuthUserSummary {
    pub auth_user_id: String,
    #[ts(type = "string")]
    pub account_id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub primary_role: String,
    pub account_status: String,
    pub provider_status: String,
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
    pub primary_role: String,
}

#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export)]
pub struct SetAuthUserStatusRequest {
    pub disabled: bool,
}

impl AuthAdminService {
    pub fn from_env() -> Result<Arc<Self>, AuthAdminConfigError> {
        let raw_url = required_env("AUTH_ADMIN_URL").ok_or(AuthAdminConfigError::MissingUrl)?;
        let parsed_url = Url::parse(&raw_url).map_err(|error| AuthAdminConfigError::InvalidUrl(error.to_string()))?;
        if !matches!(parsed_url.scheme(), "http" | "https") || parsed_url.host_str().is_none() {
            return Err(AuthAdminConfigError::UnsupportedUrl);
        }
        let timeout_secs =
            std::env::var("AUTH_ADMIN_HTTP_TIMEOUT_SECS").map_or(Ok(DEFAULT_HTTP_TIMEOUT_SECS), |value| {
                value
                    .parse::<u64>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or(AuthAdminConfigError::InvalidTimeout)
            })?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .map_err(AuthAdminConfigError::Client)?;

        Ok(Arc::new(Self {
            client,
            base_url: raw_url.trim().trim_end_matches('/').to_owned(),
            admin_token: required_env("AUTH_ADMIN_TOKEN").ok_or(AuthAdminConfigError::MissingToken)?,
        }))
    }

    async fn get_user(&self, user_id: Uuid) -> Result<Option<ExtProviderUser>, ProviderError> {
        let response = self
            .client
            .get(format!("{}/admin/users/{user_id}", self.base_url))
            .bearer_auth(&self.admin_token)
            .send()
            .await
            .map_err(ProviderError::Transport)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        read_provider_response(response).await.map(Some)
    }

    async fn create_user(&self, request: &CreateAuthUserRequest) -> Result<ExtProviderUser, ProviderError> {
        let mut attributes = serde_json::Map::new();
        attributes.insert("email".to_owned(), json!(request.email));
        attributes.insert("email_confirm".to_owned(), json!(true));
        attributes.insert("role".to_owned(), json!("authenticated"));
        attributes.insert("user_metadata".to_owned(), json!({ "username": request.username }));
        attributes.insert("app_metadata".to_owned(), json!({ "managed_by": "infra-auth" }));
        if let Some(password) = request.password.as_ref() {
            attributes.insert("password".to_owned(), json!(password));
        }
        let response = self
            .client
            .post(format!("{}/admin/users", self.base_url))
            .bearer_auth(&self.admin_token)
            .json(&attributes)
            .send()
            .await
            .map_err(ProviderError::Transport)?;
        read_provider_response(response).await
    }

    async fn set_disabled(&self, user_id: Uuid, disabled: bool) -> Result<ExtProviderUser, ProviderError> {
        let response = self
            .client
            .put(format!("{}/admin/users/{user_id}", self.base_url))
            .bearer_auth(&self.admin_token)
            .json(&json!({
                "ban_duration": if disabled { DISABLED_DURATION } else { "none" }
            }))
            .send()
            .await
            .map_err(ProviderError::Transport)?;
        read_provider_response(response).await
    }

    async fn delete_user_after_failed_link(&self, user_id: Uuid) {
        let result = self
            .client
            .delete(format!("{}/admin/users/{user_id}", self.base_url))
            .bearer_auth(&self.admin_token)
            .send()
            .await;
        if let Err(error) = result {
            log_error!(
                "Failed to compensate unlinked Auth user: auth_user_id={} error={}",
                user_id,
                error
            );
        }
    }
}

pub fn routes(auth: Arc<AuthService>, policy: AuthAdminPolicy) -> Router {
    let state = Arc::new(AuthAdminContext { auth, policy });
    Router::new()
        .route("/admin/auth-users", get(list_users).post(create_user))
        .route("/admin/auth-users/{auth_user_id}/status", put(set_user_status))
        .with_state(state)
}

async fn list_users(
    State(context): State<Arc<AuthAdminContext>>,
    Extension(actor): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<AuthUserSummary>>, AdminApiError> {
    require_permission(&actor, context.policy.read_permission)?;
    let accounts = load_mapped_accounts(&context, actor.tenant_id).await?;
    let mut users = Vec::with_capacity(accounts.len());
    for account in accounts {
        let provider_user = match Uuid::parse_str(&account.subject) {
            Ok(user_id) => context
                .admin
                .get_user(user_id)
                .await
                .map_err(|error| provider_failure("load Auth user", &actor, error))?,
            Err(error) => {
                log_error!(
                    "Mapped Auth subject is not a UUID: tenant_id={} account_id={} error={}",
                    actor.tenant_id,
                    account.account_id,
                    error
                );
                None
            }
        };
        users.push(summary(account, provider_user));
    }
    Ok(Json(users))
}

async fn create_user(
    State(context): State<Arc<AuthAdminContext>>,
    Extension(actor): Extension<AuthenticatedUser>,
    Json(mut request): Json<CreateAuthUserRequest>,
) -> Result<(StatusCode, Json<AuthUserSummary>), AdminApiError> {
    require_permission(&actor, context.policy.create_permission)?;
    normalize_create_request(&mut request)?;
    ensure_username_available(&context, &actor, &request.username).await?;
    ensure_role_available(&context, &actor, &request.primary_role).await?;

    let provider_user = context
        .admin
        .create_user(&request)
        .await
        .map_err(|error| provider_failure("create Auth user", &actor, error))?;
    let account = match link_created_user(&context, &actor, &request, provider_user.id).await {
        Ok(account) => account,
        Err(error) => {
            context.admin.delete_user_after_failed_link(provider_user.id).await;
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
    Ok((StatusCode::CREATED, Json(summary(account, Some(provider_user)))))
}

async fn set_user_status(
    State(context): State<Arc<AuthAdminContext>>,
    Extension(actor): Extension<AuthenticatedUser>,
    Path(auth_user_id): Path<String>,
    Json(request): Json<SetAuthUserStatusRequest>,
) -> Result<Json<AuthUserSummary>, AdminApiError> {
    require_permission(&actor, context.policy.disable_permission)?;
    let user_id = Uuid::parse_str(&auth_user_id)
        .map_err(|_| AdminApiError::Validation("The identity-provider user ID is invalid.".to_owned()))?;
    let account = load_mapped_account(&context, &actor, &auth_user_id).await?;
    if account.account_id == actor.account_id && request.disabled {
        return Err(AdminApiError::Validation(
            "You cannot disable the account currently in use.".to_owned(),
        ));
    }

    let previously_disabled = account.account_status == "disabled";
    let provider_user = context
        .admin
        .set_disabled(user_id, request.disabled)
        .await
        .map_err(|error| provider_failure("change Auth user status", &actor, error))?;
    if let Err(error) = update_account_status(&context, &actor, account.account_id, request.disabled).await {
        if let Err(compensation_error) = context.admin.set_disabled(user_id, previously_disabled).await {
            log_error!(
                "Failed to compensate Auth status change: auth_user_id={} error={}",
                user_id,
                compensation_error
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

    record_audit(
        "auth.user.status.change",
        "accepted",
        Some(actor.tenant_id),
        Some(actor.account_id),
    );
    let mut updated_account = account;
    updated_account.account_status = if request.disabled { "disabled" } else { "active" }.to_owned();
    Ok(Json(summary(updated_account, Some(provider_user))))
}

async fn load_mapped_accounts(context: &AuthService, tenant_id: Uuid) -> Result<Vec<MappedAccount>, AdminApiError> {
    let mut transaction = context.db.begin_tenant(tenant_id).await.map_err(|error| {
        log_error!(
            "Auth account list transaction failed: tenant_id={} error={}",
            tenant_id,
            error
        );
        AdminApiError::Internal
    })?;
    let rows = sqlx::query!(
        r#"
        SELECT identity.subject, account.id AS account_id, account.username,
               account.status AS account_status, account.primary_role_code AS primary_role
        FROM account_identities AS identity
        INNER JOIN accounts AS account
            ON account.tenant_id = identity.tenant_id AND account.id = identity.account_id
        WHERE identity.tenant_id = $1
        ORDER BY lower(account.username), account.id
        "#,
        tenant_id,
    )
    .fetch_all(transaction.connection())
    .await
    .map_err(|error| {
        log_error!("Auth account list failed: tenant_id={} error={}", tenant_id, error);
        AdminApiError::Internal
    })?;
    transaction.commit().await.map_err(|error| {
        log_error!(
            "Auth account list commit failed: tenant_id={} error={}",
            tenant_id,
            error
        );
        AdminApiError::Internal
    })?;

    Ok(rows
        .into_iter()
        .map(|row| MappedAccount {
            subject: row.subject,
            account_id: row.account_id,
            username: row.username,
            account_status: row.account_status,
            primary_role: row.primary_role,
        })
        .collect())
}

async fn load_mapped_account(
    context: &AuthService,
    actor: &AuthenticatedUser,
    subject: &str,
) -> Result<MappedAccount, AdminApiError> {
    load_mapped_accounts(context, actor.tenant_id)
        .await?
        .into_iter()
        .find(|account| account.subject == subject)
        .ok_or_else(|| AdminApiError::NotFound("The user was not found in this tenant.".to_owned()))
}

async fn ensure_username_available(
    context: &AuthService,
    actor: &AuthenticatedUser,
    username: &str,
) -> Result<(), AdminApiError> {
    let mut transaction = context.db.begin_tenant(actor.tenant_id).await.map_err(|error| {
        log_error!(
            "Auth username check transaction failed: tenant_id={} error={}",
            actor.tenant_id,
            error
        );
        AdminApiError::Internal
    })?;
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS (
            SELECT 1 FROM accounts WHERE tenant_id = $1 AND lower(username) = lower($2)
        ) AS "exists!""#,
        actor.tenant_id,
        username,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(|error| {
        log_error!(
            "Auth username check failed: tenant_id={} error={}",
            actor.tenant_id,
            error
        );
        AdminApiError::Internal
    })?;
    transaction.commit().await.map_err(|error| {
        log_error!(
            "Auth username check commit failed: tenant_id={} error={}",
            actor.tenant_id,
            error
        );
        AdminApiError::Internal
    })?;
    if exists {
        Err(AdminApiError::Conflict("The username is already in use.".to_owned()))
    } else {
        Ok(())
    }
}

async fn ensure_role_available(
    context: &AuthService,
    actor: &AuthenticatedUser,
    role: &str,
) -> Result<(), AdminApiError> {
    let mut transaction = context.db.begin_tenant(actor.tenant_id).await.map_err(|error| {
        log_error!(
            "Auth role check transaction failed: tenant_id={} error={}",
            actor.tenant_id,
            error
        );
        AdminApiError::Internal
    })?;
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS (
            SELECT 1 FROM roles WHERE code = $1 AND is_active
        ) AS "exists!""#,
        role,
    )
    .fetch_one(transaction.connection())
    .await
    .map_err(|error| {
        log_error!(
            "Auth role check failed: tenant_id={} role={} error={}",
            actor.tenant_id,
            role,
            error
        );
        AdminApiError::Internal
    })?;
    transaction.commit().await.map_err(|error| {
        log_error!(
            "Auth role check commit failed: tenant_id={} role={} error={}",
            actor.tenant_id,
            role,
            error
        );
        AdminApiError::Internal
    })?;

    if exists {
        Ok(())
    } else {
        Err(AdminApiError::Validation("Role is not available.".to_owned()))
    }
}

async fn link_created_user(
    context: &AuthService,
    actor: &AuthenticatedUser,
    request: &CreateAuthUserRequest,
    auth_user_id: Uuid,
) -> Result<MappedAccount, AdminApiError> {
    let account_id = Uuid::new_v4();
    let mut transaction = context.db.begin_tenant(actor.tenant_id).await.map_err(|error| {
        log_error!(
            "Auth account create transaction failed: tenant_id={} error={}",
            actor.tenant_id,
            error
        );
        AdminApiError::Internal
    })?;
    sqlx::query!(
        r#"
        INSERT INTO accounts (
            id, tenant_id, username, status, primary_role_code,
            created_by_account_id, updated_by_account_id
        )
        VALUES ($1, $2, $3, 'active', $4, $5, $5)
        "#,
        account_id,
        actor.tenant_id,
        request.username,
        request.primary_role,
        actor.account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error| account_create_error("insert account", actor, error))?;
    sqlx::query!(
        r#"
        INSERT INTO account_roles (tenant_id, account_id, role_code, assigned_by_account_id)
        VALUES ($1, $2, $3, $4)
        "#,
        actor.tenant_id,
        account_id,
        request.primary_role,
        actor.account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error| account_create_error("assign primary role", actor, error))?;
    sqlx::query!(
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
    transaction.commit().await.map_err(|error| {
        log_error!(
            "Auth account create commit failed: tenant_id={} actor_id={} error={}",
            actor.tenant_id,
            actor.account_id,
            error
        );
        AdminApiError::Internal
    })?;

    Ok(MappedAccount {
        subject: auth_user_id.to_string(),
        account_id,
        username: request.username.clone(),
        account_status: "active".to_owned(),
        primary_role: request.primary_role.clone(),
    })
}

async fn update_account_status(
    context: &AuthService,
    actor: &AuthenticatedUser,
    account_id: Uuid,
    disabled: bool,
) -> Result<(), AdminApiError> {
    let status = if disabled { "disabled" } else { "active" };
    let mut transaction = context.db.begin_tenant(actor.tenant_id).await.map_err(|error| {
        log_error!(
            "Auth account status transaction failed: tenant_id={} account_id={} error={}",
            actor.tenant_id,
            account_id,
            error
        );
        AdminApiError::Internal
    })?;
    let result = sqlx::query!(
        r#"
        UPDATE accounts
        SET status = $3, updated_at = CURRENT_TIMESTAMP, updated_by_account_id = $4
        WHERE tenant_id = $1 AND id = $2
        "#,
        actor.tenant_id,
        account_id,
        status,
        actor.account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error| {
        log_error!(
            "Auth account status update failed: tenant_id={} account_id={} error={}",
            actor.tenant_id,
            account_id,
            error
        );
        AdminApiError::Internal
    })?;
    if result.rows_affected() != 1 {
        return Err(AdminApiError::NotFound(
            "The account to update was not found.".to_owned(),
        ));
    }
    transaction.commit().await.map_err(|error| {
        log_error!(
            "Auth account status commit failed: tenant_id={} account_id={} error={}",
            actor.tenant_id,
            account_id,
            error
        );
        AdminApiError::Internal
    })
}

fn normalize_create_request(request: &mut CreateAuthUserRequest) -> Result<(), AdminApiError> {
    request.username = request.username.trim().to_owned();
    request.email = request.email.trim().to_ascii_lowercase();
    request.primary_role = request.primary_role.trim().to_owned();
    request.password = request.password.take().filter(|password| !password.is_empty());

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

fn require_permission(actor: &AuthenticatedUser, permission: &str) -> Result<(), AdminApiError> {
    if actor.has_permission(permission) {
        Ok(())
    } else {
        Err(AdminApiError::Forbidden)
    }
}

fn summary(account: MappedAccount, provider_user: Option<ExtProviderUser>) -> AuthUserSummary {
    let provider_status =
        provider_user.as_ref().map_or(
            "missing",
            |user| {
                if user_is_banned(user) { "disabled" } else { "active" }
            },
        );
    AuthUserSummary {
        auth_user_id: account.subject,
        account_id: account.account_id,
        username: account.username,
        email: provider_user.as_ref().and_then(|user| user.email.clone()),
        primary_role: account.primary_role,
        account_status: account.account_status,
        provider_status: provider_status.to_owned(),
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
    log_error!(
        "Auth account create step failed: operation={} tenant_id={} actor_id={} error={}",
        operation,
        actor.tenant_id,
        actor.account_id,
        error
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
    log_error!(
        "Auth provider administration failed: operation={} tenant_id={} actor_id={} error={}",
        operation,
        actor.tenant_id,
        actor.account_id,
        error
    );
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
    use super::{CreateAuthUserRequest, ExtProviderUser, normalize_create_request, provider_message, user_is_banned};
    use uuid::Uuid;

    #[test]
    fn normalizes_valid_create_request() {
        let mut request = CreateAuthUserRequest {
            username: "  Linh Nguyen  ".to_owned(),
            email: " LINH@EXAMPLE.COM ".to_owned(),
            password: Some("correct-horse".to_owned()),
            primary_role: "custom_role".to_owned(),
        };
        assert!(normalize_create_request(&mut request).is_ok());
        assert_eq!(request.username, "Linh Nguyen");
        assert_eq!(request.email, "linh@example.com");
    }

    #[test]
    fn accepts_social_only_user_without_password() {
        let mut request = CreateAuthUserRequest {
            username: "linh".to_owned(),
            email: "linh@example.com".to_owned(),
            password: Some(String::new()),
            primary_role: "custom_role".to_owned(),
        };
        assert!(normalize_create_request(&mut request).is_ok());
        assert!(request.password.is_none());
    }

    #[test]
    fn detects_future_ban_and_sanitizes_provider_message() {
        let user = ExtProviderUser {
            id: Uuid::nil(),
            email: None,
            email_confirmed_at: None,
            last_sign_in_at: None,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            banned_until: Some("2099-01-01T00:00:00Z".to_owned()),
        };
        assert!(user_is_banned(&user));
        assert_eq!(
            provider_message(r#"{"msg":"Email already exists"}"#),
            "Email already exists"
        );
    }
}
