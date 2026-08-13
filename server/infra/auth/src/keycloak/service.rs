use std::{sync::Arc, time::Instant};

use jsonwebtoken::{
    Algorithm, DecodingKey, Validation, decode, decode_header,
    jwk::{Jwk, JwkSet, KeyOperations, PublicKeyUse},
};
use tokio::sync::{Mutex, RwLock};

use super::{KeycloakAuthError, KeycloakClaims, KeycloakConfig, KeycloakPrincipal};

const UNKNOWN_KID_REFRESH_COOLDOWN_SECS: u64 = 10;

#[derive(Default)]
struct CachedJwks {
    set: JwkSet,
    fetched_at: Option<Instant>,
}

/// Validates Keycloak access tokens locally and refreshes signing keys on a
/// bounded interval or immediately when Keycloak rotates to an unknown KID.
pub struct KeycloakAuth {
    config: KeycloakConfig,
    client: reqwest::Client,
    jwks: RwLock<CachedJwks>,
    refresh_guard: Mutex<()>,
}

impl KeycloakAuth {
    pub async fn from_env() -> Result<Arc<Self>, KeycloakAuthError> {
        Self::from_config(KeycloakConfig::from_env()?).await
    }

    pub async fn from_config(config: KeycloakConfig) -> Result<Arc<Self>, KeycloakAuthError> {
        let client: reqwest::Client = reqwest::Client::builder()
            .timeout(config.http_timeout)
            .build()
            .map_err(KeycloakAuthError::JwksUnavailable)?;
        let service: Arc<KeycloakAuth> = Arc::new(Self {
            config,
            client,
            jwks: RwLock::new(CachedJwks::default()),
            refresh_guard: Mutex::new(()),
        });
        service.refresh_jwks(false).await?;
        Ok(service)
    }

    pub fn config(&self) -> &KeycloakConfig {
        &self.config
    }

    pub async fn validate_access_token(&self, token: &str) -> Result<KeycloakPrincipal, KeycloakAuthError> {
        let header: jsonwebtoken::Header =
            jsonwebtoken::decode_header(token).map_err(KeycloakAuthError::InvalidToken)?;
        if !self.config.allowed_algorithms.contains(&header.alg) {
            return Err(KeycloakAuthError::DisallowedAlgorithm);
        }
        let kid: &str = header.kid.as_deref().ok_or(KeycloakAuthError::MissingKeyId)?;
        let key: DecodingKey = self.decoding_key(kid, header.alg).await?;

        let mut validation: Validation = Validation::new(header.alg);
        validation.set_issuer(&[&self.config.issuer]);
        validation.set_audience(&[&self.config.audience]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        validation.validate_exp = true;
        validation.validate_nbf = true;
        validation.validate_aud = true;
        validation.leeway = self.config.clock_skew.as_secs();

        let token: jsonwebtoken::TokenData<KeycloakClaims> =
            decode::<KeycloakClaims>(token, &key, &validation).map_err(KeycloakAuthError::InvalidToken)?;
        let principal: KeycloakPrincipal = KeycloakPrincipal::try_from(token.claims)?;
        if principal
            .issued_at
            .is_some_and(|issued_at| issued_at > jsonwebtoken::get_current_timestamp() + validation.leeway)
        {
            return Err(KeycloakAuthError::InvalidClaims(
                "issued-at is unreasonably far in the future".to_owned(),
            ));
        }
        Ok(principal)
    }

    async fn decoding_key(&self, kid: &str, algorithm: Algorithm) -> Result<DecodingKey, KeycloakAuthError> {
        let (cached_key, cache_is_fresh): (Option<Jwk>, bool) = {
            let cache = self.jwks.read().await;
            let is_fresh: bool = cache
                .fetched_at
                .is_some_and(|fetched_at| fetched_at.elapsed() < self.config.jwks_refresh_interval);
            (cache.set.find(kid).cloned(), is_fresh)
        };
        if cache_is_fresh && let Some(jwk) = cached_key.as_ref() {
            return decoding_key_from_jwk(jwk, algorithm);
        }

        match self.refresh_jwks(cached_key.is_none()).await {
            Ok(()) => self
                .jwks
                .read()
                .await
                .set
                .find(kid)
                .cloned()
                .ok_or(KeycloakAuthError::UnknownKey)
                .and_then(|jwk| decoding_key_from_jwk(&jwk, algorithm)),
            Err(error) => {
                if let Some(jwk) = cached_key {
                    tracing::warn!(error = %error, kid, "using stale Keycloak signing key after JWKS refresh failed");
                    decoding_key_from_jwk(&jwk, algorithm)
                } else {
                    Err(error)
                }
            }
        }
    }

    async fn refresh_jwks(&self, force: bool) -> Result<(), KeycloakAuthError> {
        let _refresh_guard = self.refresh_guard.lock().await;
        let cache_age = self.jwks.read().await.fetched_at.map(|fetched_at| fetched_at.elapsed());
        let cache_is_fresh = cache_age.is_some_and(|age| age < self.config.jwks_refresh_interval);
        let unknown_kid_refresh_is_throttled =
            force && cache_age.is_some_and(|age| age.as_secs() < UNKNOWN_KID_REFRESH_COOLDOWN_SECS);
        if (cache_is_fresh && !force) || unknown_kid_refresh_is_throttled {
            return Ok(());
        }

        let set = self
            .client
            .get(&self.config.jwks_url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(KeycloakAuthError::JwksUnavailable)?
            .json::<JwkSet>()
            .await
            .map_err(KeycloakAuthError::JwksUnavailable)?;
        if set.keys.is_empty() {
            return Err(KeycloakAuthError::EmptyJwks);
        }

        *self.jwks.write().await = CachedJwks {
            set,
            fetched_at: Some(Instant::now()),
        };
        Ok(())
    }
}

#[cfg(test)]
impl KeycloakAuth {
    fn with_jwks(config: KeycloakConfig, set: JwkSet) -> Self {
        let client = reqwest::Client::builder()
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

fn decoding_key_from_jwk(jwk: &Jwk, algorithm: Algorithm) -> Result<DecodingKey, KeycloakAuthError> {
    if jwk
        .common
        .public_key_use
        .as_ref()
        .is_some_and(|key_use| !matches!(key_use, PublicKeyUse::Signature))
        || jwk.common.key_operations.as_ref().is_some_and(|operations| {
            !operations
                .iter()
                .any(|operation| matches!(operation, KeyOperations::Verify))
        })
    {
        return Err(KeycloakAuthError::DisallowedAlgorithm);
    }
    if let Some(key_algorithm) = jwk.common.key_algorithm {
        let key_algorithm = Algorithm::try_from(key_algorithm).map_err(KeycloakAuthError::InvalidSigningKey)?;
        if key_algorithm != algorithm {
            return Err(KeycloakAuthError::DisallowedAlgorithm);
        }
    }
    DecodingKey::from_jwk(jwk).map_err(KeycloakAuthError::InvalidSigningKey)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, encode, jwk::JwkSet};
    use serde::Serialize;

    use super::KeycloakAuth;
    use crate::keycloak::KeycloakConfig;

    const ISSUER: &str = "https://identity.example/realms/shepherd";
    const AUDIENCE: &str = "shepherd-api";
    const KID: &str = "keycloak-test-key";

    #[derive(Serialize)]
    struct TestClaims<'a> {
        iss: &'a str,
        sub: &'a str,
        aud: &'a str,
        exp: u64,
        iat: u64,
        preferred_username: &'a str,
    }

    fn config() -> KeycloakConfig {
        KeycloakConfig::new(
            ISSUER.to_owned(),
            AUDIENCE.to_owned(),
            "https://identity.example/realms/shepherd/protocol/openid-connect/certs".to_owned(),
            vec![Algorithm::EdDSA],
            Duration::from_secs(300),
            Duration::from_secs(5),
            Duration::from_secs(0),
            false,
        )
        .expect("test configuration")
    }

    fn service() -> KeycloakAuth {
        let decoding_key = DecodingKey::from_ed_pem(include_bytes!("../../../../security/jwtkey_dev/jwt_public.pem"))
            .expect("test public key");
        let mut jwk =
            jsonwebtoken::jwk::Jwk::from_decoding_key(&decoding_key, Some(Algorithm::EdDSA)).expect("test JWK");
        jwk.common.key_id = Some(KID.to_owned());
        KeycloakAuth::with_jwks(config(), JwkSet { keys: vec![jwk] })
    }

    fn token(audience: &str) -> String {
        let now = jsonwebtoken::get_current_timestamp();
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(KID.to_owned());
        encode(
            &header,
            &TestClaims {
                iss: ISSUER,
                sub: "keycloak-subject",
                aud: audience,
                exp: now + 300,
                iat: now,
                preferred_username: "alice",
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
        assert_eq!(principal.subject, "keycloak-subject");
        assert_eq!(principal.username.as_deref(), Some("alice"));
    }

    #[tokio::test]
    async fn rejects_token_issued_for_another_audience() {
        assert!(service().validate_access_token(&token("another-api")).await.is_err());
    }

    #[tokio::test]
    #[ignore = "requires a reachable Keycloak realm configured through environment variables"]
    async fn loads_configured_keycloak_jwks() {
        let auth = KeycloakAuth::from_env().await.expect("reachable Keycloak JWKS");

        assert!(!auth.config().issuer.is_empty());
        assert!(!auth.config().audience.is_empty());
    }
}
