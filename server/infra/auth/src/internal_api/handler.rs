use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json,
    extract::{Extension, Path, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CACHE_CONTROL, PRAGMA, SET_COOKIE},
    },
    response::{IntoResponse, Response},
};
use jsonwebtoken::{Algorithm, Header, encode};
use tracing::{error, warn, info, debug, trace};
use crate::{
    AccountMutationError, AuthenticateUserError, ChangeOwnPasswordError, CreateAccountError,
    account::{AccountSummary, AuthorizationCatalog, UserAccount},
};
use uuid::Uuid;
use validator::Validate;

use super::{
    LegacyAuthService, AuthenticatedUser,
    access_revocation::AccessRevocationCache,
    bruteforce::{BruteForceReason, BruteForceStatus, LoginAttemptContext, tenant_login_key},
    dto::{
        AccessClaims, AuthProfileResponse, AuthRequest, AuthResponse, InvalidCredentialsResponse, MessageResponse,
        ChangePasswordRequest, RegisterUserRequest, ResetPasswordRequest, UpdateAccountPermissionsRequest,
        UpdateAccountRolesRequest, UpdateAccountStatusRequest,
    },
    jwt::KID_MAIN,
    session::{
        AuthSessionError, CreatedSessionInfo, RefreshSessionCookie, RevokedAccessTokenInfo, RevokedSessionInfo,
        RotatedSessionInfo, ValidatedSessionInfo, extract_refresh_session_cookie, make_refresh_session_cookie,
    },
};

pub async fn login(
    State(ctx): State<Arc<LegacyAuthService>>,
    Extension(payload): Extension<AuthRequest>,
    Extension(login_attempt): Extension<LoginAttemptContext>,
) -> Result<Response, StatusCode> {
    payload.validate().map_err(|_| StatusCode::BAD_REQUEST)?;
    if !payload.username_is_valid() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let tenant: String = payload.normalized_tenant().ok_or(StatusCode::BAD_REQUEST)?;
    let login_key: String = tenant_login_key(&tenant, &payload.username);
    info!(
        "Login credential verification started: tenant={} username={} source_ip={:?}",
        tenant,
        payload.username.trim(),
        login_attempt.ip
    );

    match ctx
        .core_entity
        .authenticate_user_for_tenant(&tenant, payload.username.trim(), &payload.passphrase)
        .await
    {
        Ok((tenant_id, account)) => {
            debug!(
                "Login credentials accepted: tenant={} tenant_id={} account_id={} username={} role={}",
                tenant,
                tenant_id,
                account.id,
                account.username,
                account.role.as_code()
            );
            ctx.brute_force
                .record_success_with_ip(&login_key, login_attempt.ip.as_deref())
                .await;
            issue_login_response(&ctx, tenant_id, account).await
        }
        Err(AuthenticateUserError::BackendUnavailable) => Err(StatusCode::SERVICE_UNAVAILABLE),
        Err(AuthenticateUserError::InvalidCredentials(role)) => {
            info!(
                "Login credentials rejected: tenant={} username={} source_ip={:?}",
                tenant,
                payload.username.trim(),
                login_attempt.ip
            );
            let status: BruteForceStatus = ctx
                .brute_force
                .record_failure_with_ip(&login_key, login_attempt.ip.as_deref(), role.as_ref())
                .await;
            if status
                .locked
                .as_ref()
                .is_some_and(|block| matches!(block.reason, BruteForceReason::BackendUnavailable))
            {
                return Err(StatusCode::SERVICE_UNAVAILABLE);
            }
            Ok((
                StatusCode::UNAUTHORIZED,
                Json(InvalidCredentialsResponse {
                    error: "invalid_credentials".to_owned(),
                    remaining_attempts: status.remaining_attempts,
                }),
            )
                .into_response())
        }
    }
}

async fn issue_login_response(
    ctx: &Arc<LegacyAuthService>,
    tenant_id: Uuid,
    account: UserAccount,
) -> Result<Response, StatusCode> {
    let now: usize = unix_now()?;
    let jti: String = Uuid::new_v4().to_string();
    let expiration: usize = ctx.jwt.expiration_for_role(&account.role);
    let access_expires_at: usize = now.checked_add(expiration).ok_or_else(|| {
        error!("Access-token expiry overflow during login: account_id={}", account.id);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    debug!(
        "Creating refresh session for authenticated account: tenant_id={} account_id={} role={} access_expires_at={}",
        tenant_id,
        account.id,
        account.role.as_code(),
        access_expires_at
    );
    let created: CreatedSessionInfo = ctx
        .sessions
        .create_session(
            tenant_id,
            account.id,
            &account.username,
            account.role.clone(),
            account.auth_version,
            &jti,
            access_expires_at as u64,
        )
        .await
        .map_err(session_backend_status)?;
    for revoked in &created.kicked_access_tokens {
        revoke_access_token(ctx, revoked).await;
    }

    let claims = access_claims(tenant_id, &account, now, access_expires_at, jti, created.sid.clone());
    let access_token: String = match encode_access_token(ctx, &claims) {
        Ok(token) => token,
        Err(status) => {
            let _result = ctx.sessions.revoke_session(tenant_id, account.id, &created.sid).await;
            return Err(status);
        }
    };
    let cookie: HeaderValue = match refresh_cookie_header(
        Some(tenant_id),
        &created.sid,
        &created.refresh_token,
        ctx.sessions.expiration_secs.as_secs(),
    ) {
        Ok(cookie) => cookie,
        Err(status) => {
            error!(
                "Failed to construct login refresh cookie; revoking new session: tenant_id={} account_id={} sid={}",
                tenant_id, account.id, created.sid
            );
            let _result: Result<RevokedSessionInfo, AuthSessionError> =
                ctx.sessions.revoke_session(tenant_id, account.id, &created.sid).await;
            return Err(status);
        }
    };

    info!(
        "Login successful: tenant_id={} account_id={} username={}",
        tenant_id, account.id, account.username
    );
    Ok((
        StatusCode::OK,
        sensitive_headers(Some(cookie)),
        Json(AuthResponse {
            access_token,
            token_type: "Bearer".to_owned(),
            expires_in: expiration,
        }),
    )
        .into_response())
}

pub async fn logout(
    State(ctx): State<Arc<LegacyAuthService>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Response, StatusCode> {
    info!(
        "Logout requested: tenant_id={} account_id={} sid={} jti={}",
        user.tenant_id, user.account_id, user.sid, user.jti
    );
    // Revoke the authoritative refresh session first. If Redis is unavailable,
    // leave the access JWT usable so the client can retry instead of creating a
    // partial logout that cannot be retried with the same token.
    let revoked: RevokedSessionInfo = ctx
        .sessions
        .revoke_session(user.tenant_id, user.account_id, &user.sid)
        .await
        .map_err(session_backend_status)?;
    ctx.access_revocation.revoke_jti(&user.jti, user.exp).await;
    if let Some(token) = revoked.access_token {
        revoke_access_token(&ctx, &token).await;
    }
    info!(
        "Logout completed: tenant_id={} account_id={} sid={}",
        user.tenant_id, user.account_id, user.sid
    );
    message_with_cleared_cookie("Logged out successfully")
}

pub async fn logout_all(
    State(ctx): State<Arc<LegacyAuthService>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<Response, StatusCode> {
    info!(
        "Logout-all requested: tenant_id={} account_id={} current_sid={}",
        user.tenant_id, user.account_id, user.sid
    );
    let revoked = ctx
        .sessions
        .revoke_all_sessions(user.tenant_id, user.account_id)
        .await
        .map_err(session_backend_status)?;
    ctx.access_revocation.revoke_jti(&user.jti, user.exp).await;
    for token in &revoked.access_tokens {
        revoke_access_token(&ctx, token).await;
    }
    info!(
        "Logout-all completed: tenant_id={} account_id={} revoked_session_tokens={}",
        user.tenant_id,
        user.account_id,
        revoked.access_tokens.len()
    );
    message_with_cleared_cookie("Logged out all sessions successfully")
}

pub async fn refresh_session(State(ctx): State<Arc<LegacyAuthService>>, headers: HeaderMap) -> Response {
    let cookie: RefreshSessionCookie = match extract_refresh_session_cookie(&headers) {
        Some(cookie) => cookie,
        None => {
            info!("Refresh rejected before Redis lookup: reason=missing_or_malformed_cookie");
            return refresh_error_response(StatusCode::UNAUTHORIZED, true);
        }
    };
    debug!("Refresh requested: tenant_id={} sid={}", cookie.tenant_id, cookie.sid);
    let permit = match ctx
        .sessions
        .try_acquire_refresh_attempt_permit(cookie.tenant_id, &cookie.sid)
        .await
    {
        Some(permit) => permit,
        None => {
            info!(
                "Refresh rejected because another rotation is in flight: tenant_id={} sid={}",
                cookie.tenant_id, cookie.sid
            );
            return refresh_error_response(StatusCode::CONFLICT, false);
        }
    };

    let result = refresh_session_inner(&ctx, &cookie).await;
    ctx.sessions.release_refresh_attempt_permit(permit).await;
    match result {
        Ok(response) => response,
        Err(status) => refresh_error_response(status, status == StatusCode::UNAUTHORIZED),
    }
}

async fn refresh_session_inner(
    ctx: &Arc<LegacyAuthService>,
    cookie: &RefreshSessionCookie,
) -> Result<Response, StatusCode> {
    let validated: ValidatedSessionInfo = match ctx
        .sessions
        .validate_session(cookie.tenant_id, &cookie.sid, &cookie.refresh_token)
        .await
    {
        Ok(validated) => validated,
        Err(error) => return Err(handle_refresh_error(&ctx.access_revocation, error).await),
    };
    debug!(
        "Refresh session validated; loading current account state: tenant_id={} account_id={} sid={} username={}",
        validated.tenant_id, validated.account_id, validated.sid, validated.username
    );
    let account_lookup: Option<UserAccount> = ctx
        .core_entity
        .get_current_account_by_username(validated.tenant_id, &validated.username)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let account: UserAccount = match account_lookup {
        Some(account) if account.id == validated.account_id && account.active => account,
        account => {
            warn!(
                "Refresh session account is no longer eligible; revoking session: tenant_id={} expected_account_id={} actual_account_id={:?} active={:?} sid={}",
                validated.tenant_id,
                validated.account_id,
                account.as_ref().map(|value: &UserAccount| value.id),
                account.as_ref().map(|value: &UserAccount| value.active),
                validated.sid
            );
            revoke_validated_session(ctx, &validated).await?;
            return Err(StatusCode::UNAUTHORIZED);
        }
    };
    if account.auth_version != validated.auth_version {
        info!(
            "Refresh session uses a stale account authorization version; revoking session: tenant_id={} account_id={} sid={} session_version={} current_version={}",
            validated.tenant_id, validated.account_id, validated.sid, validated.auth_version, account.auth_version
        );
        revoke_validated_session(ctx, &validated).await?;
        return Err(StatusCode::UNAUTHORIZED);
    }

    let now: usize = unix_now()?;
    let expiration: usize = ctx.jwt.expiration_for_role(&account.role);
    let access_expires_at: usize = now.checked_add(expiration).ok_or_else(|| {
        error!("Access-token expiry overflow during refresh: account_id={}", account.id);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let new_jti: String = Uuid::new_v4().to_string();
    // Encode before mutating Redis. A local signing failure must not consume the
    // client's valid refresh token and strand the session without a response.
    let claims: AccessClaims = access_claims(
        validated.tenant_id,
        &account,
        now,
        access_expires_at,
        new_jti.clone(),
        validated.sid.clone(),
    );
    let access_token: String = encode_access_token(ctx, &claims)?;
    let rotated: RotatedSessionInfo = match ctx
        .sessions
        .rotate_session(
            validated.tenant_id,
            &cookie.sid,
            &cookie.refresh_token,
            &account.role,
            account.auth_version,
            &new_jti,
            access_expires_at as u64,
        )
        .await
    {
        Ok(rotated) => rotated,
        Err(error) => return Err(handle_refresh_error(&ctx.access_revocation, error).await),
    };
    if rotated.tenant_id != validated.tenant_id
        || rotated.account_id != account.id
        || rotated.sid != validated.sid
        || rotated.username != account.username
        || rotated.role != account.role
        || rotated.auth_version != account.auth_version
        || rotated.jti != new_jti
    {
        error!(
            "Refresh session identity changed during rotation; revoking session: validated_tenant_id={} rotated_tenant_id={} validated_account_id={} rotated_account_id={} sid={}",
            validated.tenant_id, rotated.tenant_id, validated.account_id, rotated.account_id, rotated.sid
        );
        revoke_rotated_session(ctx, &rotated).await?;
        return Err(StatusCode::UNAUTHORIZED);
    }
    if let Some(token) = &rotated.revoked_access_token {
        revoke_access_token(ctx, token).await;
    }

    let response_cookie: HeaderValue = match refresh_cookie_header(
        Some(rotated.tenant_id),
        &rotated.sid,
        &rotated.refresh_token,
        ctx.sessions.expiration_secs.as_secs(),
    ) {
        Ok(cookie) => cookie,
        Err(status) => {
            error!(
                "Failed to construct rotated refresh cookie; revoking session: tenant_id={} account_id={} sid={}",
                rotated.tenant_id, rotated.account_id, rotated.sid
            );
            revoke_rotated_session(ctx, &rotated).await?;
            return Err(status);
        }
    };
    info!(
        "Refresh completed: tenant_id={} account_id={} sid={} old_access_revoked={} new_jti={}",
        rotated.tenant_id,
        rotated.account_id,
        rotated.sid,
        rotated.revoked_access_token.is_some(),
        rotated.jti
    );

    Ok((
        StatusCode::OK,
        sensitive_headers(Some(response_cookie)),
        Json(AuthResponse {
            access_token,
            token_type: "Bearer".to_owned(),
            expires_in: expiration,
        }),
    )
        .into_response())
}

pub async fn get_profile(Extension(user): Extension<AuthenticatedUser>) -> impl IntoResponse {
    (
        sensitive_headers(None),
        Json(AuthProfileResponse {
            tenant_id: user.tenant_id.to_string(),
            account_id: user.account_id.to_string(),
            username: user.username,
            role: user.role,
            roles: user.roles,
            auth_version: user.auth_version,
            permissions: user.permissions,
        }),
    )
}

pub async fn list_accounts(
    State(ctx): State<Arc<LegacyAuthService>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<impl IntoResponse, StatusCode> {
    require_permission(&user, "auth.accounts.read")?;
    let accounts: Vec<AccountSummary> = ctx.core_entity.list_accounts(user.tenant_id).await.map_err(|error| {
        error!(
            "Tenant account listing failed: tenant_id={} error={}",
            user.tenant_id, error
        );
        StatusCode::SERVICE_UNAVAILABLE
    })?;
    Ok((sensitive_headers(None), Json(accounts)))
}

pub async fn get_authorization_catalog(
    State(ctx): State<Arc<LegacyAuthService>>,
    Extension(user): Extension<AuthenticatedUser>,
) -> Result<impl IntoResponse, StatusCode> {
    require_permission(&user, "auth.roles.read")?;
    let catalog: AuthorizationCatalog = ctx
        .core_entity
        .list_authorization_catalog(user.tenant_id)
        .await
        .map_err(|error| {
            error!(
                "Tenant authorization catalog listing failed: tenant_id={} error={}",
                user.tenant_id, error
            );
            StatusCode::SERVICE_UNAVAILABLE
        })?;
    Ok((sensitive_headers(None), Json(catalog)))
}

pub async fn change_own_password(
    State(ctx): State<Arc<LegacyAuthService>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<Response, StatusCode> {
    payload.validate().map_err(|_| StatusCode::BAD_REQUEST)?;
    if payload.current_passphrase == payload.new_passphrase {
        return Err(StatusCode::BAD_REQUEST);
    }
    ctx.core_entity
        .change_own_password(
            user.tenant_id,
            user.account_id,
            &user.username,
            &payload.current_passphrase,
            &payload.new_passphrase,
        )
        .await
        .map_err(|error| match error {
            ChangeOwnPasswordError::InvalidCurrentPassword => StatusCode::UNAUTHORIZED,
            ChangeOwnPasswordError::AccountNotFound => StatusCode::NOT_FOUND,
            ChangeOwnPasswordError::BackendUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        })?;
    revoke_all_account_sessions(&ctx, user.tenant_id, user.account_id).await?;
    message_with_cleared_cookie("Password changed; all sessions were revoked")
}

pub async fn reset_account_password(
    State(ctx): State<Arc<LegacyAuthService>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(account_id): Path<Uuid>,
    Json(payload): Json<ResetPasswordRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    require_permission(&user, "auth.accounts.update")?;
    payload.validate().map_err(|_| StatusCode::BAD_REQUEST)?;
    ctx.core_entity
        .set_password(user.tenant_id, account_id, &payload.new_passphrase, user.account_id)
        .await
        .map_err(account_mutation_status)?;
    revoke_all_account_sessions(&ctx, user.tenant_id, account_id).await?;
    Ok((
        sensitive_headers(None),
        Json(MessageResponse {
            msg: "Password reset; all account sessions were revoked".to_owned(),
        }),
    ))
}

pub async fn update_account_status(
    State(ctx): State<Arc<LegacyAuthService>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(account_id): Path<Uuid>,
    Json(payload): Json<UpdateAccountStatusRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    require_permission(&user, "auth.accounts.disable")?;
    if account_id == user.account_id {
        return Err(StatusCode::CONFLICT);
    }
    ctx.core_entity
        .set_account_status(user.tenant_id, account_id, payload.status, user.account_id)
        .await
        .map_err(account_mutation_status)?;
    revoke_all_account_sessions(&ctx, user.tenant_id, account_id).await?;
    Ok((
        sensitive_headers(None),
        Json(MessageResponse {
            msg: "Account status updated; all account sessions were revoked".to_owned(),
        }),
    ))
}

pub async fn update_account_roles(
    State(ctx): State<Arc<LegacyAuthService>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(account_id): Path<Uuid>,
    Json(payload): Json<UpdateAccountRolesRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    require_permission(&user, "auth.roles.manage")?;
    ctx.core_entity
        .set_account_roles(
            user.tenant_id,
            account_id,
            payload.primary_role,
            &payload.roles,
            user.account_id,
        )
        .await
        .map_err(account_mutation_status)?;
    revoke_all_account_sessions(&ctx, user.tenant_id, account_id).await?;
    Ok((
        sensitive_headers(None),
        Json(MessageResponse {
            msg: "Account roles updated; all account sessions were revoked".to_owned(),
        }),
    ))
}

pub async fn update_account_permissions(
    State(ctx): State<Arc<LegacyAuthService>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(account_id): Path<Uuid>,
    Json(payload): Json<UpdateAccountPermissionsRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    require_permission(&user, "auth.roles.manage")?;
    ctx.core_entity
        .set_account_permissions(user.tenant_id, account_id, &payload.permissions, user.account_id)
        .await
        .map_err(account_mutation_status)?;
    revoke_all_account_sessions(&ctx, user.tenant_id, account_id).await?;
    Ok((
        sensitive_headers(None),
        Json(MessageResponse {
            msg: "Account permission overrides updated; all account sessions were revoked".to_owned(),
        }),
    ))
}

pub async fn register_new_user(
    State(ctx): State<Arc<LegacyAuthService>>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(payload): Json<RegisterUserRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    payload.validate().map_err(|_| StatusCode::BAD_REQUEST)?;
    if payload.username_is_blank() || !payload.username_is_valid() {
        return Err(StatusCode::BAD_REQUEST);
    }

    ctx.core_entity
        .create_account(
            user.tenant_id,
            payload.username.trim(),
            &payload.passphrase,
            payload.role,
            Some(user.account_id),
        )
        .await
        .map_err(|error: CreateAccountError| match error {
            CreateAccountError::UsernameAlreadyExists => StatusCode::CONFLICT,
            CreateAccountError::BackendUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        })?;

    Ok((
        StatusCode::CREATED,
        sensitive_headers(None),
        Json(MessageResponse {
            msg: "User registered successfully".to_owned(),
        }),
    ))
}

fn access_claims(
    tenant_id: Uuid,
    account: &UserAccount,
    now: usize,
    expires_at: usize,
    jti: String,
    sid: String,
) -> AccessClaims {
    AccessClaims {
        sub: account.id.to_string(),
        tid: tenant_id.to_string(),
        iss: "infra".to_owned(),
        aud: "infra-api".to_owned(),
        exp: expires_at,
        nbf: now,
        iat: now,
        jti,
        sid,
        username: account.username.clone(),
        role: account.role.clone(),
        roles: account.roles.clone(),
        ver: account.auth_version,
        permissions: account.permissions.clone(),
    }
}

fn encode_access_token(ctx: &LegacyAuthService, claims: &AccessClaims) -> Result<String, StatusCode> {
    let mut header: Header = Header::new(Algorithm::EdDSA);
    header.kid = Some(KID_MAIN!().to_owned());
    encode(&header, claims, ctx.jwt.encoding()).map_err(|error| {
        error!("Failed to encode access JWT: {}", error);
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

fn refresh_cookie_header(
    tenant_id: Option<Uuid>,
    sid: &str,
    refresh_token: &str,
    max_age: u64,
) -> Result<HeaderValue, StatusCode> {
    HeaderValue::from_str(&make_refresh_session_cookie(tenant_id, sid, refresh_token, max_age))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn message_with_cleared_cookie(message: &str) -> Result<Response, StatusCode> {
    let cookie: HeaderValue = refresh_cookie_header(None, "", "", 0)?;
    Ok((
        StatusCode::OK,
        sensitive_headers(Some(cookie)),
        Json(MessageResponse {
            msg: message.to_owned(),
        }),
    )
        .into_response())
}

async fn handle_refresh_error(access_revocation: &AccessRevocationCache, error: AuthSessionError) -> StatusCode {
    match error {
        AuthSessionError::BackendUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        AuthSessionError::RefreshTokenMismatch(revoked_access_token)
        | AuthSessionError::SessionExpired(revoked_access_token) => {
            if let Some(token) = revoked_access_token {
                info!(
                    "Blacklisting current access token after refresh-session invalidation: jti={} expires_at={}",
                    token.jti, token.expires_at
                );
                access_revocation.revoke_jti(&token.jti, token.expires_at).await;
            }
            StatusCode::UNAUTHORIZED
        }
        AuthSessionError::RefreshNotFound => StatusCode::UNAUTHORIZED,
    }
}

async fn revoke_validated_session(ctx: &LegacyAuthService, validated: &ValidatedSessionInfo) -> Result<(), StatusCode> {
    let revoked: RevokedSessionInfo = ctx
        .sessions
        .revoke_session(validated.tenant_id, validated.account_id, &validated.sid)
        .await
        .map_err(session_backend_status)?;
    if let Some(token) = revoked.access_token {
        revoke_access_token(ctx, &token).await;
    }
    Ok(())
}

async fn revoke_rotated_session(ctx: &LegacyAuthService, rotated: &RotatedSessionInfo) -> Result<(), StatusCode> {
    let revoked: RevokedSessionInfo = ctx
        .sessions
        .revoke_session(rotated.tenant_id, rotated.account_id, &rotated.sid)
        .await
        .map_err(session_backend_status)?;
    if let Some(token) = revoked.access_token {
        revoke_access_token(ctx, &token).await;
    }
    Ok(())
}

fn refresh_error_response(status: StatusCode, clear_cookie: bool) -> Response {
    let cookie: Option<HeaderValue> = if clear_cookie {
        refresh_cookie_header(None, "", "", 0).ok()
    } else {
        None
    };
    (status, sensitive_headers(cookie)).into_response()
}

fn sensitive_headers(cookie: Option<HeaderValue>) -> HeaderMap {
    let mut headers: HeaderMap = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    if let Some(cookie) = cookie {
        headers.insert(SET_COOKIE, cookie);
    }
    headers
}

fn session_backend_status(error: AuthSessionError) -> StatusCode {
    error!("Authentication session operation failed: {}", error);
    StatusCode::SERVICE_UNAVAILABLE
}

async fn revoke_access_token(ctx: &LegacyAuthService, token: &RevokedAccessTokenInfo) {
    ctx.access_revocation.revoke_jti(&token.jti, token.expires_at).await;
}

async fn revoke_all_account_sessions(
    ctx: &LegacyAuthService,
    tenant_id: Uuid,
    account_id: Uuid,
) -> Result<(), StatusCode> {
    let revoked = ctx
        .sessions
        .revoke_all_sessions(tenant_id, account_id)
        .await
        .map_err(session_backend_status)?;
    for token in &revoked.access_tokens {
        revoke_access_token(ctx, token).await;
    }
    info!(
        "Security-sensitive account change revoked sessions: tenant_id={} account_id={} revoked_access_tokens={}",
        tenant_id,
        account_id,
        revoked.access_tokens.len()
    );
    Ok(())
}

fn require_permission(user: &AuthenticatedUser, permission: &str) -> Result<(), StatusCode> {
    if user.has_permission(permission) {
        Ok(())
    } else {
        info!(
            "Account management denied: tenant_id={} account_id={} required_permission={}",
            user.tenant_id, user.account_id, permission
        );
        Err(StatusCode::FORBIDDEN)
    }
}

fn account_mutation_status(error: AccountMutationError) -> StatusCode {
    match error {
        AccountMutationError::AccountNotFound => StatusCode::NOT_FOUND,
        AccountMutationError::InvalidRole | AccountMutationError::InvalidPermission => StatusCode::BAD_REQUEST,
        AccountMutationError::LastTenantOwner => StatusCode::CONFLICT,
        AccountMutationError::BackendUnavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn unix_now() -> Result<usize, StatusCode> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as usize)
        .map_err(|error| {
            error!("System time error: {}", error);
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

pub async fn test_ping() -> impl IntoResponse {
    info!("Received auth ping request");
    (StatusCode::OK, "Pong!")
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use axum::http::StatusCode;

    use super::{handle_refresh_error, AuthSessionError, RevokedAccessTokenInfo};
    use crate::access_revocation::AccessRevocationCache;

    #[tokio::test]
    async fn refresh_token_mismatch_blacklists_current_access_token() {
        let cache = AccessRevocationCache::new_arc();
        let expires_at: u64 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs().saturating_add(60))
            .unwrap_or(60);
        let token = RevokedAccessTokenInfo {
            jti: "mismatched-refresh-current-jti".to_owned(),
            expires_at,
        };

        let status: StatusCode =
            handle_refresh_error(&cache, AuthSessionError::RefreshTokenMismatch(Some(token.clone()))).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(cache.is_revoked(&token.jti).await);
    }
}
