use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::{Extension, Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{Algorithm, Header, TokenData, Validation, decode, decode_header};
use tracing::{error, warn, info, debug, trace};
use infra_kernel::request::PrincipalRateLimitKey;
use crate::account::Role;
#[cfg(feature = "password-auth")]
use crate::account::UserAccount;

use super::{AuthenticatedUser, LegacyAuthService, TenantContext, dto::AccessClaims, jwt::KID_MAIN};

fn build_jwt_validation() -> Validation {
    let mut validation = Validation::new(Algorithm::EdDSA);
    validation.set_issuer(&["infra"]);
    validation.set_audience(&["infra-api"]);
    validation.validate_nbf = true;
    validation.validate_exp = true;
    validation.validate_aud = true;
    validation.leeway = 0;
    validation
}

pub async fn require_authenticated(
    State(auth_ctx): State<Arc<LegacyAuthService>>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let authorization: &str = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let token: &str = authorization
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token: &&str| !token.is_empty())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let header: Header = decode_header(token).map_err(|error| {
        info!("JWT header invalid: {}", error);
        StatusCode::UNAUTHORIZED
    })?;
    if header.alg != Algorithm::EdDSA || header.kid.as_deref() != Some(KID_MAIN!()) {
        info!(
            "JWT rejected because signing metadata is unexpected: algorithm={:?} kid={:?}",
            header.alg, header.kid
        );
        return Err(StatusCode::UNAUTHORIZED);
    }
    let validation = build_jwt_validation();
    let token_data: TokenData<AccessClaims> = decode(token, auth_ctx.jwt.decoding(), &validation).map_err(|error| {
        info!("JWT invalid: {}", error);
        StatusCode::UNAUTHORIZED
    })?;

    let now: usize = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            error!("System time error: {}", error);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .as_secs() as usize;
    if token_data.claims.iat > now.saturating_add(60) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    #[cfg(feature = "session-revocation")]
    if auth_ctx.access_revocation.is_revoked(&token_data.claims.jti).await {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let user: AuthenticatedUser = AuthenticatedUser::from_claims(&token_data.claims).map_err(|_| {
        info!("JWT contains an invalid tenant or account UUID");
        StatusCode::UNAUTHORIZED
    })?;
    #[cfg(feature = "password-auth")]
    let current_account: UserAccount = auth_ctx
        .core_entity
        .get_current_account_by_username(user.tenant_id, &user.username)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .filter(|account: &UserAccount| {
            account.id == user.account_id
                && account.active
                && account.auth_version == user.auth_version
                && account.role == user.role
                && account.roles == user.roles
                && account.permissions == user.permissions
        })
        .ok_or_else(|| {
            info!(
                "JWT rejected because current account authorization changed: tenant_id={} account_id={} token_auth_version={}",
                user.tenant_id,
                user.account_id,
                user.auth_version
            );
            StatusCode::UNAUTHORIZED
        })?;
    #[cfg(feature = "password-auth")]
    trace!(
        "Current account authorization confirmed: tenant_id={} account_id={} auth_version={}",
        user.tenant_id, current_account.id, current_account.auth_version
    );
    let tenant = TenantContext { id: user.tenant_id };
    trace!(
        "JWT accepted: tenant_id={} account_id={} jti={}",
        user.tenant_id, user.account_id, user.jti
    );
    request.extensions_mut().insert(tenant);
    request.extensions_mut().insert(PrincipalRateLimitKey::new(format!(
        "{}:{}",
        user.tenant_id, user.account_id
    )));
    request.extensions_mut().insert(user);
    Ok(next.run(request).await)
}

pub async fn require_account_creator(
    Extension(user): Extension<AuthenticatedUser>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if user.has_permission("auth.accounts.create") {
        Ok(next.run(request).await)
    } else {
        info!(
            "Account creation denied: tenant_id={} account_id={} username={}",
            user.tenant_id, user.account_id, user.username
        );
        Err(StatusCode::FORBIDDEN)
    }
}

pub async fn require_tenant_owner(
    Extension(user): Extension<AuthenticatedUser>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if matches!(user.role, Role::Owner | Role::Director) {
        Ok(next.run(request).await)
    } else {
        info!(
            "Tenant-owner route denied: tenant_id={} account_id={} username={}",
            user.tenant_id, user.account_id, user.username
        );
        Err(StatusCode::FORBIDDEN)
    }
}

#[cfg(test)]
mod tests {
    use super::build_jwt_validation;

    #[test]
    fn rejects_access_tokens_at_signed_expiry_without_leeway() {
        let validation = build_jwt_validation();

        assert!(validation.validate_exp);
        assert_eq!(validation.leeway, 0);
    }
}
