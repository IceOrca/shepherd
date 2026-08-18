use std::sync::OnceLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, RedisResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, MutexGuard, OwnedMutexGuard};
use uuid::Uuid;

use std::net::{IpAddr, SocketAddr};
use axum::{
    extract::{ConnectInfo, Request},
    http::HeaderMap,
};
use axum::{
    extract::{Extension, Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use async_trait::async_trait;
use axum::http::header::{COOKIE, SET_COOKIE, HeaderValue, InvalidHeaderValue};

use infra_redis::RedisAdapter;
use tracing::{error, warn, info, debug, trace};
use crate::account::Role;

pub const REFRESH_SESSION_COOKIE_NAME: &str = "refresh_session";
pub const REFRESH_SESSION_COOKIE_FMT: &str =
    "refresh_session={}; HttpOnly; Secure; SameSite=Strict; Path=/auth/refresh; Max-Age={}";

#[derive(Clone, Debug)]
pub struct RefreshSessionCookie {
    pub tenant_id: Uuid,
    pub sid: String,
    pub refresh_token: String,
}

pub fn make_refresh_session_cookie(
    tenant_id: Option<Uuid>,
    sid: &str,
    refresh_token: &str,
    max_age_secs: u64,
) -> String {
    let cookie_value: String = if tenant_id.is_none() || sid.is_empty() || refresh_token.is_empty() {
        String::new()
    } else {
        format!(
            "{}.{}.{}",
            tenant_id.map(|id: Uuid| id.to_string()).unwrap_or_default(),
            sid,
            refresh_token
        )
    };

    format!(
        "refresh_session={}; HttpOnly; Secure; SameSite=Strict; Path=/auth/refresh; Max-Age={}",
        cookie_value, max_age_secs
    )
}

/// Extract refresh_session in Cookie. The value is "{tenant_id}.{sid}.{refresh_token}".
pub fn parse_refresh_session_cookie(cookie_value: &str) -> Option<RefreshSessionCookie> {
    let mut parts = cookie_value.splitn(3, '.');
    let tenant_id: Uuid = Uuid::parse_str(parts.next()?).ok()?;
    let sid: &str = parts.next()?;
    let refresh_token: &str = parts.next()?;
    if tenant_id.is_nil() || !is_simple_uuid_value(sid) || !is_refresh_token_value(refresh_token) {
        return None;
    }

    Some(RefreshSessionCookie {
        tenant_id,
        sid: sid.to_string(),
        refresh_token: refresh_token.to_string(),
    })
}

/// Extract refresh_session in Cookie of Header
pub fn extract_refresh_session_cookie(headers: &HeaderMap) -> Option<RefreshSessionCookie> {
    let cookie_header: &str = headers.get(COOKIE)?.to_str().ok()?;

    let cookie_value: String = cookie_header
        .split(';')
        .find(|part: &&str| part.trim().starts_with(&format!("{}=", REFRESH_SESSION_COOKIE_NAME)))
        .map(|part: &str| {
            part.trim()
                .trim_start_matches(&format!("{}=", REFRESH_SESSION_COOKIE_NAME))
                .to_string()
        })?;

    parse_refresh_session_cookie(&cookie_value)
}

fn is_simple_uuid_value(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte: u8| byte.is_ascii_hexdigit())
}

fn is_refresh_token_value(value: &str) -> bool {
    let Some((first, second)) = value.split_once('.') else {
        return false;
    };
    !second.contains('.') && is_simple_uuid_value(first) && is_simple_uuid_value(second)
}

static TENANT_OWNER_SESSION_MAX: AtomicU8 = AtomicU8::new(4);
static SUPERVISOR_SESSION_MAX: AtomicU8 = AtomicU8::new(3);
static EMPLOYEE_SESSION_MAX: AtomicU8 = AtomicU8::new(2);
static TENANT_OWNER_SESSION_IDLE_TIMEOUT_SECS: AtomicU64 = AtomicU64::new(7200);
static SUPERVISOR_SESSION_IDLE_TIMEOUT_SECS: AtomicU64 = AtomicU64::new(7200);
static EMPLOYEE_SESSION_IDLE_TIMEOUT_SECS: AtomicU64 = AtomicU64::new(7200);

static CREATE_SESSION_SCRIPT: OnceLock<redis::Script> = OnceLock::new();
static VALIDATE_SESSION_SCRIPT: OnceLock<redis::Script> = OnceLock::new();
static ROTATE_SESSION_SCRIPT: OnceLock<redis::Script> = OnceLock::new();
static REVOKE_SESSION_SCRIPT: OnceLock<redis::Script> = OnceLock::new();
static REVOKE_ALL_SESSIONS_SCRIPT: OnceLock<redis::Script> = OnceLock::new();

#[derive(Debug, thiserror::Error)]
pub enum AuthSessionError {
    #[error("session server unavailable")]
    BackendUnavailable,
    #[error("refresh session not found")]
    RefreshNotFound,
    #[error("session expired")]
    SessionExpired(Option<RevokedAccessTokenInfo>),
    #[error("refresh token mismatch")]
    RefreshTokenMismatch(Option<RevokedAccessTokenInfo>),
}

#[derive(Clone, Debug)]
pub struct RevokedAccessTokenInfo {
    pub jti: String,
    pub expires_at: u64,
}

#[derive(Clone, Debug)]
pub struct CreatedSessionInfo {
    pub sid: String,
    pub refresh_token: String,
    pub kicked_access_tokens: Vec<RevokedAccessTokenInfo>,
}

#[derive(Clone, Debug)]
pub struct ValidatedSessionInfo {
    pub sid: String,
    pub tenant_id: Uuid,
    pub account_id: Uuid,
    pub username: String,
    pub auth_version: i64,
}

#[derive(Clone, Debug)]
pub struct RotatedSessionInfo {
    pub sid: String,
    pub tenant_id: Uuid,
    pub account_id: Uuid,
    pub username: String,
    pub role: Role,
    pub auth_version: i64,
    pub jti: String,
    pub refresh_token: String,
    pub expires_at: Instant,
    pub ttl: Duration,
    pub revoked_access_token: Option<RevokedAccessTokenInfo>,
}

#[derive(Clone, Debug)]
pub struct RevokedSessionInfo {
    pub access_token: Option<RevokedAccessTokenInfo>,
}

#[derive(Clone, Debug)]
pub struct RevokedAllSessionsInfo {
    pub access_tokens: Vec<RevokedAccessTokenInfo>,
}

pub(crate) struct RefreshAttemptPermit {
    sid: String,
    lock: Arc<Mutex<()>>,
    _guard: OwnedMutexGuard<()>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SessionEntry {
    sid: String,
    tenant_id: Uuid,
    account_id: Uuid,
    username: String,
    role: Role,
    auth_version: i64,
    jti: String,
    jti_exp: u64,
    /// refresh token id (hashed)
    rti: String,
    /// at Unix secs
    created_at: u64,
    last_rotate: u64,
    idle_timeout_secs: u64,
    expires_at: u64,
}

impl SessionEntry {
    fn new(
        sid: &str,
        tenant_id: Uuid,
        account_id: Uuid,
        username: &str,
        role: Role,
        auth_version: i64,
        jti: &str,
        jti_exp: u64,
        rti: &str,
        idle_timeout_secs: u64,
        ttl_secs: u64,
    ) -> Self {
        let now: u64 = unix_now();
        Self {
            sid: sid.to_string(),
            tenant_id,
            account_id,
            username: username.to_string(),
            role,
            auth_version,
            jti: jti.to_string(),
            jti_exp,
            rti: rti.to_string(),
            created_at: now,
            last_rotate: now,
            idle_timeout_secs,
            expires_at: now.saturating_add(ttl_secs),
        }
    }

    fn ttl(&self) -> Duration {
        Duration::from_secs(self.expires_at.saturating_sub(unix_now()))
    }

    fn expires_at(&self) -> Instant {
        Instant::now() + self.ttl()
    }

    fn to_rotated_info(
        &self,
        refresh_token: String,
        revoked_access_token: Option<RevokedAccessTokenInfo>,
    ) -> RotatedSessionInfo {
        RotatedSessionInfo {
            sid: self.sid.clone(),
            tenant_id: self.tenant_id,
            account_id: self.account_id,
            username: self.username.clone(),
            role: self.role.clone(),
            auth_version: self.auth_version,
            jti: self.jti.clone(),
            refresh_token,
            expires_at: self.expires_at(),
            ttl: self.ttl(),
            revoked_access_token,
        }
    }
}

pub struct AuthSessionHandle {
    redis_pool: Arc<RedisAdapter>,
    key_prefix: String,
    pub expiration_secs: Duration,
    // Refresh rotation is destructive on token mismatch. Keep overlapping requests
    // for one session from interpreting a still-in-flight rotation as token reuse.
    refresh_attempt_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl AuthSessionHandle {
    pub fn new_arc(redis: Arc<RedisAdapter>) -> Arc<Self> {
        let owner_session_max: u8 = read_session_max("TENANT_OWNER_MAX_SESSION", 4);
        let supervisor_session_max: u8 = read_session_max("SUPERVISOR_MAX_SESSION", 3);
        let employee_session_max: u8 = read_session_max("EMPLOYEE_MAX_SESSION", 2);
        let owner_idle_timeout: u64 = read_session_idle_timeout("TENANT_OWNER_AUTH_SESSION_IDLE_TIMEOUT_SECS", 7200);
        let supervisor_idle_timeout: u64 = read_session_idle_timeout("SUPERVISOR_AUTH_SESSION_IDLE_TIMEOUT_SECS", 7200);
        let employee_idle_timeout: u64 = read_session_idle_timeout("EMPLOYEE_AUTH_SESSION_IDLE_TIMEOUT_SECS", 7200);
        TENANT_OWNER_SESSION_MAX.store(owner_session_max, Ordering::Relaxed);
        SUPERVISOR_SESSION_MAX.store(supervisor_session_max, Ordering::Relaxed);
        EMPLOYEE_SESSION_MAX.store(employee_session_max, Ordering::Relaxed);
        TENANT_OWNER_SESSION_IDLE_TIMEOUT_SECS.store(owner_idle_timeout, Ordering::Relaxed);
        SUPERVISOR_SESSION_IDLE_TIMEOUT_SECS.store(supervisor_idle_timeout, Ordering::Relaxed);
        EMPLOYEE_SESSION_IDLE_TIMEOUT_SECS.store(employee_idle_timeout, Ordering::Relaxed);

        let expiration_secs: Duration = Duration::from_secs(refresh_token_max_ttl_secs());
        let key_prefix: String =
            std::env::var("AUTH_SESSION_REDIS_PREFIX").unwrap_or_else(|_| "infra:host:auth:".to_string());

        info!(
            "AuthSession Redis refresh-session store initialized: key_prefix={} refresh_ttl={}s",
            key_prefix,
            expiration_secs.as_secs()
        );

        Arc::new(Self {
            redis_pool: redis,
            key_prefix,
            expiration_secs,
            refresh_attempt_locks: Mutex::new(HashMap::new()),
        })
    }

    pub(crate) async fn try_acquire_refresh_attempt_permit(
        &self,
        tenant_id: Uuid,
        sid: &str,
    ) -> Option<RefreshAttemptPermit> {
        let sid: String = format!("{}:{}", tenant_id, sid);
        let lock: Arc<Mutex<()>> = {
            let mut locks: MutexGuard<HashMap<String, Arc<Mutex<()>>>> = self.refresh_attempt_locks.lock().await;
            Arc::clone(locks.entry(sid.clone()).or_insert_with(|| Arc::new(Mutex::new(()))))
        };

        match Arc::clone(&lock).try_lock_owned() {
            Ok(guard) => Some(RefreshAttemptPermit {
                sid,
                lock,
                _guard: guard,
            }),
            Err(_) => {
                // A concurrent refresh owns this sid. Reject before Redis access so its
                // successful token rotation cannot be revoked as suspicious reuse.
                let mut locks: MutexGuard<HashMap<String, Arc<Mutex<()>>>> = self.refresh_attempt_locks.lock().await;
                let can_remove: bool = locks.get(&sid).is_some_and(|registered_lock: &Arc<Mutex<()>>| {
                    Arc::ptr_eq(registered_lock, &lock) && Arc::strong_count(registered_lock) == 2
                });
                if can_remove {
                    locks.remove(&sid);
                }
                None
            }
        }
    }

    pub(crate) async fn release_refresh_attempt_permit(&self, permit: RefreshAttemptPermit) {
        let RefreshAttemptPermit { sid, lock, _guard } = permit;
        // Release the per-sid rotation guard before pruning its registry entry.
        drop(_guard);

        let mut locks: MutexGuard<HashMap<String, Arc<Mutex<()>>>> = self.refresh_attempt_locks.lock().await;
        let can_remove: bool = locks.get(&sid).is_some_and(|registered_lock: &Arc<Mutex<()>>| {
            Arc::ptr_eq(registered_lock, &lock) && Arc::strong_count(registered_lock) == 2
        });
        if can_remove {
            locks.remove(&sid);
        }
    }

    pub async fn create_session(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        username: &str,
        role: Role,
        auth_version: i64,
        jti: &str,
        jti_exp: u64,
    ) -> Result<CreatedSessionInfo, AuthSessionError> {
        let sid: String = new_sid();
        let refresh_token: String = new_refresh_token();
        let rti: String = hash_refresh_token(&refresh_token);
        let ttl_secs: u64 = self.expiration_secs.as_secs();
        let session_idle_timeout_secs: u64 = idle_timeout_secs(&role);
        let entry: SessionEntry = SessionEntry::new(
            &sid,
            tenant_id,
            account_id,
            username,
            role.clone(),
            auth_version,
            jti,
            jti_exp,
            &rti,
            session_idle_timeout_secs,
            ttl_secs,
        );
        let fields: Vec<(String, String)> = self.redis_entry_fields(&entry)?;
        let session_key: String = self.session_key(tenant_id, &sid);
        let user_sessions_key: String = self.user_sessions_key(tenant_id, account_id);
        let max: u8 = max_session(&role);
        let score: u64 = unix_now();
        let mut connection: MultiplexedConnection = self.connection().await?;

        let script: &redis::Script = CREATE_SESSION_SCRIPT.get_or_init(|| {
            info!("Init with create_session.lua script");
            redis::Script::new(include_str!("script/redis/create_session.lua"))
        });
        let mut invocation: redis::ScriptInvocation = script.key(&session_key);
        invocation
            .key(&user_sessions_key)
            .arg(&sid)
            .arg(ttl_secs)
            .arg(max)
            .arg(score)
            .arg(self.session_key_prefix(tenant_id));

        for (field_name, field_value) in &fields {
            invocation.arg(field_name).arg(field_value);
        }

        let kicked_result: Vec<String> =
            invocation
                .invoke_async(&mut connection)
                .await
                .map_err(|err: redis::RedisError| {
                    error!(
                        "Failed to create Redis auth refresh session: tenant_id={} account_id={} sid={} error={}",
                        tenant_id, account_id, sid, err
                    );
                    AuthSessionError::BackendUnavailable
                })?;

        let Some(status) = kicked_result.first() else {
            error!(
                "Redis auth session create returned empty status: tenant_id={} account_id={} sid={}",
                tenant_id, account_id, sid
            );
            return Err(AuthSessionError::BackendUnavailable);
        };
        if status != "ok" {
            error!(
                "Redis auth session create returned unexpected status: tenant_id={} account_id={} sid={} status={}",
                tenant_id, account_id, sid, status
            );
            return Err(AuthSessionError::BackendUnavailable);
        }
        let kicked_access_tokens: Vec<RevokedAccessTokenInfo> = parse_revoked_token_pairs(kicked_result.iter().skip(1));
        for kicked_access_token in &kicked_access_tokens {
            info!(
                "AuthSession kicked oldest refresh session: tenant_id={} account_id={} kicked_jti={} kicked_jti_expires_at={}",
                tenant_id, account_id, kicked_access_token.jti, kicked_access_token.expires_at
            );
        }

        info!(
            "AuthSession created: server=redis tenant_id={} account_id={} sid={} jti={} ttl={}s max={}",
            tenant_id, account_id, sid, jti, ttl_secs, max
        );
        Ok(CreatedSessionInfo {
            sid,
            refresh_token,
            kicked_access_tokens,
        })
    }

    pub async fn rotate_session(
        &self,
        tenant_id: Uuid,
        sid: &str,
        refresh_token: &str,
        current_role: &Role,
        current_auth_version: i64,
        jti: &str,
        jti_exp: u64,
    ) -> Result<RotatedSessionInfo, AuthSessionError> {
        let presented_rti: String = hash_refresh_token(refresh_token);
        let new_refresh_token: String = new_refresh_token();
        let new_rti: String = hash_refresh_token(&new_refresh_token);
        let ttl_secs: u64 = self.expiration_secs.as_secs();
        let now: u64 = unix_now();
        let expires_at: u64 = now.saturating_add(ttl_secs);
        let fallback_idle_timeout_secs: u64 = idle_timeout_secs(&Role::Employee);
        let new_idle_timeout_secs: u64 = idle_timeout_secs(current_role);
        let current_role_json: String = serde_json::to_string(current_role).map_err(|err: serde_json::Error| {
            error!(
                "Failed to serialize current role for Redis auth session rotation: {}",
                err
            );
            AuthSessionError::BackendUnavailable
        })?;
        let session_key: String = self.session_key(tenant_id, sid);
        let user_order_key_prefix: String = self.user_order_key_prefix(tenant_id);
        let mut connection: MultiplexedConnection = self.connection().await?;

        let script: &redis::Script = ROTATE_SESSION_SCRIPT.get_or_init(|| {
            info!("Init with rotate_session.lua script");
            redis::Script::new(include_str!("script/redis/rotate_session.lua"))
        });
        let mut invocation: redis::ScriptInvocation = script.key(&session_key);
        invocation
            .arg(sid)
            .arg(&presented_rti)
            .arg(&new_rti)
            .arg(jti)
            .arg(jti_exp)
            .arg(ttl_secs)
            .arg(now)
            .arg(expires_at)
            .arg(fallback_idle_timeout_secs)
            .arg(&user_order_key_prefix)
            .arg(&current_role_json)
            .arg(new_idle_timeout_secs)
            .arg(current_auth_version);

        let result: Vec<String> =
            invocation
                .invoke_async(&mut connection)
                .await
                .map_err(|err: redis::RedisError| {
                    error!("Failed to rotate Redis auth refresh session: sid={} error={}", sid, err);
                    AuthSessionError::BackendUnavailable
                })?;

        let Some(status) = result.first() else {
            error!("Redis auth session rotate returned empty status: sid={}", sid);
            return Err(AuthSessionError::BackendUnavailable);
        };

        let revoked_access_token: Option<RevokedAccessTokenInfo> =
            parse_revoked_token_fields(result.get(1), result.get(2));
        if status == "not_found" {
            info!(
                "AuthSession refresh rejected: server=redis sid={} reason=not_found",
                sid
            );
            return Err(AuthSessionError::RefreshNotFound);
        }
        if status == "expired" {
            info!("AuthSession refresh rejected: server=redis sid={} reason=expired", sid);
            return Err(AuthSessionError::SessionExpired(revoked_access_token));
        }
        if status == "idle_timeout" {
            info!(
                "AuthSession refresh rejected: server=redis sid={} reason=idle_timeout",
                sid
            );
            return Err(AuthSessionError::SessionExpired(revoked_access_token));
        }
        if status == "mismatch" {
            warn!(
                "AuthSession refresh token mismatch; session revoked: server=redis sid={} revoked_access={:?}",
                sid, revoked_access_token
            );
            return Err(AuthSessionError::RefreshTokenMismatch(revoked_access_token));
        }

        if status != "ok" {
            error!(
                "Redis auth session rotate returned unexpected status: sid={} status={}",
                sid, status
            );
            return Err(AuthSessionError::BackendUnavailable);
        }

        // The mutation returns the updated hash so rotation and JWT claim retrieval are atomic.
        let Some(values) = parse_hash_field_pairs(result.iter().skip(3)) else {
            error!("Redis auth session rotate returned invalid session fields: sid={}", sid);
            return Err(AuthSessionError::BackendUnavailable);
        };
        let Some(entry) = self.parse_hash_entry(sid, values) else {
            return Err(AuthSessionError::BackendUnavailable);
        };
        if entry.tenant_id != tenant_id {
            error!(
                "Redis auth session tenant mismatch: cookie_tid={} stored_tid={}",
                tenant_id, entry.tenant_id
            );
            return Err(AuthSessionError::RefreshNotFound);
        }

        info!(
            "AuthSession refresh rotated: server=redis tenant_id={} account_id={} sid={} new_access_jti={}",
            entry.tenant_id, entry.account_id, entry.sid, jti
        );
        Ok(entry.to_rotated_info(new_refresh_token, revoked_access_token))
    }

    pub async fn validate_session(
        &self,
        tenant_id: Uuid,
        sid: &str,
        refresh_token: &str,
    ) -> Result<ValidatedSessionInfo, AuthSessionError> {
        let presented_rti: String = hash_refresh_token(refresh_token);
        let now: u64 = unix_now();
        let fallback_idle_timeout_secs: u64 = idle_timeout_secs(&Role::Employee);
        let session_key: String = self.session_key(tenant_id, sid);
        let user_order_key_prefix: String = self.user_order_key_prefix(tenant_id);
        let mut connection: MultiplexedConnection = self.connection().await?;

        let script: &redis::Script = VALIDATE_SESSION_SCRIPT.get_or_init(|| {
            info!("Init with validate_session.lua script");
            redis::Script::new(include_str!("script/redis/validate_session.lua"))
        });
        let mut invocation: redis::ScriptInvocation = script.key(&session_key);
        invocation
            .arg(sid)
            .arg(&presented_rti)
            .arg(now)
            .arg(fallback_idle_timeout_secs)
            .arg(&user_order_key_prefix);

        let result: Vec<String> =
            invocation
                .invoke_async(&mut connection)
                .await
                .map_err(|err: redis::RedisError| {
                    error!(
                        "Failed to validate Redis auth refresh session: sid={} error={}",
                        sid, err
                    );
                    AuthSessionError::BackendUnavailable
                })?;

        let Some(status) = result.first() else {
            error!("Redis auth session validation returned empty status: sid={}", sid);
            return Err(AuthSessionError::BackendUnavailable);
        };

        let revoked_access_token: Option<RevokedAccessTokenInfo> =
            parse_revoked_token_fields(result.get(1), result.get(2));
        if status == "not_found" {
            info!(
                "AuthSession refresh rejected: server=redis sid={} reason=not_found",
                sid
            );
            return Err(AuthSessionError::RefreshNotFound);
        }
        if status == "expired" || status == "idle_timeout" {
            info!(
                "AuthSession refresh rejected: server=redis sid={} reason={}",
                sid, status
            );
            return Err(AuthSessionError::SessionExpired(revoked_access_token));
        }
        if status == "mismatch" {
            warn!(
                "AuthSession refresh token mismatch; session revoked: server=redis sid={} revoked_access={:?}",
                sid, revoked_access_token
            );
            return Err(AuthSessionError::RefreshTokenMismatch(revoked_access_token));
        }
        if status != "ok" {
            error!(
                "Redis auth session validation returned unexpected status: sid={} status={}",
                sid, status
            );
            return Err(AuthSessionError::BackendUnavailable);
        }

        // Validation intentionally returns current state without writing it;
        // only rotation after current-account verification mutates the session.
        let Some(values) = parse_hash_field_pairs(result.iter().skip(1)) else {
            error!(
                "Redis auth session validation returned invalid session fields: sid={}",
                sid
            );
            return Err(AuthSessionError::BackendUnavailable);
        };
        let Some(entry) = self.parse_hash_entry(sid, values) else {
            return Err(AuthSessionError::BackendUnavailable);
        };
        if entry.tenant_id != tenant_id {
            error!(
                "Redis auth session tenant mismatch: cookie_tid={} stored_tid={}",
                tenant_id, entry.tenant_id
            );
            return Err(AuthSessionError::RefreshNotFound);
        }

        Ok(ValidatedSessionInfo {
            sid: entry.sid,
            tenant_id: entry.tenant_id,
            account_id: entry.account_id,
            username: entry.username,
            auth_version: entry.auth_version,
        })
    }

    pub async fn revoke_session(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
        sid: &str,
    ) -> Result<RevokedSessionInfo, AuthSessionError> {
        let session_key: String = self.session_key(tenant_id, sid);
        let user_sessions_key: String = self.user_sessions_key(tenant_id, account_id);
        let mut connection: MultiplexedConnection = self.connection().await?;

        let script: &redis::Script = REVOKE_SESSION_SCRIPT.get_or_init(|| {
            info!("Init with revoke_session.lua script");
            redis::Script::new(include_str!("script/redis/revoke_session.lua"))
        });
        let mut invocation: redis::ScriptInvocation = script.key(&session_key);
        invocation.key(&user_sessions_key).arg(account_id.to_string()).arg(sid);

        let result: RedisResult<Vec<String>> = invocation.invoke_async(&mut connection).await;

        match result {
            Ok(result) => {
                let access_token: Option<RevokedAccessTokenInfo> = parse_revoked_token_result(&result);
                info!(
                    "AuthSession revoked refresh session: server=redis tenant_id={} account_id={} sid={} revoked_access={:?}",
                    tenant_id, account_id, sid, access_token
                );
                Ok(RevokedSessionInfo { access_token })
            }
            Err(err) => {
                error!(
                    "Failed to revoke Redis auth refresh session: server=redis tenant_id={} account_id={} sid={} error={}",
                    tenant_id, account_id, sid, err
                );
                Err(AuthSessionError::BackendUnavailable)
            }
        }
    }

    pub async fn revoke_all_sessions(
        &self,
        tenant_id: Uuid,
        account_id: Uuid,
    ) -> Result<RevokedAllSessionsInfo, AuthSessionError> {
        let session_key_prefix: String = self.session_key_prefix(tenant_id);
        let user_sessions_key: String = self.user_sessions_key(tenant_id, account_id);
        let mut connection: MultiplexedConnection = self.connection().await?;

        let script: &redis::Script = REVOKE_ALL_SESSIONS_SCRIPT.get_or_init(|| {
            info!("Init with revoke_all_sessions.lua script");
            redis::Script::new(include_str!("script/redis/revoke_all_sessions.lua"))
        });
        let mut invocation: redis::ScriptInvocation = script.key(&user_sessions_key);
        invocation.arg(&session_key_prefix);

        let result: RedisResult<Vec<String>> = invocation.invoke_async(&mut connection).await;

        match result {
            Ok(result) => {
                let access_tokens: Vec<RevokedAccessTokenInfo> = parse_revoked_token_pairs(result.iter());
                info!(
                    "AuthSession revoke-all complete: server=redis tenant_id={} account_id={} access_token_count={}",
                    tenant_id,
                    account_id,
                    access_tokens.len()
                );
                Ok(RevokedAllSessionsInfo { access_tokens })
            }
            Err(err) => {
                error!(
                    "Failed to revoke all Redis auth refresh sessions: server=redis tenant_id={} account_id={} error={}",
                    tenant_id, account_id, err
                );
                Err(AuthSessionError::BackendUnavailable)
            }
        }
    }

    fn session_key(&self, tenant_id: Uuid, sid: &str) -> String {
        format!("{}tenant:{}:session:{}", self.key_prefix, tenant_id, sid)
    }

    fn session_key_prefix(&self, tenant_id: Uuid) -> String {
        format!("{}tenant:{}:session:", self.key_prefix, tenant_id)
    }

    fn user_sessions_key(&self, tenant_id: Uuid, account_id: Uuid) -> String {
        format!(
            "{}tenant:{}:account_sessions:{}",
            self.key_prefix, tenant_id, account_id
        )
    }

    fn user_order_key_prefix(&self, tenant_id: Uuid) -> String {
        format!("{}tenant:{}:account_sessions:", self.key_prefix, tenant_id)
    }

    async fn connection(&self) -> Result<MultiplexedConnection, AuthSessionError> {
        self.redis_pool.connection().await.map_err(|err: redis::RedisError| {
            error!("Failed to connect to Redis for auth session server: {}", err);
            AuthSessionError::BackendUnavailable
        })
    }

    fn parse_hash_entry(&self, sid: &str, values: HashMap<String, String>) -> Option<SessionEntry> {
        let stored_sid: String = match values.get("sid") {
            Some(stored_sid) => stored_sid.clone(),
            None => {
                error!("Redis auth session entry missing sid: sid={}", sid);
                return None;
            }
        };
        if stored_sid != sid {
            error!(
                "Redis auth session sid mismatch: key_sid={} stored_sid={}",
                sid, stored_sid
            );
            return None;
        }
        let tenant_id: Uuid = match values
            .get("tenant_id")
            .and_then(|value: &String| Uuid::parse_str(value).ok())
        {
            Some(tenant_id) => tenant_id,
            None => {
                error!("Redis auth session entry missing or invalid tenant_id: sid={}", sid);
                return None;
            }
        };
        let account_id: Uuid = match values
            .get("account_id")
            .and_then(|value: &String| Uuid::parse_str(value).ok())
        {
            Some(account_id) => account_id,
            None => {
                error!("Redis auth session entry missing or invalid account_id: sid={}", sid);
                return None;
            }
        };
        let username: String = match values.get("username") {
            Some(username) => username.clone(),
            None => {
                error!("Redis auth session entry missing username: sid={}", sid);
                return None;
            }
        };
        let role: Role = match values
            .get("role")
            .and_then(|role_json| serde_json::from_str::<Role>(role_json).ok())
        {
            Some(role) => role,
            None => {
                error!("Redis auth session entry missing or invalid role: sid={}", sid);
                return None;
            }
        };
        // Sessions created before auth-version enforcement are treated as
        // stale and will be revoked when compared with the current account.
        let auth_version: i64 = values
            .get("auth_version")
            .and_then(|value: &String| value.parse::<i64>().ok())
            .unwrap_or(0);
        let jti: String = match values.get("jti") {
            Some(jti) => jti.clone(),
            None => {
                error!("Redis auth session entry missing jti: sid={}", sid);
                return None;
            }
        };
        let jti_exp: u64 = match values.get("jti_exp").and_then(|value| value.parse::<u64>().ok()) {
            Some(jti_exp) => jti_exp,
            None => {
                error!("Redis auth session entry missing jti_exp: sid={}", sid);
                return None;
            }
        };
        let rti: String = match values.get("rti") {
            Some(rti) => rti.clone(),
            None => {
                error!("Redis auth session entry missing rti: sid={}", sid);
                return None;
            }
        };
        let created_at: u64 = values
            .get("created_at")
            .and_then(|value: &String| value.parse::<u64>().ok())
            .unwrap_or_else(unix_now);
        let last_rotate: u64 = values
            .get("last_rotate")
            .and_then(|value: &String| value.parse::<u64>().ok())
            .unwrap_or(created_at);
        let idle_timeout_secs: u64 = values
            .get("idle_timeout_secs")
            .and_then(|value: &String| value.parse::<u64>().ok())
            .unwrap_or_else(|| idle_timeout_secs(&role));
        let expires_at: u64 = match values
            .get("expires_at")
            .and_then(|value: &String| value.parse::<u64>().ok())
        {
            Some(expires_at) => expires_at,
            None => {
                error!("Redis auth session entry missing expires_at: sid={}", sid);
                return None;
            }
        };

        Some(SessionEntry {
            sid: stored_sid,
            tenant_id,
            account_id,
            username,
            role,
            auth_version,
            jti,
            jti_exp,
            rti,
            created_at,
            last_rotate,
            idle_timeout_secs,
            expires_at,
        })
    }

    fn redis_entry_fields(&self, entry: &SessionEntry) -> Result<Vec<(String, String)>, AuthSessionError> {
        let role_json: String = serde_json::to_string(&entry.role).map_err(|err: serde_json::Error| {
            error!("Failed to serialize role for Redis auth session: {}", err);
            AuthSessionError::BackendUnavailable
        })?;
        Ok(vec![
            ("sid".to_string(), entry.sid.clone()),
            ("tenant_id".to_string(), entry.tenant_id.to_string()),
            ("account_id".to_string(), entry.account_id.to_string()),
            ("username".to_string(), entry.username.clone()),
            ("role".to_string(), role_json),
            ("auth_version".to_string(), entry.auth_version.to_string()),
            ("jti".to_string(), entry.jti.clone()),
            ("jti_exp".to_string(), entry.jti_exp.to_string()),
            ("rti".to_string(), entry.rti.clone()),
            ("created_at".to_string(), entry.created_at.to_string()),
            ("last_rotate".to_string(), entry.last_rotate.to_string()),
            ("idle_timeout_secs".to_string(), entry.idle_timeout_secs.to_string()),
            ("expires_at".to_string(), entry.expires_at.to_string()),
        ])
    }
}

fn read_session_max(env_name: &str, default_value: u8) -> u8 {
    match std::env::var(env_name) {
        Ok(value) => value.parse().unwrap_or_else(|err: std::num::ParseIntError| {
            warn!("Invalid {} format: {}, using default {}", env_name, err, default_value);
            default_value
        }),
        Err(_) => {
            warn!("{} not set, using default {}", env_name, default_value);
            default_value
        }
    }
}

fn refresh_token_max_ttl_secs() -> u64 {
    match std::env::var("REFRESH_TOKEN_EXPIRATION_SECS") {
        Ok(value) => value.parse().unwrap_or_else(|err: std::num::ParseIntError| {
            warn!(
                "Invalid REFRESH_TOKEN_EXPIRATION_SECS format: {}, using default 604800s",
                err
            );
            604800
        }),
        Err(_) => {
            warn!("REFRESH_TOKEN_EXPIRATION_SECS not set, using default 604800s");
            604800
        }
    }
}

fn read_session_idle_timeout(env_name: &str, default_value: u64) -> u64 {
    match std::env::var(env_name) {
        Ok(value) => value.parse().unwrap_or_else(|err: std::num::ParseIntError| {
            warn!("Invalid {} format: {}, using default {}s", env_name, err, default_value);
            default_value
        }),
        Err(_) => {
            warn!("{} not set, using default {}s", env_name, default_value);
            default_value
        }
    }
}

fn idle_timeout_secs(role: &Role) -> u64 {
    if matches!(role, Role::TenantOwner) {
        TENANT_OWNER_SESSION_IDLE_TIMEOUT_SECS.load(Ordering::Relaxed)
    } else if matches!(role, Role::Supervisor) {
        SUPERVISOR_SESSION_IDLE_TIMEOUT_SECS.load(Ordering::Relaxed)
    } else {
        EMPLOYEE_SESSION_IDLE_TIMEOUT_SECS.load(Ordering::Relaxed)
    }
}

fn max_session(role: &Role) -> u8 {
    if matches!(role, Role::TenantOwner) {
        TENANT_OWNER_SESSION_MAX.load(Ordering::Relaxed)
    } else if matches!(role, Role::Supervisor) {
        SUPERVISOR_SESSION_MAX.load(Ordering::Relaxed)
    } else {
        EMPLOYEE_SESSION_MAX.load(Ordering::Relaxed)
    }
}

fn new_sid() -> String {
    Uuid::new_v4().simple().to_string()
}

fn new_refresh_token() -> String {
    format!("{}.{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn hash_refresh_token(token: &str) -> String {
    let mut hasher: Sha256 = Sha256::new();
    hasher.update(token.as_bytes());
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

fn unix_now() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(err) => {
            error!("System time error while computing auth session unix time: {}", err);
            0
        }
    }
}

fn parse_revoked_token_result(values: &[String]) -> Option<RevokedAccessTokenInfo> {
    parse_revoked_token_fields(values.get(1), values.get(2))
}

fn parse_revoked_token_fields(jti: Option<&String>, expires_at: Option<&String>) -> Option<RevokedAccessTokenInfo> {
    let jti: String = match jti {
        Some(jti) if !jti.is_empty() => jti.clone(),
        _ => return None,
    };
    let expires_at: u64 = expires_at.and_then(|value| value.parse::<u64>().ok()).unwrap_or(0);
    if expires_at <= unix_now() {
        return None;
    }
    Some(RevokedAccessTokenInfo { jti, expires_at })
}

fn parse_revoked_token_pairs<'a>(values: impl IntoIterator<Item = &'a String>) -> Vec<RevokedAccessTokenInfo> {
    let mut access_tokens: Vec<RevokedAccessTokenInfo> = Vec::new();
    let mut iter = values.into_iter();
    while let Some(jti) = iter.next() {
        let expires_at: Option<&String> = iter.next();
        if let Some(access_token) = parse_revoked_token_fields(Some(jti), expires_at) {
            access_tokens.push(access_token);
        }
    }
    access_tokens
}

fn parse_hash_field_pairs<'a>(values: impl IntoIterator<Item = &'a String>) -> Option<HashMap<String, String>> {
    let mut parsed_values: HashMap<String, String> = HashMap::new();
    let mut iter = values.into_iter();
    while let Some(field_name) = iter.next() {
        let field_value: &String = iter.next()?;
        parsed_values.insert(field_name.clone(), field_value.clone());
    }
    Some(parsed_values)
}

#[cfg(test)]
mod tests {
    use super::{
        make_refresh_session_cookie, parse_hash_field_pairs, parse_refresh_session_cookie, parse_revoked_token_pairs,
        unix_now, AuthSessionHandle,
    };
    use infra_redis::RedisAdapter;
    use uuid::Uuid;

    #[test]
    fn parses_refresh_session_cookie_value_with_expected_token_shape() {
        let tenant_id = Uuid::new_v4();
        let sid: String = Uuid::new_v4().simple().to_string();
        let refresh_token: String = format!("{}.{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let parsed = parse_refresh_session_cookie(&format!("{}.{}.{}", tenant_id, sid, refresh_token));
        assert!(parsed.is_some());
        let Some(parsed) = parsed else {
            return;
        };

        assert_eq!(parsed.tenant_id, tenant_id);
        assert_eq!(parsed.sid, sid);
        assert_eq!(parsed.refresh_token, refresh_token);
    }

    #[test]
    fn rejects_refresh_session_cookie_value_without_sid_or_token() {
        assert!(parse_refresh_session_cookie("tenant.sid.token").is_none());
        assert!(parse_refresh_session_cookie("sid.token").is_none());
        assert!(parse_refresh_session_cookie("sidonly").is_none());
    }

    #[test]
    fn rejects_refresh_session_cookie_with_unexpected_identifier_shapes() {
        let tenant_id: Uuid = Uuid::new_v4();
        let valid_sid: String = Uuid::new_v4().simple().to_string();
        let valid_token: String = format!("{}.{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());

        assert!(parse_refresh_session_cookie(&format!("{}.short.{}", tenant_id, valid_token)).is_none());
        assert!(parse_refresh_session_cookie(&format!("{}.{}.short", tenant_id, valid_sid)).is_none());
        assert!(parse_refresh_session_cookie(&format!("{}.{}.{}.extra", tenant_id, valid_sid, valid_token)).is_none());
    }

    #[test]
    fn formats_clear_refresh_session_cookie_without_value() {
        let cookie = make_refresh_session_cookie(None, "", "", 0);

        assert!(cookie.starts_with("refresh_session=;"));
        assert!(cookie.contains("Max-Age=0"));
    }

    #[test]
    fn parses_even_hash_field_pairs_from_script_response() {
        let fields: Vec<String> = vec![
            "sid".to_string(),
            "session-1".to_string(),
            "jti".to_string(),
            "jti-1".to_string(),
        ];

        let parsed = parse_hash_field_pairs(fields.iter());

        assert!(parsed.is_some());
        let Some(parsed) = parsed else {
            return;
        };
        assert_eq!(parsed.get("sid"), Some(&"session-1".to_string()));
        assert_eq!(parsed.get("jti"), Some(&"jti-1".to_string()));
    }

    #[test]
    fn rejects_odd_hash_field_pairs_from_script_response() {
        let fields: Vec<String> = vec!["sid".to_string(), "session-1".to_string(), "jti".to_string()];

        assert!(parse_hash_field_pairs(fields.iter()).is_none());
    }

    #[test]
    fn parses_all_unexpired_kicked_access_tokens() {
        let unexpired: String = unix_now().saturating_add(60).to_string();
        let kicked: Vec<String> = vec![
            "old-jti-1".to_string(),
            unexpired.clone(),
            "old-jti-2".to_string(),
            unexpired,
        ];

        let parsed = parse_revoked_token_pairs(kicked.iter());

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed.first().map(|token| token.jti.as_str()), Some("old-jti-1"));
        assert_eq!(parsed.get(1).map(|token| token.jti.as_str()), Some("old-jti-2"));
    }

    #[tokio::test]
    async fn rejects_overlapping_refresh_attempts_for_one_session() {
        let sessions = AuthSessionHandle::new_arc(RedisAdapter::new_arc());
        let tenant_id = Uuid::new_v4();
        let first_permit = sessions.try_acquire_refresh_attempt_permit(tenant_id, "sid-1").await;
        assert!(first_permit.is_some());

        let overlapping_permit = sessions.try_acquire_refresh_attempt_permit(tenant_id, "sid-1").await;
        assert!(overlapping_permit.is_none());

        let other_tenant_permit = sessions
            .try_acquire_refresh_attempt_permit(Uuid::new_v4(), "sid-1")
            .await;
        assert!(other_tenant_permit.is_some());
        if let Some(other_tenant_permit) = other_tenant_permit {
            sessions.release_refresh_attempt_permit(other_tenant_permit).await;
        }

        let Some(first_permit) = first_permit else {
            return;
        };
        sessions.release_refresh_attempt_permit(first_permit).await;

        let later_permit = sessions.try_acquire_refresh_attempt_permit(tenant_id, "sid-1").await;
        assert!(later_permit.is_some());
        if let Some(later_permit) = later_permit {
            sessions.release_refresh_attempt_permit(later_permit).await;
        }
    }
}
