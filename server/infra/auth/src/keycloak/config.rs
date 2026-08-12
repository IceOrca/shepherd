use std::{str::FromStr, time::Duration};

use jsonwebtoken::Algorithm;

use super::KeycloakAuthError;

const DEFAULT_JWKS_REFRESH_SECS: u64 = 300;
const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 5;
const DEFAULT_CLOCK_SKEW_SECS: u64 = 30;

/// Settings used to validate Keycloak access tokens locally.
#[derive(Clone, Debug)]
pub struct KeycloakConfig {
    pub issuer: String,
    pub audience: String,
    pub jwks_url: String,
    pub allowed_algorithms: Vec<Algorithm>,
    pub jwks_refresh_interval: Duration,
    pub http_timeout: Duration,
    pub clock_skew: Duration,
    pub accept_forwarded_access_token: bool,
}

impl KeycloakConfig {
    pub fn from_env() -> Result<Self, KeycloakAuthError> {
        let issuer = required_env("KEYCLOAK_ISSUER_URL")?;
        let audience = required_env("KEYCLOAK_AUDIENCE")?;
        let jwks_url = std::env::var("KEYCLOAK_JWKS_URL")
            .unwrap_or_else(|_| format!("{}/protocol/openid-connect/certs", issuer.trim_end_matches('/')));
        let algorithms = std::env::var("KEYCLOAK_JWT_ALGORITHMS").unwrap_or_else(|_| "RS256".to_owned());

        Self::new(
            issuer,
            audience,
            jwks_url,
            parse_algorithms(&algorithms)?,
            Duration::from_secs(parse_u64_env("KEYCLOAK_JWKS_REFRESH_SECS", DEFAULT_JWKS_REFRESH_SECS)?),
            Duration::from_secs(parse_u64_env("KEYCLOAK_HTTP_TIMEOUT_SECS", DEFAULT_HTTP_TIMEOUT_SECS)?),
            Duration::from_secs(parse_u64_env("KEYCLOAK_CLOCK_SKEW_SECS", DEFAULT_CLOCK_SKEW_SECS)?),
            parse_bool_env("KEYCLOAK_ACCEPT_FORWARDED_ACCESS_TOKEN", false)?,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        issuer: String,
        audience: String,
        jwks_url: String,
        allowed_algorithms: Vec<Algorithm>,
        jwks_refresh_interval: Duration,
        http_timeout: Duration,
        clock_skew: Duration,
        accept_forwarded_access_token: bool,
    ) -> Result<Self, KeycloakAuthError> {
        let issuer = normalize_url("issuer", issuer)?;
        let jwks_url = normalize_url("JWKS", jwks_url)?;
        if audience.trim().is_empty() {
            return Err(KeycloakAuthError::Configuration(
                "KEYCLOAK_AUDIENCE must not be empty".to_owned(),
            ));
        }
        if allowed_algorithms.is_empty() {
            return Err(KeycloakAuthError::Configuration(
                "at least one Keycloak JWT algorithm is required".to_owned(),
            ));
        }
        if allowed_algorithms
            .iter()
            .any(|algorithm| matches!(algorithm, Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512))
        {
            return Err(KeycloakAuthError::Configuration(
                "HMAC algorithms are not accepted for remote Keycloak JWKS".to_owned(),
            ));
        }
        if jwks_refresh_interval.is_zero() || http_timeout.is_zero() {
            return Err(KeycloakAuthError::Configuration(
                "JWKS refresh interval and HTTP timeout must be greater than zero".to_owned(),
            ));
        }

        Ok(Self {
            issuer,
            audience: audience.trim().to_owned(),
            jwks_url,
            allowed_algorithms,
            jwks_refresh_interval,
            http_timeout,
            clock_skew,
            accept_forwarded_access_token,
        })
    }
}

fn required_env(name: &str) -> Result<String, KeycloakAuthError> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| KeycloakAuthError::Configuration(format!("{name} is required")))
}

fn normalize_url(label: &str, value: String) -> Result<String, KeycloakAuthError> {
    let value = value.trim().trim_end_matches('/').to_owned();
    let url = reqwest::Url::parse(&value)
        .map_err(|error| KeycloakAuthError::Configuration(format!("invalid Keycloak {label} URL: {error}")))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(KeycloakAuthError::Configuration(format!(
            "Keycloak {label} URL must be an absolute HTTP(S) URL"
        )));
    }
    Ok(value)
}

fn parse_algorithms(value: &str) -> Result<Vec<Algorithm>, KeycloakAuthError> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            Algorithm::from_str(value)
                .map_err(|_| KeycloakAuthError::Configuration(format!("unsupported Keycloak JWT algorithm: {value}")))
        })
        .collect()
}

fn parse_u64_env(name: &str, default: u64) -> Result<u64, KeycloakAuthError> {
    std::env::var(name).map_or(Ok(default), |value| {
        value
            .parse::<u64>()
            .map_err(|error| KeycloakAuthError::Configuration(format!("{name} must be an unsigned integer: {error}")))
    })
}

fn parse_bool_env(name: &str, default: bool) -> Result<bool, KeycloakAuthError> {
    std::env::var(name).map_or(Ok(default), |value| {
        value
            .parse::<bool>()
            .map_err(|error| KeycloakAuthError::Configuration(format!("{name} must be true or false: {error}")))
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use jsonwebtoken::Algorithm;

    use super::KeycloakConfig;

    #[test]
    fn normalizes_urls_and_preserves_expected_audience() {
        let config = KeycloakConfig::new(
            "https://identity.example/realms/shepherd/".to_owned(),
            "shepherd-api".to_owned(),
            "https://identity.example/realms/shepherd/certs/".to_owned(),
            vec![Algorithm::RS256],
            Duration::from_secs(300),
            Duration::from_secs(5),
            Duration::from_secs(30),
            false,
        )
        .expect("valid configuration");

        assert_eq!(config.issuer, "https://identity.example/realms/shepherd");
        assert_eq!(config.audience, "shepherd-api");
        assert_eq!(config.jwks_url, "https://identity.example/realms/shepherd/certs");
    }

    #[test]
    fn rejects_symmetric_algorithms_for_remote_jwks() {
        let result = KeycloakConfig::new(
            "https://identity.example/realms/shepherd".to_owned(),
            "shepherd-api".to_owned(),
            "https://identity.example/realms/shepherd/certs".to_owned(),
            vec![Algorithm::HS256],
            Duration::from_secs(300),
            Duration::from_secs(5),
            Duration::from_secs(30),
            false,
        );

        assert!(result.is_err());
    }
}
