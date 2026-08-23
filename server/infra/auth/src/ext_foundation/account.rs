use std::{collections::BTreeSet, sync::Arc};

use axum::{
    Extension, Json, Router,
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
    routing::get,
};
use infra_kernel::request::PrincipalRateLimitKey;
use infra_postgres::{DatabaseAdapter, TenantDbErr, with_active_branch};
use sqlx::PgConnection;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, trace, warn};
use ts_rs::TS;
use uuid::Uuid;

use crate::{
    AuthService, PermissionCode, RoleCode,
    ext_foundation::{AuthenticatedPrincipal, account_cache::AuthenticatedUserCacheError},
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    Active,
    Disabled,
}

impl AccountStatus {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "active" => Some(Self::Active),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PermissionEffect {
    Allow,
    Deny,
}

impl PermissionEffect {
    fn from_code(code: &str) -> Option<Self> {
        match code {
            "allow" => Some(Self::Allow),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuthenticatedUser {
    pub tenant_id: Uuid,
    pub account_id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub primary_role: RoleCode,
    pub roles: Vec<RoleCode>,
    pub permissions: Vec<PermissionCode>,
    pub branch_ids: Vec<Uuid>,
    pub active_branch_id: Option<Uuid>,
}

impl AuthenticatedUser {
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions
            .iter()
            .any(|current: &PermissionCode| current.as_str() == permission)
    }

    fn profile(&self) -> CurrentUserProfile {
        CurrentUserProfile {
            tenant_id: self.tenant_id,
            account_id: self.account_id,
            username: self.username.clone(),
            email: self.email.clone(),
            primary_role: self.primary_role.clone(),
            roles: self.roles.clone(),
            permissions: self.permissions.clone(),
            branch_ids: self.branch_ids.clone(),
            active_branch_id: self.active_branch_id,
        }
    }
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct CurrentUserProfile {
    #[ts(type = "string")]
    pub tenant_id: Uuid,
    #[ts(type = "string")]
    pub account_id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub primary_role: RoleCode,
    pub roles: Vec<RoleCode>,
    pub permissions: Vec<PermissionCode>,
    #[ts(type = "Array<string>")]
    pub branch_ids: Vec<Uuid>,
    #[ts(type = "string | null")]
    pub active_branch_id: Option<Uuid>,
}

pub fn routes(auth: Arc<AuthService>) -> Router {
    info!("Configured external authentication account routes");
    Router::new().route("/me", get(current_user)).with_state(auth)
}

async fn current_user(Extension(user): Extension<AuthenticatedUser>) -> Json<CurrentUserProfile> {
    let profile: CurrentUserProfile = user.profile();
    debug!(
        operation = "current_user_profile",
        tenant_id = %profile.tenant_id,
        account_id = %profile.account_id,
        primary_role = %profile.primary_role,
        role_count = profile.roles.len(),
        permission_count = profile.permissions.len(),
        "Returning authenticated current-user profile"
    );
    Json(profile)
}

struct AccountIdentity {
    pub tenant_id: Uuid,
    pub account_id: Uuid,
}

struct UserAccount {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub username: String,
    pub status: String,
    pub email: Option<String>,
    pub primary_role_code: String,
}

struct AccountRole {
    pub role_code: String,
}

struct AccountPermission {
    pub permission_code: String,
    pub effect: String,
}

struct AccountBranch {
    pub branch_id: Uuid,
}

const ACTIVE_BRANCH_HEADER: &str = "x-shepherd-branch-id";

pub async fn resolve_application_account(
    State(ctx): State<Arc<AuthService>>,
    Extension(principal): Extension<AuthenticatedPrincipal>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let method: String = request.method().as_str().to_owned();
    let path: String = request.uri().path().to_owned();
    let issuer: String = principal.issuer.clone();
    let subject: String = principal.subject.clone();
    trace!(
        operation = "resolve_application_account",
        method = %method,
        path = %path,
        issuer = %issuer,
        subject = %subject,
        "Resolving application account for authenticated external identity"
    );
    let cache_result: Result<Option<AuthenticatedUser>, AuthenticatedUserCacheError> =
        ctx.account_cache.get(&principal).await;
    let cached_user: Option<AuthenticatedUser> = match cache_result {
        Ok(user) => user,
        Err(cache_error) => {
            warn!(
                operation = "resolve_application_account",
                issuer = %issuer,
                subject = %subject,
                reason = %cache_error,
                "Authenticated-user cache unavailable; resolving from PostgreSQL"
            );
            None
        }
    };
    let mut user: AuthenticatedUser = match cached_user {
        Some(cached_user) if principal.tenant_id == Some(cached_user.tenant_id) => {
            trace!(
                operation = "resolve_application_account",
                tenant_id = %cached_user.tenant_id,
                account_id = %cached_user.account_id,
                "Verified JWT tenant claim against cached application account"
            );
            cached_user
        }
        cached_user => {
            if let Some(cached_user) = cached_user {
                warn!(
                    operation = "resolve_application_account",
                    claimed_tenant_id = ?principal.tenant_id,
                    cached_tenant_id = %cached_user.tenant_id,
                    account_id = %cached_user.account_id,
                    "JWT tenant claim does not match cached account; reloading PostgreSQL authority"
                );
            } else {
                debug!(
                    operation = "resolve_application_account",
                    claimed_tenant_id = ?principal.tenant_id,
                    "Authenticated-user cache missed; loading PostgreSQL authority"
                );
            }
            let loaded_user: AuthenticatedUser = load_account(&ctx.db, &principal).await?;
            let cache_write_result: Result<(), AuthenticatedUserCacheError> =
                ctx.account_cache.put(&principal, &loaded_user).await;
            if let Err(cache_error) = cache_write_result {
                warn!(
                    operation = "resolve_application_account",
                    tenant_id = %loaded_user.tenant_id,
                    account_id = %loaded_user.account_id,
                    reason = %cache_error,
                    "Authenticated-user cache write failed; request will continue"
                );
            }
            loaded_user
        }
    };
    if principal.tenant_id != Some(user.tenant_id) {
        warn!(
            operation = "resolve_application_account",
            issuer = %issuer,
            subject = %subject,
            claimed_tenant_id = ?principal.tenant_id,
            authoritative_tenant_id = %user.tenant_id,
            account_id = %user.account_id,
            "JWT tenant claim is missing or stale; requesting a signed token refresh"
        );
        return Err(StatusCode::UNAUTHORIZED);
    }
    let active_branch_id: Uuid = resolve_active_branch(request.headers(), &user)?;
    user.active_branch_id = Some(active_branch_id);
    let tenant_id: Uuid = user.tenant_id;
    let account_id: Uuid = user.account_id;
    debug!(
        operation = "resolve_application_account",
        tenant_id = %tenant_id,
        account_id = %account_id,
        role_count = user.roles.len(),
        permission_count = user.permissions.len(),
        active_branch_id = %active_branch_id,
        accessible_branch_count = user.branch_ids.len(),
        "Resolved active application account for external identity"
    );
    request
        .extensions_mut()
        .insert(PrincipalRateLimitKey::new(format!("{tenant_id}:{account_id}")));
    request.extensions_mut().insert(user);
    let response: Response = with_active_branch(active_branch_id, next.run(request)).await;
    info!(
        operation = "resolve_application_account",
        method = %method,
        path = %path,
        tenant_id = %tenant_id,
        account_id = %account_id,
        status = response.status().as_u16(),
        "Protected request completed after application account resolution"
    );
    Ok(response)
}

fn resolve_active_branch(headers: &HeaderMap, user: &AuthenticatedUser) -> Result<Uuid, StatusCode> {
    let requested_branch_id: Option<Uuid> = headers
        .get(ACTIVE_BRANCH_HEADER)
        .map(|value| value.to_str().map_err(|_| StatusCode::BAD_REQUEST))
        .transpose()?
        .map(|value: &str| Uuid::parse_str(value).map_err(|_| StatusCode::BAD_REQUEST))
        .transpose()?;
    let active_branch_id: Uuid = match requested_branch_id {
        Some(branch_id) if user.branch_ids.contains(&branch_id) => branch_id,
        Some(branch_id) => {
            warn!(
                operation = "resolve_active_branch",
                tenant_id = %user.tenant_id,
                account_id = %user.account_id,
                requested_branch_id = %branch_id,
                accessible_branch_count = user.branch_ids.len(),
                "Account attempted to select an unauthorized branch"
            );
            return Err(StatusCode::FORBIDDEN);
        }
        None => user.branch_ids.first().copied().ok_or_else(|| {
            warn!(
                operation = "resolve_active_branch",
                tenant_id = %user.tenant_id,
                account_id = %user.account_id,
                "Active account has no accessible active branch"
            );
            StatusCode::FORBIDDEN
        })?,
    };
    debug!(
        operation = "resolve_active_branch",
        tenant_id = %user.tenant_id,
        account_id = %user.account_id,
        active_branch_id = %active_branch_id,
        requested_explicitly = requested_branch_id.is_some(),
        "Resolved validated active branch for protected request"
    );
    Ok(active_branch_id)
}

async fn load_account(
    db: &DatabaseAdapter,
    principal: &AuthenticatedPrincipal,
) -> Result<AuthenticatedUser, StatusCode> {
    let identity: AccountIdentity = sqlx::query_as!(
        AccountIdentity,
        r#"
        SELECT tenant_id, account_id
        FROM account_identities
        WHERE issuer = $1 AND subject = $2
        "#,
        principal.issuer,
        principal.subject,
    )
    // The tenant is not known until this global identity mapping is resolved.
    .fetch_optional(db.global_pool())
    .await
    .map_err(|database_error: sqlx::Error| {
        error!(
            issuer = %principal.issuer,
            subject = %principal.subject,
            reason = %database_error,
            "Application account identity lookup failed"
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?
    .ok_or_else(|| {
        warn!(
            issuer = %principal.issuer,
            subject = %principal.subject,
            "Authenticated external identity has no active application account mapping"
        );
        StatusCode::FORBIDDEN
    })?;
    trace!(
        operation = "load_application_account",
        tenant_id = %identity.tenant_id,
        account_id = %identity.account_id,
        "External identity mapped to application account"
    );

    let tenant_id: Uuid = identity.tenant_id;
    let account_id: Uuid = identity.account_id;
    let authorization_rows: (
        Option<UserAccount>,
        Vec<AccountRole>,
        Vec<AccountPermission>,
        Vec<AccountBranch>,
    ) = db
        .run_with_tenant(tenant_id, async move |connection: &mut PgConnection| {
            let account: Option<UserAccount> = sqlx::query_as!(
                UserAccount,
                r#"
                SELECT id, tenant_id, username, email, status, primary_role_code
                FROM accounts
                WHERE tenant_id = $1 AND id = $2
                "#,
                tenant_id,
                account_id,
            )
            .fetch_optional(&mut *connection)
            .await?;
            let role_rows: Vec<AccountRole> = sqlx::query_as!(
                AccountRole,
                r#"
                SELECT role_code
                FROM account_roles
                WHERE tenant_id = $1 AND account_id = $2
                ORDER BY role_code
                "#,
                tenant_id,
                account_id,
            )
            .fetch_all(&mut *connection)
            .await?;
            let permission_rows: Vec<AccountPermission> = sqlx::query_as!(
                AccountPermission,
                r#"
                SELECT permission_code AS "permission_code!", effect AS "effect!"
                FROM (
                    SELECT role_permission.permission_code, 'allow'::TEXT AS effect, 0 AS precedence
                    FROM account_roles AS account_role
                    INNER JOIN role_permissions AS role_permission ON role_permission.role_code = account_role.role_code
                    WHERE account_role.tenant_id = $1 AND account_role.account_id = $2
                    UNION ALL
                    SELECT permission_code AS "permission_code!", effect AS "effect!", 1 AS precedence
                    FROM account_permissions
                    WHERE tenant_id = $1 AND account_id = $2
                      AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP)
                ) AS grants
                ORDER BY permission_code, precedence
                "#,
                tenant_id,
                account_id,
            )
            .fetch_all(&mut *connection)
            .await?;
            let branch_rows: Vec<AccountBranch> = sqlx::query_as!(
                AccountBranch,
                r#"
                SELECT branch.id AS branch_id
                FROM branches AS branch
                INNER JOIN accounts AS account
                    ON account.tenant_id = branch.tenant_id
                   AND account.id = $2
                INNER JOIN auth_role_branch_assignment_rules AS branch_rule
                    ON branch_rule.role_code = account.primary_role_code
                LEFT JOIN account_branch_assignments AS assignment
                    ON assignment.tenant_id = account.tenant_id
                   AND assignment.account_id = account.id
                   AND assignment.branch_id = branch.id
                WHERE branch.tenant_id = $1
                  AND branch.status = 'active'
                  AND (
                      branch_rule.max_assignments = 0
                      OR assignment.branch_id IS NOT NULL
                  )
                ORDER BY lower(branch.name), branch.id
                "#,
                tenant_id,
                account_id,
            )
            .fetch_all(&mut *connection)
            .await?;
            Ok((account, role_rows, permission_rows, branch_rows))
        })
        .await
        .map_err(|database_error: TenantDbErr| {
            error!(
                tenant_id = %tenant_id,
                account_id = %account_id,
                reason = %database_error,
                "Application account authorization tenant operation failed"
            );
            StatusCode::SERVICE_UNAVAILABLE
        })?;
    let (account, role_rows, permission_rows, branch_rows): (
        Option<UserAccount>,
        Vec<AccountRole>,
        Vec<AccountPermission>,
        Vec<AccountBranch>,
    ) = authorization_rows;
    let account: UserAccount = account.ok_or_else(|| {
        error!(
            tenant_id = %tenant_id,
            account_id = %account_id,
            "External identity references a missing application account"
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    let account_status: AccountStatus = AccountStatus::from_code(&account.status).ok_or_else(|| {
        error!(
            tenant_id = %account.tenant_id,
            account_id = %account.id,
            account_status = %account.status,
            "Application account has an unsupported status"
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?;
    if account_status != AccountStatus::Active {
        warn!(
            tenant_id = %account.tenant_id,
            account_id = %account.id,
            account_status = %account.status,
            "Inactive application identity rejected"
        );
        return Err(StatusCode::FORBIDDEN);
    }
    debug!(
        operation = "load_application_account",
        tenant_id = %account.tenant_id,
        account_id = %account.id,
        primary_role = %account.primary_role_code,
        "Application account is active"
    );

    debug!(
        operation = "load_application_account",
        tenant_id = %account.tenant_id,
        account_id = %account.id,
        role_row_count = role_rows.len(),
        permission_grant_row_count = permission_rows.len(),
        "Loaded application role and permission grants"
    );
    let primary_role: RoleCode = RoleCode::try_from(account.primary_role_code).map_err(|code_error| {
        error!(
            tenant_id = %account.tenant_id,
            account_id = %account.id,
            reason = %code_error,
            "Application account primary role code is invalid"
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?;
    let roles: Vec<RoleCode> = role_rows
        .into_iter()
        .map(|row: AccountRole| RoleCode::try_from(row.role_code))
        .collect::<Result<Vec<RoleCode>, _>>()
        .map_err(|code_error| {
            error!(
                tenant_id = %account.tenant_id,
                account_id = %account.id,
                reason = %code_error,
                "Application account role code is invalid"
            );
            StatusCode::SERVICE_UNAVAILABLE
        })?;
    let mut permission_set: BTreeSet<PermissionCode> = BTreeSet::new();
    for row in permission_rows {
        let permission_code: PermissionCode = PermissionCode::try_from(row.permission_code).map_err(|code_error| {
            error!(
                tenant_id = %account.tenant_id,
                account_id = %account.id,
                reason = %code_error,
                "Application account permission code is invalid"
            );
            StatusCode::SERVICE_UNAVAILABLE
        })?;
        let permission_effect: PermissionEffect = PermissionEffect::from_code(&row.effect).ok_or_else(|| {
            error!(
                tenant_id = %account.tenant_id,
                account_id = %account.id,
                permission_effect = %row.effect,
                "Application account permission grant has an unsupported effect"
            );
            StatusCode::SERVICE_UNAVAILABLE
        })?;
        if permission_effect == PermissionEffect::Deny {
            permission_set.remove(&permission_code);
        } else {
            permission_set.insert(permission_code);
        }
    }
    let permissions: Vec<PermissionCode> = permission_set.into_iter().collect();
    let branch_ids: Vec<Uuid> = branch_rows
        .into_iter()
        .map(|row: AccountBranch| row.branch_id)
        .collect();
    debug!(
        operation = "load_application_account",
        tenant_id = %account.tenant_id,
        account_id = %account.id,
        role_count = roles.len(),
        permission_count = permissions.len(),
        accessible_branch_count = branch_ids.len(),
        "Resolved effective application authorization"
    );

    Ok(AuthenticatedUser {
        tenant_id: account.tenant_id,
        account_id: account.id,
        username: account.username,
        email: account.email,
        primary_role,
        roles,
        permissions,
        branch_ids,
        active_branch_id: None,
    })
}
