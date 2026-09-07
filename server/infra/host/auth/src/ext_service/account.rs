use std::{collections::BTreeSet, sync::Arc};

use axum::{
    Extension, Json, Router,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
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
use crate::ext_service::middleware::require_authenticated;

use crate::{
    AuthCodeError, AuthService, PermissionCode, RoleCode,
    ext_service::{AuthedPrincipal, account_cache::AuthedCacheErr},
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
pub struct AuthedUser {
    pub tenant_id: Uuid,
    pub account_id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub primary_role: RoleCode,
    pub roles: Vec<RoleCode>,
    pub permissions: Vec<PermissionCode>,
    pub branch_ids: Vec<Uuid>,
    pub active_branch_id: Option<Uuid>,
    #[serde(default)]
    pub authz_roles: Vec<ScopedRoleGrant>,
    #[serde(default)]
    pub authz_perms: Vec<ScopedPermissionGrant>,
}

impl AuthedUser {
    pub fn has_permission(&self, perm: &str) -> bool {
        self.permissions.iter().any(|p: &PermissionCode| p.as_str() == perm)
    }

    /// Evaluates the effective permission set for an explicitly selected branch
    /// without changing the request's active write branch. Tenant-wide and
    /// matching branch grants apply, and any applicable deny wins.
    pub fn has_permission_for_branch(&self, branch_id: Uuid, perm: &str) -> bool {
        if !self.branch_ids.contains(&branch_id) {
            return false;
        }
        let mut allowed: bool = false;
        for grant in &self.authz_perms {
            if grant.permission_code.as_str() != perm
                || (grant.branch_id.is_some() && grant.branch_id != Some(branch_id))
            {
                continue;
            }
            match grant.effect {
                PermissionEffect::Allow => allowed = true,
                PermissionEffect::Deny => return false,
            }
        }
        allowed
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

    fn activate_branch(&mut self, branch_id: Uuid) -> Result<(), StatusCode> {
        if !self.branch_ids.contains(&branch_id) {
            warn!(
                operation = "activate_authenticated_branch",
                tenant_id = %self.tenant_id,
                account_id = %self.account_id,
                branch_id = %branch_id,
                "Cannot activate a branch outside the account authorization scope"
            );
            return Err(StatusCode::FORBIDDEN);
        }

        let mut role_set: BTreeSet<RoleCode> = BTreeSet::new();
        for role_grant in &self.authz_roles {
            if role_grant.branch_id.is_none() || role_grant.branch_id == Some(branch_id) {
                role_set.insert(role_grant.role_code.clone());
            }
        }

        let mut allowed_permissions: BTreeSet<PermissionCode> = BTreeSet::new();
        let mut denied_permissions: BTreeSet<PermissionCode> = BTreeSet::new();
        for permission_grant in &self.authz_perms {
            if permission_grant.branch_id.is_some() && permission_grant.branch_id != Some(branch_id) {
                continue;
            }
            match permission_grant.effect {
                PermissionEffect::Allow => {
                    allowed_permissions.insert(permission_grant.permission_code.clone());
                }
                PermissionEffect::Deny => {
                    denied_permissions.insert(permission_grant.permission_code.clone());
                }
            }
        }
        allowed_permissions.retain(|permission_code: &PermissionCode| !denied_permissions.contains(permission_code));

        self.roles = role_set.into_iter().collect();
        self.permissions = allowed_permissions.into_iter().collect();
        self.active_branch_id = Some(branch_id);
        debug!(
            operation = "activate_authenticated_branch",
            tenant_id = %self.tenant_id,
            account_id = %self.account_id,
            branch_id = %branch_id,
            role_count = self.roles.len(),
            permission_count = self.permissions.len(),
            "Resolved branch-specific effective roles and permissions"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ScopedRoleGrant {
    pub branch_id: Option<Uuid>,
    pub role_code: RoleCode,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ScopedPermissionGrant {
    pub branch_id: Option<Uuid>,
    pub permission_code: PermissionCode,
    effect: PermissionEffect,
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

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct TenantMembershipSummary {
    #[ts(type = "string")]
    pub tenant_id: Uuid,
    #[ts(type = "string")]
    pub account_id: Uuid,
    pub tenant_slug: String,
    pub tenant_display_name: String,
    pub username: String,
    pub email: Option<String>,
    pub primary_role: RoleCode,
}

pub fn routes(auth: Arc<AuthService>) -> Router {
    info!("Configured external authentication account routes");
    Router::new().route("/me", get(current_user)).with_state(auth)
}

pub fn identity_routes(auth: Arc<AuthService>) -> Router {
    info!("Configured external identity tenant-membership routes");
    Router::new()
        .route("/tenants", get(list_tenant_memberships))
        .with_state(auth)
}

async fn current_user(Extension(user): Extension<AuthedUser>) -> Json<CurrentUserProfile> {
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

struct TenantMembershipRow {
    pub tenant_id: Uuid,
    pub account_id: Uuid,
}

struct ActiveTenantMembershipRow {
    pub tenant_id: Uuid,
    pub account_id: Uuid,
    pub tenant_slug: String,
    pub tenant_display_name: String,
    pub username: String,
    pub email: Option<String>,
    pub primary_role_code: String,
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
    pub branch_id: Option<Uuid>,
    pub role_code: String,
}

struct AccountPermission {
    pub branch_id: Option<Uuid>,
    pub permission_code: String,
    pub effect: String,
}

struct AccountBranch {
    pub branch_id: Uuid,
}

const ACTIVE_BRANCH_HEADER: &str = "x-branch-id";
const ACTIVE_TENANT_HEADER: &str = "x-tenant-id";
const MAX_TENANT_MEMBERSHIPS: i64 = 1024;

async fn list_tenant_memberships(
    State(ctx): State<Arc<AuthService>>,
    Extension(principal): Extension<AuthedPrincipal>,
) -> Result<Json<Vec<TenantMembershipSummary>>, StatusCode> {
    let memberships: Vec<TenantMembershipSummary> = load_tenant_memberships(&ctx.db, &principal).await?;
    info!(
        operation = "list_tenant_memberships",
        issuer = %principal.issuer,
        subject = %principal.subject,
        membership_count = memberships.len(),
        "Returned active tenant memberships for authenticated identity"
    );
    Ok(Json(memberships))
}

pub async fn resolve_app_acct(
    State(ctx): State<Arc<AuthService>>,
    Extension(principal): Extension<AuthedPrincipal>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let method: String = request.method().as_str().to_owned();
    let path: String = request.uri().path().to_owned();
    let issuer: String = principal.issuer.clone();
    let subject: String = principal.subject.clone();
    trace!(
        operation = "resolve_app_acct",
        method = %method,
        path = %path,
        issuer = %issuer,
        subject = %subject,
        "Resolving application account for authenticated external identity"
    );
    let selected_tid: Uuid = resolve_active_tenant(&ctx.db, request.headers(), &principal).await?;
    if principal
        .tenant_id
        .is_some_and(|claimed_tid: Uuid| claimed_tid != selected_tid)
    {
        info!(
            operation = "resolve_app_acct",
            claimed_tid = ?principal.tenant_id,
            selected_tid = %selected_tid,
            "Explicit validated tenant selection differs from the signed default tenant hint"
        );
    }

    let cache_result: Result<Option<AuthedUser>, AuthedCacheErr> = ctx.acct_cache.get(&principal, selected_tid).await;
    let cached_user: Option<AuthedUser> = match cache_result {
        Ok(user) => user,
        Err(cache_error) => {
            warn!(
                operation = "resolve_app_acct",
                issuer = %issuer,
                subject = %subject,
                reason = %cache_error,
                "Authed-user cache unavailable; resolving from PostgreSQL"
            );
            None
        }
    };

    let mut user: AuthedUser = match cached_user {
        Some(cached_user) if cached_user.tenant_id == selected_tid => {
            trace!(
                operation = "resolve_app_acct",
                tenant_id = %cached_user.tenant_id,
                account_id = %cached_user.account_id,
                "Resolved selected tenant membership from authenticated-user cache"
            );
            cached_user
        }
        cached_user => {
            if let Some(cached_user) = cached_user {
                warn!(
                    operation = "resolve_app_acct",
                    selected_tid = %selected_tid,
                    cached_tenant_id = %cached_user.tenant_id,
                    account_id = %cached_user.account_id,
                    "Cached account tenant does not match selection; reloading PostgreSQL authority"
                );
            } else {
                debug!(
                    operation = "resolve_app_acct",
                    selected_tid = %selected_tid,
                    "Authed-user cache missed; loading PostgreSQL authority"
                );
            }
            let loaded_user: AuthedUser = load_app_acct(&ctx.db, &principal, selected_tid).await?;
            let cache_write_result: Result<(), AuthedCacheErr> = ctx.acct_cache.put(&principal, &loaded_user).await;
            if let Err(cache_error) = cache_write_result {
                warn!(
                    operation = "resolve_app_acct",
                    tenant_id = %loaded_user.tenant_id,
                    account_id = %loaded_user.account_id,
                    reason = %cache_error,
                    "Authed-user cache write failed; request will continue"
                );
            }
            loaded_user
        }
    };

    let active_branch_id: Uuid = resolve_active_branch(request.headers(), &user)?;
    user.activate_branch(active_branch_id)?;
    let tenant_id: Uuid = user.tenant_id;
    let account_id: Uuid = user.account_id;
    debug!(
        operation = "resolve_app_acct",
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
        operation = "resolve_app_acct",
        method = %method,
        path = %path,
        tenant_id = %tenant_id,
        account_id = %account_id,
        status = response.status().as_u16(),
        "Protected request completed after application account resolution"
    );
    Ok(response)
}

async fn resolve_active_tenant(
    db: &DatabaseAdapter,
    headers: &HeaderMap,
    principal: &AuthedPrincipal,
) -> Result<Uuid, StatusCode> {
    let requested_tenant_id: Option<Uuid> = headers
        .get(ACTIVE_TENANT_HEADER)
        .map(|value: &HeaderValue| value.to_str().map_err(|_| StatusCode::BAD_REQUEST))
        .transpose()?
        .map(|value: &str| Uuid::parse_str(value).map_err(|_| StatusCode::BAD_REQUEST))
        .transpose()?;
    if let Some(tenant_id) = requested_tenant_id {
        debug!(
            operation = "resolve_active_tenant",
            tenant_id = %tenant_id,
            source = "request_header",
            "Accepted explicit tenant selection for membership validation"
        );
        return Ok(tenant_id);
    }
    if let Some(tenant_id) = principal.tenant_id {
        debug!(
            operation = "resolve_active_tenant",
            tenant_id = %tenant_id,
            source = "signed_jwt_hint",
            "Using signed tenant hint because no explicit selection was provided"
        );
        return Ok(tenant_id);
    }

    let memberships: Vec<TenantMembershipSummary> = load_tenant_memberships(db, principal).await?;
    match memberships.as_slice() {
        [membership] => {
            debug!(
                operation = "resolve_active_tenant",
                tenant_id = %membership.tenant_id,
                source = "single_active_membership",
                "Selected the identity's only active tenant membership"
            );
            Ok(membership.tenant_id)
        }
        [] => {
            warn!(
                operation = "resolve_active_tenant",
                issuer = %principal.issuer,
                subject = %principal.subject,
                "Authed identity has no active application tenant membership"
            );
            Err(StatusCode::FORBIDDEN)
        }
        memberships => {
            warn!(
                operation = "resolve_active_tenant",
                issuer = %principal.issuer,
                subject = %principal.subject,
                membership_count = memberships.len(),
                "Multi-tenant identity omitted the required active tenant selection"
            );
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

fn resolve_active_branch(headers: &HeaderMap, user: &AuthedUser) -> Result<Uuid, StatusCode> {
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

async fn load_tenant_memberships(
    db: &DatabaseAdapter,
    principal: &AuthedPrincipal,
) -> Result<Vec<TenantMembershipSummary>, StatusCode> {
    let identity_rows: Vec<TenantMembershipRow> = sqlx::query_as!(
        TenantMembershipRow,
        r#"
        SELECT identity.tenant_id, identity.account_id
        FROM account_identities AS identity
        INNER JOIN tenants AS tenant
            ON tenant.id = identity.tenant_id
           AND tenant.status = 'active'
        WHERE identity.issuer = $1 AND identity.subject = $2
        ORDER BY identity.tenant_id
        LIMIT $3
        "#,
        principal.issuer,
        principal.subject,
        MAX_TENANT_MEMBERSHIPS + 1,
    )
    .fetch_all(db.global_pool())
    .await
    .map_err(|err: sqlx::Error| {
        error!(
            operation = "load_tenant_memberships",
            issuer = %principal.issuer,
            subject = %principal.subject,
            reason = %err,
            "Tenant membership registry lookup failed"
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    if i64::try_from(identity_rows.len()).unwrap_or(i64::MAX) > MAX_TENANT_MEMBERSHIPS {
        error!(
            operation = "load_tenant_memberships",
            issuer = %principal.issuer,
            subject = %principal.subject,
            membership_count = identity_rows.len(),
            max_membership_count = MAX_TENANT_MEMBERSHIPS,
            "Authed identity exceeds the supported active tenant membership bound"
        );
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let mut memberships: Vec<TenantMembershipSummary> = Vec::with_capacity(identity_rows.len());
    for identity in identity_rows {
        let tenant_id: Uuid = identity.tenant_id;
        let account_id: Uuid = identity.account_id;
        let active_row: Option<ActiveTenantMembershipRow> = db
            .tran_with_tenant(tenant_id, async move |conn: &mut PgConnection| {
                sqlx::query_as!(
                    ActiveTenantMembershipRow,
                    r#"
                    SELECT tenant.id AS tenant_id, account.id AS account_id,
                           tenant.slug AS tenant_slug, tenant.display_name AS tenant_display_name,
                           account.username, account.email,
                           account.primary_role_code
                    FROM accounts AS account
                    INNER JOIN tenants AS tenant
                        ON account.tenant_id = tenant.id
                    WHERE tenant.id = $1
                      AND tenant.status = 'active'
                      AND account.id = $2
                      AND account.status = 'active'
                    "#,
                    tenant_id,
                    account_id,
                )
                .fetch_optional(conn)
                .await
            })
            .await
            .map_err(|err: TenantDbErr| {
                error!(
                    operation = "load_tenant_memberships",
                    tenant_id = %tenant_id,
                    account_id = %account_id,
                    reason = %err,
                    "Tenant membership account validation failed"
                );
                StatusCode::SERVICE_UNAVAILABLE
            })?;

        let Some(row) = active_row else {
            debug!(
                operation = "load_tenant_memberships",
                tenant_id = %tenant_id,
                account_id = %account_id,
                "Excluded inactive or incomplete tenant membership"
            );
            continue;
        };

        let primary_role: RoleCode =
            RoleCode::try_from(row.primary_role_code).map_err(|code_error: AuthCodeError| {
                error!(
                    operation = "load_tenant_memberships",
                    tenant_id = %row.tenant_id,
                    account_id = %row.account_id,
                    reason = %code_error,
                    "Tenant membership has an invalid primary role code"
                );
                StatusCode::SERVICE_UNAVAILABLE
            })?;

        memberships.push(TenantMembershipSummary {
            tenant_id: row.tenant_id,
            account_id: row.account_id,
            tenant_slug: row.tenant_slug,
            tenant_display_name: row.tenant_display_name,
            username: row.username,
            email: row.email,
            primary_role,
        });
    }
    memberships.sort_by(|left: &TenantMembershipSummary, right: &TenantMembershipSummary| {
        left.tenant_display_name
            .to_lowercase()
            .cmp(&right.tenant_display_name.to_lowercase())
            .then_with(|| left.tenant_id.cmp(&right.tenant_id))
    });
    debug!(
        operation = "load_tenant_memberships",
        issuer = %principal.issuer,
        subject = %principal.subject,
        active_membership_count = memberships.len(),
        "Loaded active tenant memberships"
    );
    Ok(memberships)
}

async fn load_app_acct(
    db: &DatabaseAdapter,
    principal: &AuthedPrincipal,
    selected_tid: Uuid,
) -> Result<AuthedUser, StatusCode> {
    let identity: AccountIdentity = sqlx::query_as!(
        AccountIdentity,
        r#"
        SELECT tenant_id, account_id
        FROM account_identities
        WHERE issuer = $1 AND subject = $2 AND tenant_id = $3
        "#,
        principal.issuer,
        principal.subject,
        selected_tid,
    )
    // The selected tenant has not entered RLS context yet, so validate the
    // global identity-to-membership registry before loading tenant data.
    .fetch_optional(db.global_pool())
    .await
    .map_err(|err: sqlx::Error| {
        error!(
            issuer = %principal.issuer,
            subject = %principal.subject,
            reason = %err,
            "Application account identity lookup failed"
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?
    .ok_or_else(|| {
        warn!(
            issuer = %principal.issuer,
            subject = %principal.subject,
            "Authed external identity has no active application account mapping"
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
        .tran_with_tenant(tenant_id, async move |conn: &mut PgConnection| {
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
            .fetch_optional(&mut *conn)
            .await?;

            let role_rows: Vec<AccountRole> = sqlx::query_as!(
                AccountRole,
                r#"
                SELECT assignment.branch_id, assignment.role_code
                FROM account_role_assignments AS assignment
                INNER JOIN tenant_roles AS tenant_role
                    ON tenant_role.tenant_id = assignment.tenant_id
                   AND tenant_role.code = assignment.role_code
                   AND tenant_role.is_active
                WHERE assignment.tenant_id = $1 AND assignment.account_id = $2
                ORDER BY assignment.branch_id NULLS FIRST, assignment.role_code
                "#,
                tenant_id,
                account_id,
            )
            .fetch_all(&mut *conn)
            .await?;

            let permission_rows: Vec<AccountPermission> = sqlx::query_as!(
                AccountPermission,
                r#"
                SELECT branch_id, permission_code AS "permission_code!", effect AS "effect!"
                FROM (
                    SELECT
                        assignment.branch_id,
                        role_permission.permission_code,
                        'allow'::TEXT AS effect
                    FROM account_role_assignments AS assignment
                    INNER JOIN tenant_roles AS tenant_role
                        ON tenant_role.tenant_id = assignment.tenant_id
                       AND tenant_role.code = assignment.role_code
                       AND tenant_role.is_active
                    INNER JOIN tenant_role_permissions AS role_permission
                        ON role_permission.tenant_id = assignment.tenant_id
                       AND role_permission.role_code = assignment.role_code
                    WHERE assignment.tenant_id = $1 AND assignment.account_id = $2
                    UNION ALL
                    SELECT branch_id, permission_code, effect
                    FROM account_permission_overrides
                    WHERE tenant_id = $1 AND account_id = $2
                      AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP)
                ) AS grants
                ORDER BY branch_id NULLS FIRST, permission_code, effect
                "#,
                tenant_id,
                account_id,
            )
            .fetch_all(&mut *conn)
            .await?;

            let branch_rows: Vec<AccountBranch> = sqlx::query_as!(
                AccountBranch,
                r#"
                SELECT branch.id AS branch_id
                FROM branches AS branch
                WHERE branch.tenant_id = $1
                  AND branch.status = 'active'
                  AND (
                      EXISTS (
                          SELECT 1
                          FROM account_role_assignments AS tenant_assignment
                          WHERE tenant_assignment.tenant_id = $1
                            AND tenant_assignment.account_id = $2
                            AND tenant_assignment.branch_id IS NULL
                      )
                      OR EXISTS (
                          SELECT 1
                          FROM account_role_assignments AS branch_assignment
                          WHERE branch_assignment.tenant_id = $1
                            AND branch_assignment.account_id = $2
                            AND branch_assignment.branch_id = branch.id
                      )
                  )
                ORDER BY lower(branch.name), branch.id
                "#,
                tenant_id,
                account_id,
            )
            .fetch_all(&mut *conn)
            .await?;
            Ok((account, role_rows, permission_rows, branch_rows))
        })
        .await
        .map_err(|err: TenantDbErr| {
            error!(
                tenant_id = %tenant_id,
                account_id = %account_id,
                reason = %err,
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

    // Loaded application role and permission grants
    let primary_role: RoleCode =
        RoleCode::try_from(account.primary_role_code).map_err(|code_error: AuthCodeError| {
            error!(
                tenant_id = %account.tenant_id,
                account_id = %account.id,
                reason = %code_error,
                "Application account primary role code is invalid"
            );
            StatusCode::SERVICE_UNAVAILABLE
        })?;
    let authz_roles: Vec<ScopedRoleGrant> = role_rows
        .into_iter()
        .map(|row: AccountRole| -> Result<ScopedRoleGrant, AuthCodeError> {
            Ok(ScopedRoleGrant {
                branch_id: row.branch_id,
                role_code: RoleCode::try_from(row.role_code)?,
            })
        })
        .collect::<Result<Vec<ScopedRoleGrant>, _>>()
        .map_err(|code_error: AuthCodeError| {
            error!(
                tenant_id = %account.tenant_id,
                account_id = %account.id,
                reason = %code_error,
                "Application account role code is invalid"
            );
            StatusCode::SERVICE_UNAVAILABLE
        })?;
    let mut authz_perms: Vec<ScopedPermissionGrant> = Vec::with_capacity(permission_rows.len());
    for row in permission_rows {
        let permission_code: PermissionCode =
            PermissionCode::try_from(row.permission_code).map_err(|code_error: AuthCodeError| {
                error!(
                    tenant_id = %account.tenant_id,
                    account_id = %account.id,
                    reason = %code_error,
                    "Application account permission code is invalid"
                );
                StatusCode::SERVICE_UNAVAILABLE
            })?;
        let perm_effect: PermissionEffect = PermissionEffect::from_code(&row.effect).ok_or_else(|| {
            error!(
                tenant_id = %account.tenant_id,
                account_id = %account.id,
                perm_effect = %row.effect,
                "Application account permission grant has an unsupported effect"
            );
            StatusCode::SERVICE_UNAVAILABLE
        })?;
        authz_perms.push(ScopedPermissionGrant {
            branch_id: row.branch_id,
            permission_code,
            effect: perm_effect,
        });
    }

    let roles: Vec<RoleCode> = Vec::new();
    let permissions: Vec<PermissionCode> = Vec::new();
    let branch_ids: Vec<Uuid> = branch_rows
        .into_iter()
        .map(|row: AccountBranch| row.branch_id)
        .collect();
    debug!(
        operation = "load_application_account",
        tenant_id = %account.tenant_id,
        account_id = %account.id,
        scoped_role_count = authz_roles.len(),
        scoped_permission_count = authz_perms.len(),
        accessible_branch_count = branch_ids.len(),
        "Resolved effective application authorization"
    );

    Ok(AuthedUser {
        tenant_id: account.tenant_id,
        account_id: account.id,
        username: account.username,
        email: account.email,
        primary_role,
        roles,
        permissions,
        branch_ids,
        active_branch_id: None,
        authz_roles,
        authz_perms,
    })
}

pub fn protected_layer(auth: Arc<AuthService>, routes: Router) -> Router {
    routes
        .route_layer(axum::middleware::from_fn_with_state(
            Arc::clone(&auth),
            resolve_app_acct,
        ))
        .route_layer(axum::middleware::from_fn_with_state(
            Arc::clone(&auth),
            require_authenticated,
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn role(code: &str) -> RoleCode {
        RoleCode::try_from(code).expect("test role code must be valid")
    }

    fn permission(code: &str) -> PermissionCode {
        PermissionCode::try_from(code).expect("test permission code must be valid")
    }

    fn scoped_user(first_branch_id: Uuid, second_branch_id: Uuid) -> AuthedUser {
        AuthedUser {
            tenant_id: Uuid::from_u128(1),
            account_id: Uuid::from_u128(2),
            username: "scoped-user".to_owned(),
            email: Some("scoped-user@example.test".to_owned()),
            primary_role: role("member"),
            roles: Vec::new(),
            permissions: Vec::new(),
            branch_ids: vec![first_branch_id, second_branch_id],
            active_branch_id: None,
            authz_roles: vec![
                ScopedRoleGrant {
                    branch_id: None,
                    role_code: role("tenant_auditor"),
                },
                ScopedRoleGrant {
                    branch_id: Some(first_branch_id),
                    role_code: role("first_branch_dispatcher"),
                },
                ScopedRoleGrant {
                    branch_id: Some(second_branch_id),
                    role_code: role("second_branch_dispatcher"),
                },
            ],
            authz_perms: vec![
                ScopedPermissionGrant {
                    branch_id: None,
                    permission_code: permission("business.shared.read"),
                    effect: PermissionEffect::Allow,
                },
                ScopedPermissionGrant {
                    branch_id: Some(first_branch_id),
                    permission_code: permission("business.first.manage"),
                    effect: PermissionEffect::Allow,
                },
                ScopedPermissionGrant {
                    branch_id: Some(second_branch_id),
                    permission_code: permission("business.second.manage"),
                    effect: PermissionEffect::Allow,
                },
            ],
        }
    }

    #[test]
    fn active_branch_excludes_other_branch_roles_and_permissions() {
        let first_branch_id: Uuid = Uuid::from_u128(10);
        let second_branch_id: Uuid = Uuid::from_u128(11);
        let mut user: AuthedUser = scoped_user(first_branch_id, second_branch_id);

        user.activate_branch(first_branch_id)
            .expect("authorized branch must activate");

        assert_eq!(user.active_branch_id, Some(first_branch_id));
        assert!(user.roles.contains(&role("tenant_auditor")));
        assert!(user.roles.contains(&role("first_branch_dispatcher")));
        assert!(!user.roles.contains(&role("second_branch_dispatcher")));
        assert!(user.has_permission("business.shared.read"));
        assert!(user.has_permission("business.first.manage"));
        assert!(!user.has_permission("business.second.manage"));
    }

    #[test]
    fn applicable_deny_override_wins_without_leaking_to_another_branch() {
        let first_branch_id: Uuid = Uuid::from_u128(20);
        let second_branch_id: Uuid = Uuid::from_u128(21);
        let mut user: AuthedUser = scoped_user(first_branch_id, second_branch_id);
        user.authz_perms.push(ScopedPermissionGrant {
            branch_id: Some(first_branch_id),
            permission_code: permission("business.shared.read"),
            effect: PermissionEffect::Deny,
        });

        user.activate_branch(first_branch_id)
            .expect("authorized branch must activate");
        assert!(!user.has_permission("business.shared.read"));

        user.activate_branch(second_branch_id)
            .expect("authorized branch must activate");
        assert!(user.has_permission("business.shared.read"));
    }
}
