use std::{sync::Arc, time::Instant};
use std::time::Duration;
use jsonwebtoken::{
    Algorithm, DecodingKey, Validation, decode, decode_header,
    jwk::{Jwk, JwkSet, KeyOperations, PublicKeyUse},
};
use tokio::sync::{RwLockReadGuard, MutexGuard};
use tokio::sync::{Mutex, RwLock};
use reqwest::Client;
use tracing::{debug, error, info, trace, warn};

use super::{AccessTokenClaims, AccessTokenErr, AuthedPrincipal, OidcJwksVerifierCfg};

const UNKNOWN_KID_REFRESH_COOLDOWN_SECS: u64 = 10;

#[derive(Default)]
struct CachedJwks {
    set: JwkSet,
    fetched_at: Option<Instant>,
}

/// Validates access tokens locally and refreshes provider signing keys on a
/// bounded interval or immediately when the provider rotates to an unknown KID.
pub struct OidcJwksVerifier {
    config: OidcJwksVerifierCfg,
    client: reqwest::Client,
    jwks: RwLock<CachedJwks>,
    refresh_guard: Mutex<()>,
}

impl OidcJwksVerifier {
    pub async fn from_env() -> Result<Arc<Self>, AccessTokenErr> {
        debug!("Loading external identity provider configuration");
        Self::from_config(OidcJwksVerifierCfg::from_env()?).await
    }

    pub async fn from_config(config: OidcJwksVerifierCfg) -> Result<Arc<Self>, AccessTokenErr> {
        info!(
            issuer = %config.issuer,
            audience = %config.audience,
            algorithm_count = config.allowed_algs.len(),
            "Initializing external identity provider"
        );
        let client: reqwest::Client = reqwest::Client::builder()
            .timeout(config.http_timeout)
            .build()
            .map_err(|error: reqwest::Error| {
                error!(error = %error, "External identity provider HTTP client initialization failed");
                AccessTokenErr::JwksUnavailable(error)
            })?;
        let service: Arc<OidcJwksVerifier> = Arc::new(Self {
            config,
            client,
            jwks: RwLock::new(CachedJwks::default()),
            refresh_guard: Mutex::new(()),
        });
        service.refresh_jwks(false).await?;
        info!("External identity provider initialized");
        Ok(service)
    }

    pub fn config(&self) -> &OidcJwksVerifierCfg {
        &self.config
    }

    pub async fn validate_access_token(&self, token: &str) -> Result<AuthedPrincipal, AccessTokenErr> {
        trace!("External access-token validation accepted");
        let header: jsonwebtoken::Header =
            jsonwebtoken::decode_header(token).map_err(|error: jsonwebtoken::errors::Error| {
                warn!(error = %error, "External access token header is invalid");
                AccessTokenErr::InvalidToken(error)
            })?;
        if !self.config.allowed_algs.contains(&header.alg) {
            warn!(alg = ?header.alg, "External access token uses a disallowed signing alg");
            return Err(AccessTokenErr::DisallowedAlgorithm);
        }
        let kid: &str = header.kid.as_deref().ok_or_else(|| {
            warn!("External access token is missing its signing key identifier");
            AccessTokenErr::MissingKeyId
        })?;
        debug!(kid, alg = ?header.alg, "Resolving external access-token signing key");
        let key: DecodingKey = self.decoding_key(kid, header.alg).await?;

        let mut validation: Validation = Validation::new(header.alg);
        validation.set_issuer(&[&self.config.issuer]);
        validation.set_audience(&[&self.config.audience]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        validation.validate_exp = true;
        validation.validate_nbf = true;
        validation.validate_aud = true;
        validation.leeway = self.config.clock_skew.as_secs();

        let token: jsonwebtoken::TokenData<AccessTokenClaims> =
            jsonwebtoken::decode::<AccessTokenClaims>(token, &key, &validation).map_err(
                |err: jsonwebtoken::errors::Error| {
                    warn!(kid, err = %err, "External access token signature or claims are invalid");
                    AccessTokenErr::InvalidToken(err)
                },
            )?;
        let principal: AuthedPrincipal = AuthedPrincipal::try_from(token.claims)?;
        if principal
            .issued_at
            .is_some_and(|issued_at: u64| issued_at > jsonwebtoken::get_current_timestamp() + validation.leeway)
        {
            warn!(subject = %principal.subject, "External access token issued-at claim is in the future");
            return Err(AccessTokenErr::InvalidClaims(
                "issued-at is unreasonably far in the future".to_owned(),
            ));
        }
        debug!(subject = %principal.subject, "External access token validated");
        Ok(principal)
    }

    async fn decoding_key(&self, kid: &str, alg: Algorithm) -> Result<DecodingKey, AccessTokenErr> {
        let (cached_jwk, cache_is_fresh): (Option<Jwk>, bool) = {
            let cache: RwLockReadGuard<CachedJwks> = self.jwks.read().await;
            let is_fresh: bool = cache
                .fetched_at
                .is_some_and(|fetched_at: Instant| fetched_at.elapsed() < self.config.jwks_refresh_interval);
            (cache.set.find(kid).cloned(), is_fresh)
        };
        if cache_is_fresh && let Some(jwk) = &cached_jwk {
            trace!(kid, "Using fresh cached external signing key");
            return decoding_key_from_jwk(jwk, alg);
        }
        debug!(
            kid,
            key_was_cached = cached_jwk.is_some(),
            "Refreshing external signing keys"
        );

        match self.refresh_jwks(cached_jwk.is_none()).await {
            Ok(()) => self
                .jwks
                .read()
                .await
                .set
                .find(kid)
                .cloned()
                .ok_or(AccessTokenErr::UnknownKey)
                .and_then(|jwk: Jwk| decoding_key_from_jwk(&jwk, alg)),
            Err(error) => {
                if let Some(jwk) = cached_jwk {
                    warn!(error = %error, kid, "Using stale provider signing key after JWKS refresh failed");
                    decoding_key_from_jwk(&jwk, alg)
                } else {
                    error!(error = %error, kid, "No provider signing key is available after JWKS refresh failed");
                    Err(error)
                }
            }
        }
    }

    async fn refresh_jwks(&self, force: bool) -> Result<(), AccessTokenErr> {
        let _refresh_guard: MutexGuard<()> = self.refresh_guard.lock().await;
        let cache_age: Option<Duration> = self
            .jwks
            .read()
            .await
            .fetched_at
            .map(|fetched_at: Instant| fetched_at.elapsed());
        let cache_is_fresh: bool = cache_age.is_some_and(|age: Duration| age < self.config.jwks_refresh_interval);
        let unknown_kid_refresh_is_throttled: bool =
            force && cache_age.is_some_and(|age: Duration| age.as_secs() < UNKNOWN_KID_REFRESH_COOLDOWN_SECS);
        if (cache_is_fresh && !force) || unknown_kid_refresh_is_throttled {
            trace!(
                force,
                cache_is_fresh, unknown_kid_refresh_is_throttled, "External signing-key refresh skipped"
            );
            return Ok(());
        }

        debug!(force, "Fetching external signing keys");
        let set: JwkSet = self
            .client
            .get(&self.config.jwks_url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(AccessTokenErr::JwksUnavailable)?
            .json::<JwkSet>()
            .await
            .map_err(AccessTokenErr::JwksUnavailable)?;
        if set.keys.is_empty() {
            error!("External identity provider returned an empty signing-key set");
            return Err(AccessTokenErr::EmptyJwks);
        }
        let key_count: usize = set.keys.len();

        *self.jwks.write().await = CachedJwks {
            set,
            fetched_at: Some(Instant::now()),
        };
        info!(key_count, force, "External signing-key cache refreshed");
        Ok(())
    }
}

#[cfg(test)]
impl OidcJwksVerifier {
    fn with_jwks(config: OidcJwksVerifierCfg, set: JwkSet) -> Self {
        let client: reqwest::Client = reqwest::Client::builder()
            .timeout(config.http_timeout)
            .build()
            .expect("test HTTP client");
        Self {
            config,
            client,
            jwks: RwLock::new(CachedJwks {
                set,
                fetched_at: Some(Instant::now()),
            }),
            refresh_guard: Mutex::new(()),
        }
    }
}

fn decoding_key_from_jwk(jwk: &Jwk, alg: Algorithm) -> Result<DecodingKey, AccessTokenErr> {
    if jwk
        .common
        .public_key_use
        .as_ref()
        .is_some_and(|key_use: &PublicKeyUse| !matches!(key_use, PublicKeyUse::Signature))
        || jwk
            .common
            .key_operations
            .as_ref()
            .is_some_and(|operations: &Vec<KeyOperations>| {
                !operations
                    .iter()
                    .any(|operation: &KeyOperations| matches!(operation, KeyOperations::Verify))
            })
    {
        return Err(AccessTokenErr::DisallowedAlgorithm);
    }

    if let Some(key_algorithm) = jwk.common.key_algorithm {
        let key_algorithm: Algorithm = Algorithm::try_from(key_algorithm).map_err(AccessTokenErr::InvalidSigningKey)?;
        if key_algorithm != alg {
            return Err(AccessTokenErr::DisallowedAlgorithm);
        }
    }

    DecodingKey::from_jwk(jwk).map_err(AccessTokenErr::InvalidSigningKey)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, encode, jwk::JwkSet};
    use serde::Serialize;

    use super::OidcJwksVerifier;
    use crate::ext_service::OidcJwksVerifierCfg;

    const ISSUER: &str = "https://identity.example/auth/v1";
    const AUDIENCE: &str = "authenticated";
    const KID: &str = "provider-test-key";

    #[derive(Serialize)]
    struct TestClaims<'a> {
        iss: &'a str,
        sub: &'a str,
        aud: &'a str,
        exp: u64,
        iat: u64,
        preferred_username: &'a str,
        tid: Option<uuid::Uuid>,
    }

    fn config() -> OidcJwksVerifierCfg {
        OidcJwksVerifierCfg::new(
            ISSUER.to_owned(),
            AUDIENCE.to_owned(),
            "https://identity.example/auth/v1/.well-known/jwks.json".to_owned(),
            vec![Algorithm::EdDSA],
            Duration::from_secs(300),
            Duration::from_secs(5),
            Duration::from_secs(0),
        )
        .expect("test configuration")
    }

    fn service() -> OidcJwksVerifier {
        let decoding_key = DecodingKey::from_ed_pem(include_bytes!("../../../../security/jwtkey_dev/jwt_public.pem"))
            .expect("test public key");
        let mut jwk =
            jsonwebtoken::jwk::Jwk::from_decoding_key(&decoding_key, Some(Algorithm::EdDSA)).expect("test JWK");
        jwk.common.key_id = Some(KID.to_owned());
        OidcJwksVerifier::with_jwks(config(), JwkSet { keys: vec![jwk] })
    }

    fn token(audience: &str) -> String {
        let now = jsonwebtoken::get_current_timestamp();
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(KID.to_owned());
        encode(
            &header,
            &TestClaims {
                iss: ISSUER,
                sub: "provider-subject",
                aud: audience,
                exp: now + 300,
                iat: now,
                preferred_username: "alice",
                tid: Some(uuid::uuid!("018f4c6d-7e41-7b89-a4fd-0f8efcc57e31")),
            },
            &EncodingKey::from_ed_pem(include_bytes!("../../../../security/jwtkey_dev/jwt_private.pem"))
                .expect("test private key"),
        )
        .expect("signed test token")
    }

    #[tokio::test]
    async fn validates_signature_issuer_audience_and_subject() {
        let principal = service()
            .validate_access_token(&token(AUDIENCE))
            .await
            .expect("valid access token");

        assert_eq!(principal.issuer, ISSUER);
        assert_eq!(principal.subject, "provider-subject");
        assert_eq!(principal.username.as_deref(), Some("alice"));
        assert_eq!(
            principal.tenant_id,
            Some(uuid::uuid!("018f4c6d-7e41-7b89-a4fd-0f8efcc57e31"))
        );
    }

    #[tokio::test]
    async fn rejects_token_issued_for_another_audience() {
        assert!(service().validate_access_token(&token("another-api")).await.is_err());
    }

    #[tokio::test]
    #[ignore = "requires a reachable identity provider configured through environment variables"]
    async fn loads_configured_provider_jwks() {
        let auth = OidcJwksVerifier::from_env().await.expect("reachable provider JWKS");

        assert!(!auth.config().issuer.is_empty());
        assert!(!auth.config().audience.is_empty());
    }
}
