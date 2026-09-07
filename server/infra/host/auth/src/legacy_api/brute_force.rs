use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::{
    extract::{ConnectInfo, Request, State},
    http::HeaderMap,
};
pub use axum::{
    body::Body,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use tokio::sync::{Mutex, MutexGuard, OwnedMutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};
use tokio::time::Interval;
use validator::Validate;

pub use crate::{LegacyAuthService, dto::AuthRequest};
use infra_kernel::request::OriginatorIp;
use infra_redis::RedisAdapter;
use tracing::{error, warn, info, debug, trace};
use crate::account::Role;

#[derive(Clone, Debug)]
pub struct LoginAttemptContext {
    pub ip: Option<String>,
}

#[derive(Clone, Debug)]
pub enum BruteForceReason {
    Username,
    Ip,
    BackendUnavailable,
}

#[derive(Clone, Debug)]
pub struct BruteForceBlock {
    pub retry_after_secs: u64,
    pub reason: BruteForceReason,
}

#[async_trait]
trait BruteForceOps {
    async fn check_login_allowed(&self, username: &str, ip: Option<&str>) -> Result<(), BruteForceBlock>;
    async fn record_failure(&self, username: &str, ip: Option<&str>, role: Option<&Role>) -> BruteForceStatus;
    async fn record_success(&self, username: &str, ip: Option<&str>);
    async fn remaining_attempts(&self, username: &str, ip: Option<&str>, role: Option<&Role>) -> u32;
}

type DynBruteForceOps = Arc<dyn BruteForceOps + Send + Sync>;

#[derive(Clone, Debug)]
pub struct BruteForceStatus {
    pub remaining_attempts: u32,
    pub locked: Option<BruteForceBlock>,
}

pub struct BruteForceGuard {
    ops: DynBruteForceOps,
    // Keep one credential verification in flight per account key so multiple
    // parallel failures cannot all pass the pre-authentication lock check.
    login_attempt_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

struct LoginAttemptPermit {
    key: String,
    lock: Arc<Mutex<()>>,
    _guard: OwnedMutexGuard<()>,
}

#[derive(Clone, Debug)]
struct BruteForcePolicy {
    max_failures: u32,
    failure_window: Duration,
    lockout_duration: Duration,
}

#[derive(Clone, Debug)]
struct BruteForceConfig {
    tenant_owner: BruteForcePolicy,
    supervisor: BruteForcePolicy,
    employee: BruteForcePolicy,
}

impl BruteForceConfig {
    fn from_env() -> Self {
        Self {
            tenant_owner: read_role_policy("TENANT_OWNER", 5, 300, 900),
            supervisor: read_role_policy("SUPERVISOR", 5, 300, 900),
            employee: read_role_policy("EMPLOYEE", 5, 300, 900),
        }
    }

    fn for_role(&self, role: Option<&Role>) -> &BruteForcePolicy {
        match role {
            Some(Role::TenantOwner | Role::ExecutiveManager) => &self.tenant_owner,
            Some(Role::BranchManager | Role::Supervisor) => &self.supervisor,
            Some(Role::Staff) | None => &self.employee,
        }
    }
}

pub fn tenant_login_key(tenant: &str, username: &str) -> String {
    format!("tenant:{}:user:{}", tenant, username.trim().to_lowercase())
}

impl BruteForceGuard {
    pub fn new_arc(redis: Arc<RedisAdapter>) -> Arc<Self> {
        let config: BruteForceConfig = BruteForceConfig::from_env();
        let server: String = std::env::var("BRUTE_FORCE_BACKEND").unwrap_or_else(|_| "dashmap".to_string());
        let ops: DynBruteForceOps = if server == "redis" {
            redis_::BruteForceTracker::new_arc(config, redis) as DynBruteForceOps
        } else {
            memory_::BruteForceTracker::new_arc(config) as DynBruteForceOps
        };

        Arc::new(Self {
            ops,
            login_attempt_locks: Mutex::new(HashMap::new()),
        })
    }

    async fn acquire_login_attempt_permit(&self, username: &str) -> LoginAttemptPermit {
        let key: String = username.to_string();
        let lock: Arc<Mutex<()>> = {
            let mut locks: MutexGuard<HashMap<String, Arc<Mutex<()>>>> = self.login_attempt_locks.lock().await;
            Arc::clone(locks.entry(key.clone()).or_insert_with(|| Arc::new(Mutex::new(()))))
        };
        let guard: OwnedMutexGuard<()> = Arc::clone(&lock).lock_owned().await;
        LoginAttemptPermit {
            key,
            lock,
            _guard: guard,
        }
    }

    async fn release_login_attempt_permit(&self, permit: LoginAttemptPermit) {
        let key: String = permit.key.clone();
        let lock: Arc<Mutex<()>> = Arc::clone(&permit.lock);
        drop(permit);

        let mut locks: MutexGuard<HashMap<String, Arc<Mutex<()>>>> = self.login_attempt_locks.lock().await;
        let can_remove: bool = locks.get(&key).is_some_and(|registered_lock: &Arc<Mutex<()>>| {
            Arc::ptr_eq(registered_lock, &lock) && Arc::strong_count(registered_lock) == 2
        });
        if can_remove {
            locks.remove(&key);
        }
    }

    pub async fn check_login_allowed(&self, username: &str, ip: Option<&str>) -> Result<(), BruteForceBlock> {
        self.ops.check_login_allowed(username, ip).await
    }

    /// Compatibility wrapper for the login middleware.
    pub async fn check_username_block(&self, username: &str) -> Option<u64> {
        self.ops
            .check_login_allowed(username, None)
            .await
            .err()
            .map(|block: BruteForceBlock| block.retry_after_secs)
    }

    pub async fn record_failure_with_ip(
        &self,
        username: &str,
        ip: Option<&str>,
        role: Option<&Role>,
    ) -> BruteForceStatus {
        self.ops.record_failure(username, ip, role).await
    }

    /// Call when login FAIL.
    pub async fn record_failure(&self, username: &str) {
        let _status: BruteForceStatus = self.ops.record_failure(username, None, None).await;
    }

    pub async fn record_success_with_ip(&self, username: &str, ip: Option<&str>) {
        self.ops.record_success(username, ip).await
    }

    /// Call when login SUCCESS: reset username counter.
    pub async fn record_success(&self, username: &str) {
        self.ops.record_success(username, None).await
    }

    pub async fn remaining_attempts_with_ip(&self, username: &str, ip: Option<&str>) -> u32 {
        self.ops.remaining_attempts(username, ip, None).await
    }

    pub async fn remaining_attempts(&self, username: &str) -> u32 {
        self.ops.remaining_attempts(username, None, None).await
    }
}

fn read_role_policy(
    prefix: &str,
    max_failures_default: u32,
    window_default: u64,
    lockout_default: u64,
) -> BruteForcePolicy {
    let max_failures_name: String = format!("{}_BRUTE_FORCE_MAX_FAILURES", prefix);
    let failure_window_name: String = format!("{}_BRUTE_FORCE_WINDOW_SECS", prefix);
    let lockout_duration_name: String = format!("{}_BRUTE_FORCE_LOCKOUT_SECS", prefix);

    BruteForcePolicy {
        max_failures: parse_env_u32(&max_failures_name, max_failures_default),
        failure_window: Duration::from_secs(parse_env_u64(&failure_window_name, window_default)),
        lockout_duration: Duration::from_secs(parse_env_u64(&lockout_duration_name, lockout_default)),
    }
}

fn parse_env_u32(name: &str, default: u32) -> u32 {
    match std::env::var(name) {
        Ok(val) => val.parse().unwrap_or_else(|err: std::num::ParseIntError| {
            warn!("Invalid {} format: {}, using default {}", name, err, default);
            default
        }),
        Err(_) => {
            warn!("{} not set, using default {}", name, default);
            default
        }
    }
}

fn parse_env_u64(name: &str, default: u64) -> u64 {
    match std::env::var(name) {
        Ok(val) => val.parse().unwrap_or_else(|err: std::num::ParseIntError| {
            warn!("Invalid {} format: {}, using default {}", name, err, default);
            default
        }),
        Err(_) => {
            warn!("{} not set, using default {}", name, default);
            default
        }
    }
}

mod memory_ {
    use super::*;

    #[derive(Debug)]
    struct FailRecord {
        count: u32,
        first_failure: Instant,
        failure_window: Duration,
        locked_until: Option<Instant>,
    }

    impl FailRecord {
        fn new(now: Instant, failure_window: Duration) -> Self {
            Self {
                count: 1,
                first_failure: now,
                failure_window,
                locked_until: None,
            }
        }
    }

    pub(super) struct BruteForceTracker {
        /// Tracks failed attempts by scoped key, e.g. username or ip.
        failed_attempts: RwLock<HashMap<String, FailRecord>>,
        config: BruteForceConfig,
    }

    impl BruteForceTracker {
        pub(super) fn new_arc(config: BruteForceConfig) -> Arc<Self> {
            let pself: Arc<BruteForceTracker> = Arc::new(Self {
                failed_attempts: RwLock::new(HashMap::new()),
                config,
            });

            Self::cleanup_monitor(Arc::clone(&pself));
            pself
        }

        fn username_key(username: &str) -> String {
            format!("user:{}", username)
        }

        fn ip_key(ip: &str) -> String {
            format!("ip:{}", ip)
        }

        async fn check_key(&self, key: &str, reason: BruteForceReason) -> Result<(), BruteForceBlock> {
            let map: RwLockReadGuard<HashMap<String, FailRecord>> = self.failed_attempts.read().await;
            if let Some(record) = map.get(key) {
                if let Some(locked_until) = record.locked_until {
                    let now: Instant = Instant::now();
                    if now < locked_until {
                        return Err(BruteForceBlock {
                            retry_after_secs: locked_until.saturating_duration_since(now).as_secs(),
                            reason,
                        });
                    }
                }
            }
            Ok(())
        }

        async fn record_failure_key(
            &self,
            key: &str,
            reason: BruteForceReason,
            policy: &BruteForcePolicy,
        ) -> BruteForceStatus {
            let mut map: RwLockWriteGuard<HashMap<String, FailRecord>> = self.failed_attempts.write().await;
            let now: Instant = Instant::now();
            let entry: std::collections::hash_map::Entry<String, FailRecord> = map.entry(key.to_string());
            let record: &mut FailRecord = match entry {
                std::collections::hash_map::Entry::Occupied(occupied) => {
                    let record: &mut FailRecord = occupied.into_mut();
                    if now.duration_since(record.first_failure) > policy.failure_window {
                        *record = FailRecord::new(now, policy.failure_window);
                    } else {
                        record.count += 1;
                        record.failure_window = policy.failure_window;
                    }
                    record
                }
                std::collections::hash_map::Entry::Vacant(vacant) => {
                    vacant.insert(FailRecord::new(now, policy.failure_window))
                }
            };

            let remaining_attempts: u32 = policy.max_failures.saturating_sub(record.count);
            if record.count >= policy.max_failures {
                let locked_until: Instant = now + policy.lockout_duration;
                record.locked_until = Some(locked_until);
                warn!("Brute force key locked: key={} count={}", key, record.count);
                BruteForceStatus {
                    remaining_attempts,
                    locked: Some(BruteForceBlock {
                        retry_after_secs: policy.lockout_duration.as_secs(),
                        reason,
                    }),
                }
            } else {
                BruteForceStatus {
                    remaining_attempts,
                    locked: None,
                }
            }
        }

        async fn remaining_attempts_key(&self, key: &str, policy: &BruteForcePolicy) -> u32 {
            let map: RwLockReadGuard<HashMap<String, FailRecord>> = self.failed_attempts.read().await;
            map.get(key)
                .map(|record: &FailRecord| policy.max_failures.saturating_sub(record.count))
                .unwrap_or(policy.max_failures)
        }

        fn cleanup_monitor(pself: Arc<Self>) {
            tokio::spawn(async move {
                let mut ticker: Interval = tokio::time::interval(Duration::from_secs(300));
                loop {
                    ticker.tick().await;

                    let mut map: RwLockWriteGuard<HashMap<String, FailRecord>> = pself.failed_attempts.write().await;
                    let now: Instant = Instant::now();
                    map.retain(|_key: &String, record: &mut FailRecord| match record.locked_until {
                        Some(until) => now < until,
                        None => now.duration_since(record.first_failure) < record.failure_window,
                    });

                    debug!("BruteForce cleanup: {} records", map.len());
                }
            });
        }
    }

    #[async_trait]
    impl BruteForceOps for BruteForceTracker {
        async fn check_login_allowed(&self, username: &str, ip: Option<&str>) -> Result<(), BruteForceBlock> {
            // Intentionally enforce brute-force protection by username only. This prevents attackers from
            // bypassing the limit by rotating IPs, devices, or clients against the same account.
            let _ip: Option<&str> = ip;
            self.check_key(&Self::username_key(username), BruteForceReason::Username)
                .await
        }

        async fn record_failure(&self, username: &str, ip: Option<&str>, role: Option<&Role>) -> BruteForceStatus {
            // Intentionally record the failure against the username only. The username counter is global
            // across every IP/client in this process.
            let _ip: Option<&str> = ip;
            let policy: BruteForcePolicy = self.config.for_role(role).clone();
            self.record_failure_key(&Self::username_key(username), BruteForceReason::Username, &policy)
                .await
        }

        async fn record_success(&self, username: &str, ip: Option<&str>) {
            // Successful login clears the global username counter only.
            let _ip: Option<&str> = ip;
            let mut map: RwLockWriteGuard<HashMap<String, FailRecord>> = self.failed_attempts.write().await;
            map.remove(&Self::username_key(username));
        }

        async fn remaining_attempts(&self, username: &str, ip: Option<&str>, role: Option<&Role>) -> u32 {
            // Report the account-wide remaining attempts; IP is not part of the limit key.
            let _ip: Option<&str> = ip;
            self.remaining_attempts_key(&Self::username_key(username), self.config.for_role(role))
                .await
        }
    }
}

mod redis_ {
    use super::*;
    use redis::AsyncCommands;
    use redis::{aio::MultiplexedConnection, RedisResult};

    pub(super) struct BruteForceTracker {
        redis_pool: Arc<RedisAdapter>,
        config: BruteForceConfig,
        key_prefix: String,
    }

    impl BruteForceTracker {
        pub(super) fn new_arc(config: BruteForceConfig, redis_pool: Arc<RedisAdapter>) -> Arc<Self> {
            let key_prefix: String =
                std::env::var("BRUTE_FORCE_REDIS_PREFIX").unwrap_or_else(|_| "infra:bruteforce:".to_string());
            info!("BruteForce Redis server initialized with key_prefix={}", key_prefix);
            Arc::new(Self {
                redis_pool,
                config,
                key_prefix,
            })
        }

        fn username_fail_key(&self, username: &str) -> String {
            format!("{}fail:user:{}", self.key_prefix, username)
        }

        fn username_lock_key(&self, username: &str) -> String {
            format!("{}lock:user:{}", self.key_prefix, username)
        }

        fn ip_fail_key(&self, ip: &str) -> String {
            format!("{}fail:ip:{}", self.key_prefix, ip)
        }

        fn ip_lock_key(&self, ip: &str) -> String {
            format!("{}lock:ip:{}", self.key_prefix, ip)
        }

        fn backend_unavailable_block() -> BruteForceBlock {
            BruteForceBlock {
                retry_after_secs: 0,
                reason: BruteForceReason::BackendUnavailable,
            }
        }

        fn backend_unavailable_status() -> BruteForceStatus {
            BruteForceStatus {
                remaining_attempts: 0,
                locked: Some(Self::backend_unavailable_block()),
            }
        }

        async fn check_lock_key(&self, key: &str, reason: BruteForceReason) -> Result<(), BruteForceBlock> {
            let connection: RedisResult<MultiplexedConnection> = self.redis_pool.connection().await;
            let mut connection: MultiplexedConnection = match connection {
                Ok(connection) => connection,
                Err(err) => {
                    error!("Failed to connect to Redis while checking brute force lock: {}", err);
                    return Err(Self::backend_unavailable_block());
                }
            };

            let ttl: RedisResult<i64> = connection.ttl(key).await;
            match ttl {
                Ok(ttl) if ttl > 0 => Err(BruteForceBlock {
                    retry_after_secs: ttl as u64,
                    reason,
                }),
                Ok(_) => Ok(()),
                Err(err) => {
                    error!("Failed to read Redis brute force lock ttl: key={} error={}", key, err);
                    Err(Self::backend_unavailable_block())
                }
            }
        }

        async fn record_failure_key(
            &self,
            fail_key: &str,
            lock_key: &str,
            reason: BruteForceReason,
            policy: &BruteForcePolicy,
        ) -> BruteForceStatus {
            let connection: RedisResult<MultiplexedConnection> = self.redis_pool.connection().await;
            let mut connection: MultiplexedConnection = match connection {
                Ok(connection) => connection,
                Err(err) => {
                    error!(
                        "Failed to connect to Redis while recording brute force failure: {}",
                        err
                    );
                    return Self::backend_unavailable_status();
                }
            };

            let script: &str = r#"
                local fail_key = KEYS[1]
                local lock_key = KEYS[2]
                local max_failures = tonumber(ARGV[1])
                local window_secs = tonumber(ARGV[2])
                local lockout_secs = tonumber(ARGV[3])

                local lock_ttl = redis.call('TTL', lock_key)
                if lock_ttl and lock_ttl > 0 then
                    return {max_failures, lock_ttl}
                end

                local count = redis.call('INCR', fail_key)
                if count == 1 then
                    redis.call('EXPIRE', fail_key, window_secs)
                end

                if count >= max_failures then
                    redis.call('SET', lock_key, '1', 'EX', lockout_secs)
                    redis.call('DEL', fail_key)
                    return {count, lockout_secs}
                end

                return {count, 0}
            "#;

            let result: RedisResult<(u32, u64)> = redis::cmd("EVAL")
                .arg(script)
                .arg(2)
                .arg(fail_key)
                .arg(lock_key)
                .arg(policy.max_failures)
                .arg(policy.failure_window.as_secs())
                .arg(policy.lockout_duration.as_secs())
                .query_async(&mut connection)
                .await;

            match result {
                Ok((count, retry_after_secs)) => {
                    let remaining_attempts: u32 = policy.max_failures.saturating_sub(count);
                    if retry_after_secs > 0 {
                        warn!("Brute force key locked in Redis: key={} count={}", lock_key, count);
                        BruteForceStatus {
                            remaining_attempts,
                            locked: Some(BruteForceBlock {
                                retry_after_secs,
                                reason,
                            }),
                        }
                    } else {
                        BruteForceStatus {
                            remaining_attempts,
                            locked: None,
                        }
                    }
                }
                Err(err) => {
                    error!(
                        "Failed to record Redis brute force failure: fail_key={} lock_key={} error={}",
                        fail_key, lock_key, err
                    );
                    Self::backend_unavailable_status()
                }
            }
        }

        async fn record_success_key(&self, fail_key: &str, lock_key: &str) {
            let connection: RedisResult<MultiplexedConnection> = self.redis_pool.connection().await;
            let mut connection: MultiplexedConnection = match connection {
                Ok(connection) => connection,
                Err(err) => {
                    error!("Failed to connect to Redis while clearing brute force records: {}", err);
                    return;
                }
            };

            let result: RedisResult<()> = redis::pipe()
                .atomic()
                .cmd("DEL")
                .arg(fail_key)
                .ignore()
                .cmd("DEL")
                .arg(lock_key)
                .ignore()
                .query_async(&mut connection)
                .await;
            if let Err(err) = result {
                error!("Failed to clear Redis brute force records: error={}", err);
            }
        }

        async fn remaining_attempts_key(&self, fail_key: &str, lock_key: &str, policy: &BruteForcePolicy) -> u32 {
            let connection: RedisResult<MultiplexedConnection> = self.redis_pool.connection().await;
            let mut connection: MultiplexedConnection = match connection {
                Ok(connection) => connection,
                Err(err) => {
                    error!("Failed to connect to Redis while reading brute force attempts: {}", err);
                    return policy.max_failures;
                }
            };

            let lock_ttl: RedisResult<i64> = connection.ttl(lock_key).await;
            if matches!(lock_ttl, Ok(ttl) if ttl > 0) {
                return 0;
            }

            let count: RedisResult<Option<u32>> = connection.get(fail_key).await;
            match count {
                Ok(Some(count)) => policy.max_failures.saturating_sub(count),
                Ok(None) => policy.max_failures,
                Err(err) => {
                    error!(
                        "Failed to read Redis brute force attempts: key={} error={}",
                        fail_key, err
                    );
                    policy.max_failures
                }
            }
        }
    }

    #[async_trait]
    impl BruteForceOps for BruteForceTracker {
        async fn check_login_allowed(&self, username: &str, ip: Option<&str>) -> Result<(), BruteForceBlock> {
            // Intentionally enforce brute-force protection by username only. This prevents attackers from
            // bypassing the limit by rotating IPs, devices, or clients against the same account.
            let _ip: Option<&str> = ip;
            self.check_lock_key(&self.username_lock_key(username), BruteForceReason::Username)
                .await
        }

        async fn record_failure(&self, username: &str, ip: Option<&str>, role: Option<&Role>) -> BruteForceStatus {
            // Intentionally record the failure against the username only. Redis keys are shared by all
            // host instances, so the username counter is global across every IP/client and process.
            let _ip: Option<&str> = ip;
            let policy: BruteForcePolicy = self.config.for_role(role).clone();
            self.record_failure_key(
                &self.username_fail_key(username),
                &self.username_lock_key(username),
                BruteForceReason::Username,
                &policy,
            )
            .await
        }

        async fn record_success(&self, username: &str, ip: Option<&str>) {
            // Successful login clears the global username counter only.
            let _ip: Option<&str> = ip;
            self.record_success_key(&self.username_fail_key(username), &self.username_lock_key(username))
                .await;
        }

        async fn remaining_attempts(&self, username: &str, ip: Option<&str>, role: Option<&Role>) -> u32 {
            // Report the account-wide remaining attempts; IP is not part of the limit key.
            let _ip: Option<&str> = ip;
            self.remaining_attempts_key(
                &self.username_fail_key(username),
                &self.username_lock_key(username),
                self.config.for_role(role),
            )
            .await
        }
    }
}

pub async fn brute_force_guard_layer(
    State(auth_ctx): State<Arc<LegacyAuthService>>,
    req: Request,
    next: Next,
) -> Response {
    use axum::body::Bytes;

    let ip: Option<String> = req
        .extensions()
        .get::<OriginatorIp>()
        .map(|originator: &OriginatorIp| originator.ip().to_string());

    let (parts, body) = req.into_parts();

    let bytes: Bytes = match axum::body::to_bytes(body, 1024 * 5).await {
        Ok(b) => b,
        Err(_err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_body",
                })),
            )
                .into_response();
        }
    };

    let payload: AuthRequest = match serde_json::from_slice::<AuthRequest>(&bytes) {
        Ok(p) => p,
        Err(_err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_body",
                })),
            )
                .into_response();
        }
    };
    if payload.validate().is_err() || !payload.username_is_valid() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_login_payload",
            })),
        )
            .into_response();
    }

    let tenant = match payload.normalized_tenant() {
        Some(tenant) => tenant,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid_tenant" })),
            )
                .into_response();
        }
    };
    let login_key: String = tenant_login_key(&tenant, &payload.username);
    let login_attempt_permit: LoginAttemptPermit = auth_ctx.brute_force.acquire_login_attempt_permit(&login_key).await;
    let allowed: Result<(), BruteForceBlock> = auth_ctx
        .brute_force
        .check_login_allowed(&login_key, ip.as_deref())
        .await;

    if let Err(block) = allowed {
        warn!(
            "Login blocked: username={} ip={:?} retry_after={}s reason={:?}",
            login_key, ip, block.retry_after_secs, block.reason
        );
        if matches!(&block.reason, BruteForceReason::BackendUnavailable) {
            auth_ctx
                .brute_force
                .release_login_attempt_permit(login_attempt_permit)
                .await;
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "authentication_protection_unavailable",
                })),
            )
                .into_response();
        }
        auth_ctx
            .brute_force
            .release_login_attempt_permit(login_attempt_permit)
            .await;
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error":       "account_locked",
                "message":     "Too many failed attempts",
                "retry_after": block.retry_after_secs,
            })),
        )
            .into_response();
    }

    let mut req: Request<Body> = Request::from_parts(parts, Body::from(bytes));
    req.extensions_mut().insert(payload);
    req.extensions_mut().insert(LoginAttemptContext { ip });

    let response: Response = next.run(req).await;
    auth_ctx
        .brute_force
        .release_login_attempt_permit(login_attempt_permit)
        .await;
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> BruteForceConfig {
        BruteForceConfig {
            tenant_owner: BruteForcePolicy {
                max_failures: 3,
                failure_window: Duration::from_secs(60),
                lockout_duration: Duration::from_secs(120),
            },
            supervisor: BruteForcePolicy {
                max_failures: 3,
                failure_window: Duration::from_secs(60),
                lockout_duration: Duration::from_secs(120),
            },
            employee: BruteForcePolicy {
                max_failures: 3,
                failure_window: Duration::from_secs(60),
                lockout_duration: Duration::from_secs(120),
            },
        }
    }

    #[tokio::test]
    async fn locks_username_across_different_ips() {
        let store: Arc<memory_::BruteForceTracker> = memory_::BruteForceTracker::new_arc(test_config());

        let first_status: BruteForceStatus = store.record_failure("alice", Some("10.0.0.1"), None).await;
        assert_eq!(first_status.remaining_attempts, 2);
        assert!(first_status.locked.is_none());

        let second_status: BruteForceStatus = store.record_failure("alice", Some("10.0.0.2"), None).await;
        assert_eq!(second_status.remaining_attempts, 1);
        assert!(second_status.locked.is_none());

        let allowed_before_limit: Result<(), BruteForceBlock> =
            store.check_login_allowed("alice", Some("10.0.0.3")).await;
        assert!(allowed_before_limit.is_ok());

        let third_status: BruteForceStatus = store.record_failure("alice", Some("10.0.0.3"), None).await;
        assert_eq!(third_status.remaining_attempts, 0);
        assert!(third_status.locked.is_some());

        let blocked_from_new_ip: Result<(), BruteForceBlock> =
            store.check_login_allowed("alice", Some("10.0.0.4")).await;
        assert!(blocked_from_new_ip.is_err());

        let other_user_same_ip: Result<(), BruteForceBlock> = store.check_login_allowed("bob", Some("10.0.0.4")).await;
        assert!(other_user_same_ip.is_ok());
    }

    #[tokio::test]
    async fn success_from_any_ip_clears_username_counter() {
        let store: Arc<memory_::BruteForceTracker> = memory_::BruteForceTracker::new_arc(test_config());

        let first_status: BruteForceStatus = store.record_failure("alice", Some("10.0.0.1"), None).await;
        assert_eq!(first_status.remaining_attempts, 2);

        store.record_success("alice", Some("10.0.0.2")).await;

        let remaining: u32 = store.remaining_attempts("alice", Some("10.0.0.3"), None).await;
        assert_eq!(remaining, 3);
    }

    #[tokio::test]
    async fn same_username_in_different_tenants_has_independent_limits() {
        let store: Arc<memory_::BruteForceTracker> = memory_::BruteForceTracker::new_arc(test_config());
        let first_tenant = tenant_login_key("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", "alice");
        let second_tenant = tenant_login_key("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", "alice");

        for _attempt in 0..3 {
            store.record_failure(&first_tenant, None, None).await;
        }

        assert!(store.check_login_allowed(&first_tenant, None).await.is_err());
        assert!(store.check_login_allowed(&second_tenant, None).await.is_ok());
    }

    #[tokio::test]
    async fn serializes_parallel_attempts_for_one_username() {
        let guard: Arc<BruteForceGuard> = Arc::new(BruteForceGuard {
            ops: memory_::BruteForceTracker::new_arc(test_config()) as DynBruteForceOps,
            login_attempt_locks: Mutex::new(HashMap::new()),
        });
        let first_permit: LoginAttemptPermit = guard.acquire_login_attempt_permit("alice").await;
        let waiting_guard: Arc<BruteForceGuard> = Arc::clone(&guard);
        let mut waiting_attempt: tokio::task::JoinHandle<()> = tokio::spawn(async move {
            let permit: LoginAttemptPermit = waiting_guard.acquire_login_attempt_permit("alice").await;
            waiting_guard.release_login_attempt_permit(permit).await;
        });

        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut waiting_attempt)
                .await
                .is_err()
        );
        guard.release_login_attempt_permit(first_permit).await;
        assert!(
            tokio::time::timeout(Duration::from_secs(1), waiting_attempt)
                .await
                .is_ok()
        );
    }

    #[test]
    fn selects_role_policy_and_uses_user_policy_for_unknown_accounts() {
        let config: BruteForceConfig = BruteForceConfig {
            tenant_owner: BruteForcePolicy {
                max_failures: 2,
                failure_window: Duration::from_secs(20),
                lockout_duration: Duration::from_secs(200),
            },
            supervisor: BruteForcePolicy {
                max_failures: 3,
                failure_window: Duration::from_secs(30),
                lockout_duration: Duration::from_secs(300),
            },
            employee: BruteForcePolicy {
                max_failures: 4,
                failure_window: Duration::from_secs(40),
                lockout_duration: Duration::from_secs(400),
            },
        };

        assert_eq!(config.for_role(Some(&Role::TenantOwner)).max_failures, 2);
        assert_eq!(config.for_role(Some(&Role::Supervisor)).max_failures, 3);
        assert_eq!(config.for_role(Some(&Role::Employee)).max_failures, 4);
        assert_eq!(config.for_role(None).lockout_duration, Duration::from_secs(400));
    }
}
