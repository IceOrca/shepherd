use std::sync::Arc;

use infra_postgres::DatabaseAdapter;
use infra_redis::RedisAdapter;
use uuid::Uuid;

use crate::{
    AuthMngtEntity, AuthProvider, DynAccountRepo, access_revocation::AccessRevocationCache, account::Role,
    bruteforce::BruteForceGuard, dto::AccessClaims, jwt::JwtHandle, session::AuthSessionHandle,
};

/// Verified tenant identity made available to tenant-aware request handlers.
#[derive(Clone, Debug)]
pub struct TenantContext {
    pub id: Uuid,
}

#[derive(Clone, Debug)]
pub struct AuthenticatedUser {
    pub tenant_id: Uuid,
    pub account_id: Uuid,
    pub username: String,
    pub role: Role,
    pub roles: Vec<String>,
    pub auth_version: i64,
    pub permissions: Vec<String>,
    pub sid: String,
    pub jti: String,
    pub exp: u64,
}

impl AuthenticatedUser {
    pub fn from_claims(claims: &AccessClaims) -> Result<Self, ()> {
        let tenant_id: Uuid = Uuid::parse_str(&claims.tid).map_err(|_| ())?;
        let account_id: Uuid = Uuid::parse_str(&claims.sub).map_err(|_| ())?;
        if tenant_id.is_nil()
            || account_id.is_nil()
            || Uuid::parse_str(&claims.jti).is_err()
            || claims.sid.len() != 32
            || !claims.sid.bytes().all(|byte: u8| byte.is_ascii_hexdigit())
            || claims.username.trim() != claims.username
            || !(3..=128).contains(&claims.username.len())
            || claims.exp <= claims.iat
            || claims.nbf > claims.iat
            || claims.ver < 1
            || claims.roles.is_empty()
            || claims.roles.len() > 64
            || !claims.roles.iter().any(|role: &String| role == claims.role.as_code())
            || claims.permissions.len() > 256
            || claims
                .permissions
                .iter()
                .any(|permission: &String| permission.is_empty() || permission.len() > 160)
        {
            return Err(());
        }

        Ok(Self {
            tenant_id,
            account_id,
            username: claims.username.clone(),
            role: claims.role.clone(),
            roles: claims.roles.clone(),
            auth_version: claims.ver,
            permissions: claims.permissions.clone(),
            sid: claims.sid.clone(),
            jti: claims.jti.clone(),
            exp: claims.exp as u64,
        })
    }

    pub fn has_permission(&self, permission_code: &str) -> bool {
        self.permissions
            .iter()
            .any(|permission: &String| permission == permission_code)
    }
}

pub struct AuthService {
    pub core_entity: Arc<AuthMngtEntity>,
    pub jwt: JwtHandle,
    pub sessions: Arc<AuthSessionHandle>,
    pub access_revocation: Arc<AccessRevocationCache>,
    pub brute_force: Arc<BruteForceGuard>,
}

impl AuthService {
    pub fn new_arc(core_entity: Arc<AuthMngtEntity>, redis: Arc<RedisAdapter>) -> Arc<Self> {
        Arc::new(Self {
            core_entity,
            jwt: JwtHandle::new(
                &std::env::var("JWT_PRIVATE_KEY_PATH").unwrap_or_else(|_| {
                    panic!("JWT_PRIVATE_KEY_PATH not set, cannot initialize JWT handle");
                }),
                &std::env::var("JWT_PUBLIC_KEY_PATH").unwrap_or_else(|_| {
                    panic!("JWT_PUBLIC_KEY_PATH not set, cannot initialize JWT handle");
                }),
            ),
            sessions: AuthSessionHandle::new_arc(Arc::clone(&redis)),
            access_revocation: AccessRevocationCache::new_arc(),
            brute_force: BruteForceGuard::new_arc(redis),
        })
    }

    pub async fn init(&self) {}

    /// Build the complete auth service from reusable infrastructure adapters.
    pub async fn from_adapters(database: Arc<DatabaseAdapter>, redis: Arc<RedisAdapter>) -> Arc<Self> {
        let repository: Arc<AuthProvider> = AuthProvider::new_arc(database);
        let core_entity: Arc<AuthMngtEntity> = AuthMngtEntity::new_arc(repository as DynAccountRepo).await;
        let service: Arc<Self> = Self::new_arc(core_entity, redis);
        service.init().await;
        service
    }
}

/// Authentication feature assembled from its domain, persistence, and web adapters.
#[derive(Clone)]
pub struct AuthFeature {
    pub service: Arc<AuthService>,
}

impl AuthFeature {
    pub async fn new_arc(database: Arc<DatabaseAdapter>, redis: Arc<RedisAdapter>) -> Arc<Self> {
        let service: Arc<AuthService> = AuthService::from_adapters(database, redis).await;
        Arc::new(Self { service })
    }
}

#[cfg(test)]
mod tests {
    use crate::account::Role;
    use uuid::Uuid;

    use super::AuthenticatedUser;
    use crate::dto::AccessClaims;

    fn valid_claims() -> AccessClaims {
        AccessClaims {
            sub: Uuid::new_v4().to_string(),
            tid: Uuid::new_v4().to_string(),
            iss: "infra".to_owned(),
            aud: "infra-api".to_owned(),
            exp: 2_000,
            nbf: 1_000,
            iat: 1_000,
            jti: Uuid::new_v4().to_string(),
            sid: Uuid::new_v4().simple().to_string(),
            username: "alice".to_owned(),
            role: Role::Employee,
            roles: vec!["employee".to_owned()],
            ver: 1,
            permissions: vec!["auth.accounts.read".to_owned()],
        }
    }

    #[test]
    fn accepts_complete_access_claim_identity() {
        assert!(AuthenticatedUser::from_claims(&valid_claims()).is_ok());
    }

    #[test]
    fn rejects_empty_or_malformed_access_claim_identifiers() {
        let mut missing_sid: AccessClaims = valid_claims();
        missing_sid.sid.clear();
        assert!(AuthenticatedUser::from_claims(&missing_sid).is_err());

        let mut malformed_jti: AccessClaims = valid_claims();
        malformed_jti.jti = "not-a-jti".to_owned();
        assert!(AuthenticatedUser::from_claims(&malformed_jti).is_err());
    }

    #[test]
    fn rejects_invalid_access_claim_time_ordering() {
        let mut claims: AccessClaims = valid_claims();
        claims.exp = claims.iat;
        assert!(AuthenticatedUser::from_claims(&claims).is_err());
    }
}
