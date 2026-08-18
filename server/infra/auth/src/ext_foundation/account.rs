use std::{collections::BTreeSet, sync::Arc};

use axum::{
    Extension, Json, Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
    routing::get,
};
use infra_kernel::request::PrincipalRateLimitKey;
use infra_postgres::{DatabaseAdapter, TenantDbErr, TenantTransaction};
use serde::Serialize;
use tracing::{debug, error, info, trace, warn};
use ts_rs::TS;
use uuid::Uuid;

use crate::{AuthService, ext_foundation::AuthenticatedPrincipal};

#[derive(Clone, Debug)]
pub struct AuthenticatedUser {
    pub tenant_id: Uuid,
    pub account_id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub primary_role: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}

impl AuthenticatedUser {
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|current: &String| current == permission)
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
    pub primary_role: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
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
    pub primary_role_code: String,
}

struct AccountRole {
    pub role_code: String,
}

struct AccountPermission {
    pub permission_code: String,
    pub effect: String,
}

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
    let user: AuthenticatedUser = load_account(&ctx.db, &principal).await?;
    let tenant_id: Uuid = user.tenant_id;
    let account_id: Uuid = user.account_id;
    debug!(
        operation = "resolve_application_account",
        tenant_id = %tenant_id,
        account_id = %account_id,
        role_count = user.roles.len(),
        permission_count = user.permissions.len(),
        "Resolved active application account for external identity"
    );
    request
        .extensions_mut()
        .insert(PrincipalRateLimitKey::new(format!("{tenant_id}:{account_id}")));
    request.extensions_mut().insert(user);
    let response: Response = next.run(request).await;
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

    let mut transaction: TenantTransaction =
        db.begin_tenant(identity.tenant_id)
            .await
            .map_err(|database_error: TenantDbErr| {
                error!(
                    tenant_id = %identity.tenant_id,
                    account_id = %identity.account_id,
                    reason = %database_error,
                    "Application account authorization transaction could not be opened"
                );
                StatusCode::SERVICE_UNAVAILABLE
            })?;
    trace!(
        operation = "load_application_account",
        tenant_id = %identity.tenant_id,
        account_id = %identity.account_id,
        "Opened tenant-scoped transaction for application account resolution"
    );
    let account: UserAccount = sqlx::query_as!(
        UserAccount,
        r#"
        SELECT id, tenant_id, username, status, primary_role_code
        FROM accounts
        WHERE tenant_id = $1 AND id = $2
        "#,
        identity.tenant_id,
        identity.account_id,
    )
    .fetch_optional(transaction.connection())
    .await
    .map_err(|database_error: sqlx::Error| {
        error!(
            tenant_id = %identity.tenant_id,
            account_id = %identity.account_id,
            reason = %database_error,
            "Application account lookup failed"
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?
    .ok_or_else(|| {
        error!(
            tenant_id = %identity.tenant_id,
            account_id = %identity.account_id,
            "External identity references a missing application account"
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    if account.status != "active" {
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

    let role_rows: Vec<AccountRole> = sqlx::query_as!(
        AccountRole,
        r#"
        SELECT role_code
        FROM account_roles
        WHERE tenant_id = $1 AND account_id = $2
        ORDER BY role_code
        "#,
        account.tenant_id,
        account.id,
    )
    .fetch_all(transaction.connection())
    .await
    .map_err(|database_error: sqlx::Error| {
        error!(
            tenant_id = %account.tenant_id,
            account_id = %account.id,
            reason = %database_error,
            "Application account role lookup failed"
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?;
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
        account.tenant_id,
        account.id,
    )
    .fetch_all(transaction.connection())
    .await
    .map_err(|database_error: sqlx::Error| {
        error!(
            tenant_id = %account.tenant_id,
            account_id = %account.id,
            reason = %database_error,
            "Application account permission lookup failed"
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?;
    debug!(
        operation = "load_application_account",
        tenant_id = %account.tenant_id,
        account_id = %account.id,
        role_row_count = role_rows.len(),
        permission_grant_row_count = permission_rows.len(),
        "Loaded application role and permission grants"
    );
    transaction.commit().await.map_err(|database_error: sqlx::Error| {
        error!(
            tenant_id = %account.tenant_id,
            account_id = %account.id,
            reason = %database_error,
            "Application account authorization transaction commit failed"
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?;
    trace!(
        operation = "load_application_account",
        tenant_id = %account.tenant_id,
        account_id = %account.id,
        "Committed application account authorization lookup"
    );

    let roles: Vec<String> = role_rows.into_iter().map(|row: AccountRole| row.role_code).collect();
    let mut permission_set: BTreeSet<String> = BTreeSet::new();
    for row in permission_rows {
        if row.effect == "deny" {
            permission_set.remove(&row.permission_code);
        } else {
            permission_set.insert(row.permission_code);
        }
    }
    let permissions: Vec<String> = permission_set.into_iter().collect();
    debug!(
        operation = "load_application_account",
        tenant_id = %account.tenant_id,
        account_id = %account.id,
        role_count = roles.len(),
        permission_count = permissions.len(),
        "Resolved effective application authorization"
    );

    Ok(AuthenticatedUser {
        tenant_id: account.tenant_id,
        account_id: account.id,
        username: account.username,
        email: principal.email.clone(),
        primary_role: account.primary_role_code,
        roles,
        permissions,
    })
}
