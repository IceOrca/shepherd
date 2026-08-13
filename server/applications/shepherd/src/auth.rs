use std::{collections::BTreeSet, sync::Arc};

use axum::{
    Extension, Json,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
    routing::get,
    Router,
};
use infra_auth::keycloak::KeycloakPrincipal;
use infra_kernel::{debug::*, request::PrincipalRateLimitKey};
use infra_postgres::DatabaseAdapter;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use crate::AppContext;

#[derive(Clone, Debug)]
pub struct AuthenticatedUser {
    pub tenant_id: Uuid,
    pub account_id: Uuid,
    pub username: String,
    pub primary_role: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}

impl AuthenticatedUser {
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|current| current == permission)
    }

    fn profile(&self) -> CurrentUserProfile {
        CurrentUserProfile {
            tenant_id: self.tenant_id,
            account_id: self.account_id,
            username: self.username.clone(),
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
    pub primary_role: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}

pub fn routes() -> Router<Arc<AppContext>> {
    Router::new().route("/me", get(current_user))
}

async fn current_user(Extension(user): Extension<AuthenticatedUser>) -> Json<CurrentUserProfile> {
    Json(user.profile())
}

pub async fn resolve_application_account(
    State(context): State<Arc<AppContext>>,
    Extension(principal): Extension<KeycloakPrincipal>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let user = load_account(&context.database, &principal).await?;
    request.extensions_mut().insert(PrincipalRateLimitKey::new(format!(
        "{}:{}",
        user.tenant_id, user.account_id
    )));
    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}

async fn load_account(
    database: &DatabaseAdapter,
    principal: &KeycloakPrincipal,
) -> Result<AuthenticatedUser, StatusCode> {
    let identity = sqlx::query!(
        r#"
        SELECT tenant_id, account_id
        FROM account_identities
        WHERE issuer = $1 AND subject = $2
        "#,
        principal.issuer,
        principal.subject,
    )
    .fetch_optional(database.client().pool())
    .await
    .map_err(|error| {
        log_error!(
            "Identity lookup failed: issuer={} subject={} error={}",
            principal.issuer,
            principal.subject,
            error
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?
    .ok_or_else(|| {
        log_notice!(
            "Authenticated Keycloak identity has no Shepherd account: issuer={} subject={}",
            principal.issuer,
            principal.subject
        );
        StatusCode::FORBIDDEN
    })?;

    let mut transaction = database.begin_tenant(identity.tenant_id).await.map_err(|error| {
        log_error!(
            "Identity tenant transaction failed: tenant_id={} error={}",
            identity.tenant_id,
            error
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?;
    let account = sqlx::query!(
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
    .map_err(|error| {
        log_error!(
            "Application account lookup failed: tenant_id={} account_id={} error={}",
            identity.tenant_id,
            identity.account_id,
            error
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?
    .ok_or_else(|| {
        log_error!(
            "OIDC identity references a missing account: tenant_id={} account_id={}",
            identity.tenant_id,
            identity.account_id
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    if account.status != "active" {
        log_notice!(
            "Inactive Shepherd identity rejected: tenant_id={} account_id={} account_status={}",
            account.tenant_id,
            account.id,
            account.status
        );
        return Err(StatusCode::FORBIDDEN);
    }

    let role_rows = sqlx::query!(
        "SELECT role_code FROM account_roles WHERE tenant_id = $1 AND account_id = $2 ORDER BY role_code",
        account.tenant_id,
        account.id,
    )
    .fetch_all(transaction.connection())
    .await
    .map_err(|error| {
        log_error!(
            "Account role lookup failed: tenant_id={} account_id={} error={}",
            account.tenant_id,
            account.id,
            error
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?;
    let permission_rows = sqlx::query!(
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
    .map_err(|error| {
        log_error!(
            "Account permission lookup failed: tenant_id={} account_id={} error={}",
            account.tenant_id,
            account.id,
            error
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?;
    transaction.commit().await.map_err(|error| {
        log_error!(
            "Identity lookup commit failed: tenant_id={} account_id={} error={}",
            account.tenant_id,
            account.id,
            error
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    let roles = role_rows.into_iter().map(|row| row.role_code).collect();
    let mut permissions = BTreeSet::new();
    for row in permission_rows {
        if row.effect == "deny" {
            permissions.remove(&row.permission_code);
        } else {
            permissions.insert(row.permission_code);
        }
    }

    Ok(AuthenticatedUser {
        tenant_id: account.tenant_id,
        account_id: account.id,
        username: account.username,
        primary_role: account.primary_role_code,
        roles,
        permissions: permissions.into_iter().collect(),
    })
}
