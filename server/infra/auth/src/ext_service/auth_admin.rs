use std::{ops::Deref, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, put},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use infra_postgres::{TenantDbErr, TenantTransaction};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, postgres::PgQueryResult};
use tracing::{debug, error, info, trace, warn};
use ts_rs::TS;
use uuid::Uuid;

use crate::{
    AuthCodeError, AuthService, PermissionCode, RoleCode,
    ext_service::ListPaginationPolicy,
    ext_service::account::{AccountStatus, AuthedUser},
};

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
    provisioner: Arc<dyn AuthProvisioner>,
    pagination: ListPaginationPolicy,
}

impl Deref for AuthAdminContext {
    type Target = AuthService;

    fn deref(&self) -> &Self::Target {
        &self.auth
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExtAdminErr {
    #[error("external identity request is invalid: {0}")]
    Validation(String),
    #[error("external identity conflicts with existing provider state: {0}")]
    Conflict(String),
    #[error("external identity was not found: {0}")]
    NotFound(String),
    #[error("external identity provider is unavailable: {0}")]
    Unavailable(String),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalIdentityStatus {
    Active,
    Disabled,
}

#[derive(Clone, Debug)]
pub struct ExternalIdentity {
    pub subject: String,
    pub email: Option<String>,
    pub status: ExternalIdentityStatus,
    pub email_confirmed: bool,
    pub created_at: Option<String>,
    pub last_sign_in_at: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CreateExternalIdentityRequest {
    pub username: String,
    pub email: String,
    pub password: Option<String>,
    pub tenant_id: Uuid,
    pub idempotency_key: Uuid,
}

#[async_trait]
pub trait ExtAuthAdmin: Send + Sync {
    async fn get_identity(&self, subject: &str) -> Result<Option<ExternalIdentity>, ExtAdminErr>;

    async fn find_identity_by_email(&self, normalized_email: &str) -> Result<Option<ExternalIdentity>, ExtAdminErr>;

    async fn find_provisioned_identity(
        &self,
        normalized_email: &str,
        tenant_id: Uuid,
        idempotency_key: Uuid,
    ) -> Result<Option<ExternalIdentity>, ExtAdminErr>;

    async fn create_identity(&self, request: &CreateExternalIdentityRequest) -> Result<ExternalIdentity, ExtAdminErr>;
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
    is_system: bool,
    min_assignments: Option<i16>,
    max_assignments: Option<i16>,
    valid_branch_count: i64,
}

#[derive(Clone, Debug)]
pub struct AcctProvisionContext {
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
pub struct AcctProvisionErr {
    code: &'static str,
}

impl AcctProvisionErr {
    pub const fn new(code: &'static str) -> Self {
        Self { code }
    }

    pub(crate) const fn code(&self) -> &'static str {
        self.code
    }
}

#[derive(Clone, Debug)]
pub struct AuthAccountAccessContext {
    pub tenant_id: Uuid,
    pub actor_account_id: Uuid,
    pub account_id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub primary_role: RoleCode,
    pub branch_ids: Vec<Uuid>,
}

#[async_trait]
pub trait AuthProvisioner: Send + Sync {
    async fn provision(
        &self,
        connection: &mut PgConnection,
        context: &AcctProvisionContext,
    ) -> Result<(), AcctProvisionErr>;

    async fn update_access(
        &self,
        _connection: &mut PgConnection,
        _context: &AuthAccountAccessContext,
    ) -> Result<(), AcctProvisionErr> {
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct NoopAuthAccountProvisioner;

#[async_trait]
impl AuthProvisioner for NoopAuthAccountProvisioner {
    async fn provision(
        &self,
        _connection: &mut PgConnection,
        _context: &AcctProvisionContext,
    ) -> Result<(), AcctProvisionErr> {
        Ok(())
    }
}

#[derive(Debug)]
struct ProvisioningStateRow {
    request_fingerprint: String,
    status: String,
    auth_user_id: Option<String>,
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
    Proceed { auth_user_id: Option<String> },
    Replay { auth_user_id: String, account_id: Uuid },
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

#[derive(Debug, Deserialize)]
struct AuthUserPageQuery {
    limit: Option<u16>,
    cursor: Option<String>,
    search: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct AuthUserCursor {
    username: String,
    account_id: Uuid,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct AuthUserPage {
    pub items: Vec<AuthUserSummary>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub limit: u16,
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

pub fn routes(auth: Arc<AuthService>, policy: AuthAdminPolicy, pagination: ListPaginationPolicy) -> Router {
    routes_with_provisioner(auth, policy, Arc::new(NoopAuthAccountProvisioner), pagination)
}

pub fn routes_with_provisioner(
    auth: Arc<AuthService>,
    policy: AuthAdminPolicy,
    provisioner: Arc<dyn AuthProvisioner>,
    pagination: ListPaginationPolicy,
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
        pagination,
    });
    Router::new()
        .route("/admin/auth-users", get(list_users).post(create_user))
        .route("/admin/auth-users/{auth_user_id}/status", put(set_user_status))
        .with_state(state)
}

async fn list_users(
    State(context): State<Arc<AuthAdminContext>>,
    Extension(actor): Extension<AuthedUser>,
    Query(query): Query<AuthUserPageQuery>,
) -> Result<Json<AuthUserPage>, AdminApiError> {
    info!(tenant_id = %actor.tenant_id, actor_id = %actor.account_id, "Auth user list request accepted");
    require_permission(&actor, &context.policy.read_permission)?;
    let limit = context
        .pagination
        .resolve(query.limit)
        .map_err(AdminApiError::Validation)?;
    let cursor: Option<AuthUserCursor> = query.cursor.as_deref().map(decode_auth_user_cursor).transpose()?;
    let search: Option<String> = query.search.map(|value| value.trim().to_lowercase()).filter(|value| !value.is_empty());
    let mut accounts: Vec<MappedAccount> = load_mapped_accounts_page(
        &context,
        &actor,
        i64::from(limit) + 1,
        cursor.as_ref(),
        search.as_deref(),
    )
    .await?;
    let has_more = accounts.len() > usize::from(limit);
    accounts.truncate(usize::from(limit));
    let next_cursor = if has_more {
        accounts
            .last()
            .map(|account| {
                encode_auth_user_cursor(&AuthUserCursor {
                    username: account.username.to_lowercase(),
                    account_id: account.account_id,
                })
            })
            .transpose()?
    } else {
        None
    };
    let mut users: Vec<AuthUserSummary> = Vec::with_capacity(accounts.len());
    for account in accounts {
        let provider_user: Option<ExternalIdentity> = context
            .auth_admin
            .get_identity(&account.subject)
            .await
            .map_err(|error: ExtAdminErr| provider_failure("load Auth user", &actor, error))?;
        users.push(summary(account, provider_user));
    }
    info!(
        tenant_id = %actor.tenant_id,
        actor_id = %actor.account_id,
        user_count = users.len(),
        "Auth user list request completed"
    );
    Ok(Json(AuthUserPage {
        items: users,
        next_cursor,
        has_more,
        limit,
    }))
}

fn decode_auth_user_cursor(value: &str) -> Result<AuthUserCursor, AdminApiError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AdminApiError::Validation("The Auth user cursor is invalid.".to_owned()))?;
    serde_json::from_slice(&bytes).map_err(|_| AdminApiError::Validation("The Auth user cursor is invalid.".to_owned()))
}

fn encode_auth_user_cursor(cursor: &AuthUserCursor) -> Result<String, AdminApiError> {
    let bytes = serde_json::to_vec(cursor).map_err(|_| AdminApiError::Internal)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

async fn load_mapped_accounts_page(
    context: &AuthService,
    actor: &AuthedUser,
    limit: i64,
    cursor: Option<&AuthUserCursor>,
    search: Option<&str>,
) -> Result<Vec<MappedAccount>, AdminApiError> {
    let tenant_id = actor.tenant_id;
    let actor_account_id = actor.account_id;
    let actor_branch_ids = actor.branch_ids.clone();
    let cursor_username = cursor.map(|value| value.username.clone());
    let cursor_account_id = cursor.map(|value| value.account_id);
    let search = search.map(str::to_owned);
    let rows: Vec<MappedAccountRow> = context
        .db
        .tran_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
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
                JOIN accounts AS account
                  ON account.tenant_id = identity.tenant_id AND account.id = identity.account_id
                LEFT JOIN account_role_assignments AS assignment
                  ON assignment.tenant_id = account.tenant_id
                 AND assignment.account_id = account.id
                 AND assignment.role_code = account.primary_role_code
                WHERE identity.tenant_id = $1
                  AND (
                      EXISTS (
                          SELECT 1 FROM account_role_assignments AS tenant_actor
                          WHERE tenant_actor.tenant_id = $1
                            AND tenant_actor.account_id = $2
                            AND tenant_actor.branch_id IS NULL
                      )
                      OR EXISTS (
                          SELECT 1 FROM account_role_assignments AS visible
                          WHERE visible.tenant_id = account.tenant_id
                            AND visible.account_id = account.id
                            AND visible.branch_id = ANY($3)
                      )
                  )
                  AND ($4::TEXT IS NULL
                       OR lower(account.username) LIKE '%' || $4 || '%'
                       OR lower(COALESCE(account.email, '')) LIKE '%' || $4 || '%')
                  AND ($5::TEXT IS NULL OR (lower(account.username), account.id) > ($5, $6::UUID))
                GROUP BY identity.subject, account.id, account.username, account.email,
                         account.status, account.primary_role_code
                ORDER BY lower(account.username), account.id
                LIMIT $7
                "#,
                tenant_id,
                actor_account_id,
                &actor_branch_ids,
                search,
                cursor_username,
                cursor_account_id,
                limit,
            )
            .fetch_all(connection)
            .await
        })
        .await
        .map_err(|error| {
            error!(tenant_id = %tenant_id, reason = %error, "Paginated Auth account query failed");
            AdminApiError::Internal
        })?;
    rows.into_iter()
        .map(MappedAccount::try_from)
        .collect::<Result<Vec<_>, _>>()
}

async fn create_user(
    State(context): State<Arc<AuthAdminContext>>,
    Extension(actor): Extension<AuthedUser>,
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
    ensure_role_grantable(&context, &actor, &request.primary_role, &request.branch_ids).await?;
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
        let provider_user: ExternalIdentity = context
            .auth_admin
            .get_identity(&auth_user_id)
            .await
            .map_err(|error: ExtAdminErr| provider_failure("replay Auth user creation", &actor, error))?
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

    let provider_result: Result<ExternalIdentity, ExtAdminErr> =
        resolve_or_create_provider_user(&context, &request, actor.tenant_id, idempotency_key, auth_user_id).await;
    let provider_user: ExternalIdentity = match provider_result {
        Ok(provider_user) => provider_user,
        Err(error) => {
            mark_provisioning_failed(&context, &actor, idempotency_key, "provider_create_failed", None).await;
            return Err(provider_failure("create Auth user", &actor, error));
        }
    };
    if let Err(error) = record_provisioned_auth_user(&context, &actor, idempotency_key, &provider_user.subject).await {
        retain_provider_user_after_failed_link(
            &context,
            &actor,
            idempotency_key,
            &provider_user.subject,
            "provider_record_failed",
        )
        .await;
        return Err(error);
    }

    let account: MappedAccount = match link_created_user(
        &context,
        &actor,
        &request,
        &provider_user.subject,
        idempotency_key,
        &request_fingerprint,
    )
    .await
    {
        Ok(account) => account,
        Err(error) => {
            retain_provider_user_after_failed_link(
                &context,
                &actor,
                idempotency_key,
                &provider_user.subject,
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
        auth_user_id = %provider_user.subject,
        idempotency_key = %idempotency_key,
        "Auth user creation request completed"
    );
    Ok((StatusCode::CREATED, Json(summary(account, Some(provider_user)))))
}

async fn set_user_status(
    State(context): State<Arc<AuthAdminContext>>,
    Extension(actor): Extension<AuthedUser>,
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
    let account: MappedAccount = load_mapped_account(&context, &actor, &auth_user_id).await?;
    if account.account_id == actor.account_id && request.disabled {
        return Err(AdminApiError::Validation(
            "You cannot disable the account currently in use.".to_owned(),
        ));
    }

    invalidate_account_cache(&context, &actor, &auth_user_id, "before_status_change").await;
    let provider_user: ExternalIdentity = context
        .auth_admin
        .get_identity(&auth_user_id)
        .await
        .map_err(|error: ExtAdminErr| {
            provider_failure("load Auth user before tenant account status change", &actor, error)
        })?
        .ok_or_else(|| AdminApiError::NotFound("The identity-provider user was not found.".to_owned()))?;
    update_account_status(&context, &actor, account.account_id, request.disabled).await?;
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
        auth_user_id = %auth_user_id,
        disabled = request.disabled,
        "Tenant-local application account status request completed without changing the shared provider identity"
    );
    Ok(Json(summary(updated_account, Some(provider_user))))
}

async fn load_mapped_account_record(
    context: &AuthService,
    actor: &AuthedUser,
    subject: Option<&str>,
    account_id: Option<Uuid>,
) -> Result<Option<MappedAccount>, AdminApiError> {
    let tenant_id: Uuid = actor.tenant_id;
    let actor_account_id: Uuid = actor.account_id;
    let actor_branch_ids: Vec<Uuid> = actor.branch_ids.clone();
    let subject: Option<String> = subject.map(str::to_owned);
    trace!(
        tenant_id = %tenant_id,
        actor_id = %actor_account_id,
        accessible_branch_count = actor_branch_ids.len(),
        "Loading one branch-authorized mapped Auth account"
    );
    let row: Option<MappedAccountRow> = context
        .db
        .tran_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
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
                LEFT JOIN account_role_assignments AS assignment
                    ON assignment.tenant_id = account.tenant_id
                   AND assignment.account_id = account.id
                   AND assignment.role_code = account.primary_role_code
                WHERE identity.tenant_id = $1
                  AND (
                      EXISTS (
                          SELECT 1
                          FROM account_role_assignments AS tenant_wide_actor_role
                          WHERE tenant_wide_actor_role.tenant_id = $1
                            AND tenant_wide_actor_role.account_id = $2
                            AND tenant_wide_actor_role.branch_id IS NULL
                      )
                      OR EXISTS (
                          SELECT 1
                          FROM account_role_assignments AS visible_assignment
                          WHERE visible_assignment.tenant_id = account.tenant_id
                            AND visible_assignment.account_id = account.id
                            AND visible_assignment.branch_id = ANY($3)
                      )
                  )
                  AND ($4::TEXT IS NULL OR identity.subject = $4)
                  AND ($5::UUID IS NULL OR account.id = $5)
                GROUP BY identity.subject, account.id, account.username, account.email,
                         account.status, account.primary_role_code
                ORDER BY identity.subject
                LIMIT 1
                "#,
                tenant_id,
                actor_account_id,
                &actor_branch_ids,
                subject,
                account_id,
            )
            .fetch_optional(connection)
            .await
        })
        .await
        .map_err(|error: TenantDbErr| {
            error!(tenant_id = %tenant_id, error = %error, "Auth account lookup tenant operation failed");
            AdminApiError::Internal
        })?;
    row.map(MappedAccount::try_from).transpose()
}

async fn load_mapped_account(
    context: &AuthService,
    actor: &AuthedUser,
    subject: &str,
) -> Result<MappedAccount, AdminApiError> {
    load_mapped_account_record(context, actor, Some(subject), None)
        .await?
        .ok_or_else(|| AdminApiError::NotFound("The user was not found in this tenant.".to_owned()))
}

async fn load_mapped_account_by_id(
    context: &AuthService,
    actor: &AuthedUser,
    account_id: Uuid,
) -> Result<MappedAccount, AdminApiError> {
    load_mapped_account_record(context, actor, None, Some(account_id))
        .await?
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
    actor: &AuthedUser,
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
        let auth_user_id: String = state.auth_user_id.ok_or(AdminApiError::Internal)?;
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
    known_auth_user_id: Option<String>,
) -> Result<ExternalIdentity, ExtAdminErr> {
    if let Some(auth_user_id) = known_auth_user_id
        && let Some(user) = context.auth_admin.get_identity(&auth_user_id).await?
    {
        debug!(tenant_id = %tenant_id, auth_user_id = %auth_user_id, idempotency_key = %idempotency_key, "Recovered Auth user by persisted ID");
        return Ok(user);
    }
    if let Some(user) = context
        .auth_admin
        .find_provisioned_identity(&request.email, tenant_id, idempotency_key)
        .await?
    {
        return Ok(user);
    }
    if let Some(user) = context.auth_admin.find_identity_by_email(&request.email).await? {
        info!(
            tenant_id = %tenant_id,
            auth_user_id = %user.subject,
            password_supplied_but_ignored = request.password.is_some(),
            "Reusing existing external identity for an additional tenant membership"
        );
        return Ok(user);
    }
    let create_request: CreateExternalIdentityRequest = CreateExternalIdentityRequest {
        username: request.username.clone(),
        email: request.email.clone(),
        password: request.password.clone(),
        tenant_id,
        idempotency_key,
    };
    context.auth_admin.create_identity(&create_request).await
}

async fn record_provisioned_auth_user(
    context: &AuthService,
    actor: &AuthedUser,
    idempotency_key: Uuid,
    auth_user_id: &str,
) -> Result<(), AdminApiError> {
    let result: PgQueryResult = context
        .db
        .tran_with_tenant(actor.tenant_id, async move |connection: &mut PgConnection| {
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
    actor: &AuthedUser,
    idempotency_key: Uuid,
    error_code: &'static str,
    retained_auth_user_id: Option<String>,
) {
    let result: Result<PgQueryResult, infra_postgres::TenantDbErr> = context
        .db
        .tran_with_tenant(actor.tenant_id, async move |connection: &mut PgConnection| {
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

async fn retain_provider_user_after_failed_link(
    context: &AuthService,
    actor: &AuthedUser,
    idempotency_key: Uuid,
    auth_user_id: &str,
    error_code: &'static str,
) {
    warn!(
        tenant_id = %actor.tenant_id,
        actor_id = %actor.account_id,
        idempotency_key = %idempotency_key,
        auth_user_id = %auth_user_id,
        "Retaining Auth identity after application link failure because the identity may belong to other tenants"
    );
    mark_provisioning_failed(
        context,
        actor,
        idempotency_key,
        error_code,
        Some(auth_user_id.to_owned()),
    )
    .await;
}

async fn ensure_username_available(
    context: &AuthService,
    actor: &AuthedUser,
    username: &str,
) -> Result<(), AdminApiError> {
    trace!(tenant_id = %actor.tenant_id, "Checking Auth username availability");
    let tenant_id: Uuid = actor.tenant_id;
    let row: ExistsRow = context
        .db
        .tran_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
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
    context: &AuthAdminContext,
    actor: &AuthedUser,
    role: &RoleCode,
    branch_ids: &[Uuid],
) -> Result<(), AdminApiError> {
    trace!(tenant_id = %actor.tenant_id, actor_id = %actor.account_id, role = %role, "Checking Auth role assignment grant");
    let tenant_id: Uuid = actor.tenant_id;
    let actor_account_id: Uuid = actor.account_id;
    let requested_branch_ids: Vec<Uuid> = branch_ids.to_vec();
    let row: ExistsRow = context
        .db
        .tran_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
            sqlx::query_as!(
                ExistsRow,
                r#"SELECT EXISTS (
                    SELECT 1
                    FROM tenant_roles AS target_role
                    WHERE target_role.tenant_id = $1
                      AND target_role.code = $3
                      AND target_role.is_active
                      AND (
                          (
                              target_role.is_system
                              AND EXISTS (
                                  SELECT 1
                                  FROM account_roles AS actor_role
                                  INNER JOIN auth_role_assignment_grants AS role_grant
                                      ON role_grant.grantor_role_code = actor_role.role_code
                                  WHERE actor_role.tenant_id = $1
                                    AND actor_role.account_id = $2
                                    AND role_grant.target_role_code = target_role.code
                              )
                          )
                          OR (
                              NOT target_role.is_system
                              AND (
                                  (
                                      target_role.scope_type = 'tenant'
                                      AND shepherd_account_has_tenant_permission($1, $2, $4)
                                  )
                                  OR (
                                      target_role.scope_type = 'branch'
                                      AND cardinality($5::UUID[]) > 0
                                      AND NOT EXISTS (
                                          SELECT 1
                                          FROM unnest($5::UUID[]) AS requested(branch_id)
                                          WHERE NOT shepherd_account_has_permission(
                                              $1,
                                              $2,
                                              requested.branch_id,
                                              $4
                                          )
                                      )
                                  )
                              )
                          )
                      )
                ) AS "exists!""#,
                tenant_id,
                actor_account_id,
                role.as_str(),
                context.policy.role_manage_permission.as_str(),
                &requested_branch_ids,
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
    actor: &AuthedUser,
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
        .tran_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
            sqlx::query_as!(
                BranchAssignmentRuleRow,
                r#"
                SELECT tenant_role.is_system,
                       rule.min_assignments,
                       rule.max_assignments,
                       COUNT(branch.id)::BIGINT AS "valid_branch_count!"
                FROM tenant_roles AS tenant_role
                LEFT JOIN auth_role_branch_assignment_rules AS rule
                    ON rule.role_code = tenant_role.code
                LEFT JOIN branches AS branch
                    ON branch.tenant_id = $1
                   AND branch.id = ANY($2)
                   AND branch.status = 'active'
                   AND branch.id = ANY($3)
                WHERE tenant_role.tenant_id = $1
                  AND tenant_role.code = $4
                  AND tenant_role.is_active
                GROUP BY tenant_role.code, tenant_role.is_system,
                         rule.min_assignments, rule.max_assignments
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
    let minimum_assignments: i16 = if rule.is_system {
        rule.min_assignments.ok_or_else(|| {
            AdminApiError::Validation("The selected system role has no branch-assignment policy.".to_owned())
        })?
    } else {
        1
    };
    let maximum_assignments: Option<i16> = if rule.is_system { rule.max_assignments } else { Some(1) };
    if requested_count < minimum_assignments
        || maximum_assignments.is_some_and(|maximum: i16| requested_count > maximum)
    {
        warn!(
            tenant_id = %actor.tenant_id,
            actor_id = %actor.account_id,
            role = %role,
            requested_branch_count = requested_count,
            minimum_branch_count = minimum_assignments,
            maximum_branch_count = ?maximum_assignments,
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
    actor: &AuthedUser,
    request: &CreateAuthUserRequest,
    auth_user_id: &str,
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
    let tenant_role = sqlx::query!(
        r#"
        SELECT scope_type, is_system
        FROM tenant_roles
        WHERE tenant_id = $1 AND code = $2 AND is_active
        FOR SHARE
        "#,
        actor.tenant_id,
        request.primary_role.as_str(),
    )
    .fetch_optional(transaction.connection())
    .await
    .map_err(|error: sqlx::Error| account_create_error("lock tenant role", actor, error))?
    .ok_or_else(|| AdminApiError::Validation("The selected role is no longer active.".to_owned()))?;
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
    if tenant_role.scope_type == "tenant" {
        sqlx::query!(
            r#"
            INSERT INTO account_role_assignments (
                tenant_id, account_id, role_code, branch_id, assigned_by_account_id
            )
            VALUES ($1, $2, $3, NULL, $4)
            "#,
            actor.tenant_id,
            account_id,
            request.primary_role.as_str(),
            actor.account_id,
        )
        .execute(transaction.connection())
        .await
        .map_err(|error: sqlx::Error| account_create_error("assign tenant primary role", actor, error))?;
    } else {
        for branch_id in &request.branch_ids {
            sqlx::query!(
                r#"
                INSERT INTO account_role_assignments (
                    tenant_id, account_id, role_code, branch_id, assigned_by_account_id
                )
                VALUES ($1, $2, $3, $4, $5)
                "#,
                actor.tenant_id,
                account_id,
                request.primary_role.as_str(),
                branch_id,
                actor.account_id,
            )
            .execute(transaction.connection())
            .await
            .map_err(|error: sqlx::Error| account_create_error("assign branch primary role", actor, error))?;
        }
    }
    trace!(account_id = %account_id, role = %request.primary_role, "Tenant primary Auth role assigned");
    if tenant_role.is_system {
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
        .map_err(|error: sqlx::Error| account_create_error("assign legacy primary role", actor, error))?;
        trace!(rows_affected = role_insert.rows_affected(), account_id = %account_id, "Legacy primary Auth role assigned");
    }
    for branch_id in &request.branch_ids {
        if !tenant_role.is_system {
            continue;
        }
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
        context.token_verifier.config().issuer,
        auth_user_id,
        actor.tenant_id,
        account_id,
    )
    .execute(transaction.connection())
    .await
    .map_err(|error| account_create_error("link Auth identity", actor, error))?;
    trace!(rows_affected = identity_insert.rows_affected(), account_id = %account_id, "External Auth identity linked");
    let provisioning_context: AcctProvisionContext = AcctProvisionContext {
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
        .map_err(|error: AcctProvisionErr| {
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
        subject: auth_user_id.to_owned(),
        account_id,
        username: request.username.clone(),
        account_status: AccountStatus::Active,
        email: Some(request.email.clone()),
        primary_role: request.primary_role.clone(),
        branch_ids: request.branch_ids.clone(),
    })
}

async fn invalidate_account_cache(context: &AuthService, actor: &AuthedUser, subject: &str, phase: &str) {
    let invalidation_result: Result<(), crate::ext_service::account_cache::AuthedCacheErr> = context
        .acct_cache
        .invalidate(&context.token_verifier.config().issuer, subject, actor.tenant_id)
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
    actor: &AuthedUser,
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
        .tran_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
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

fn require_permission(actor: &AuthedUser, permission: &PermissionCode) -> Result<(), AdminApiError> {
    if actor.has_permission(permission.as_str()) {
        trace!(tenant_id = %actor.tenant_id, actor_id = %actor.account_id, permission = %permission, "Auth administration permission accepted");
        Ok(())
    } else {
        warn!(tenant_id = %actor.tenant_id, actor_id = %actor.account_id, permission = %permission, "Auth administration permission rejected");
        Err(AdminApiError::Forbidden)
    }
}

fn summary(account: MappedAccount, provider_user: Option<ExternalIdentity>) -> AuthUserSummary {
    let provider_status: AuthProviderUserStatus =
        provider_user
            .as_ref()
            .map_or(AuthProviderUserStatus::Missing, |user: &ExternalIdentity| {
                match user.status {
                    ExternalIdentityStatus::Active => AuthProviderUserStatus::Active,
                    ExternalIdentityStatus::Disabled => AuthProviderUserStatus::Disabled,
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
            .is_some_and(|user: &ExternalIdentity| user.email_confirmed),
        created_at: provider_user.as_ref().and_then(|user| user.created_at.clone()),
        last_sign_in_at: provider_user.and_then(|user| user.last_sign_in_at),
    }
}

fn account_create_error(operation: &str, actor: &AuthedUser, error: sqlx::Error) -> AdminApiError {
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

fn provider_failure(operation: &str, actor: &AuthedUser, error: ExtAdminErr) -> AdminApiError {
    warn!(
        operation,
        tenant_id = %actor.tenant_id,
        actor_id = %actor.account_id,
        reason = %error,
        "External identity administration failed"
    );
    match error {
        ExtAdminErr::Validation(message) => AdminApiError::Validation(message),
        ExtAdminErr::Conflict(message) => AdminApiError::Conflict(message),
        ExtAdminErr::NotFound(message) => AdminApiError::NotFound(message),
        ExtAdminErr::Unavailable(_message) => AdminApiError::ProviderUnavailable,
    }
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

#[cfg(test)]
mod tests {
    use crate::RoleCode;

    use super::{CreateAuthUserRequest, normalize_create_request, provisioning_fingerprint};
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
    fn provisioning_fingerprint_is_stable_and_covers_password() {
        let request: CreateAuthUserRequest = CreateAuthUserRequest {
            username: "linh".to_owned(),
            email: "linh@example.com".to_owned(),
            password: Some("first-password".to_owned()),
            primary_role: RoleCode::parse("operator").expect("valid test role code"),
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
