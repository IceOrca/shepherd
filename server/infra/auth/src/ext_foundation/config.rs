use std::{str::FromStr, time::Duration};

use jsonwebtoken::Algorithm;

use super::AccessTokenError;

const DEFAULT_JWKS_REFRESH_SECS: u64 = 300;
const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 5;
const DEFAULT_CLOCK_SKEW_SECS: u64 = 30;

/// Settings used to validate external access tokens locally.
#[derive(Clone, Debug)]
pub struct ExtProviderConfig {
    pub issuer: String,
    pub audience: String,
    pub jwks_url: String,
    pub allowed_algorithms: Vec<Algorithm>,
    pub jwks_refresh_interval: Duration,
    pub http_timeout: Duration,
    pub clock_skew: Duration,
}

impl ExtProviderConfig {
    pub fn from_env() -> Result<Self, AccessTokenError> {
        let issuer: String = required_env("AUTH_ISSUER_URL")?;
        let audience: String = required_env("AUTH_AUDIENCE")?;
        let jwks_url: String = required_env("AUTH_JWKS_URL")?;
        let algorithms: String = std::env::var("AUTH_JWT_ALGORITHMS").unwrap_or_else(|_| "EdDSA".to_owned());

        Self::new(
            issuer,
            audience,
            jwks_url,
            parse_algorithms(&algorithms)?,
            Duration::from_secs(parse_u64_env("AUTH_JWKS_REFRESH_SECS", DEFAULT_JWKS_REFRESH_SECS)?),
            Duration::from_secs(parse_u64_env("AUTH_HTTP_TIMEOUT_SECS", DEFAULT_HTTP_TIMEOUT_SECS)?),
            Duration::from_secs(parse_u64_env("AUTH_CLOCK_SKEW_SECS", DEFAULT_CLOCK_SKEW_SECS)?),
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
    ) -> Result<Self, AccessTokenError> {
        let issuer: String = normalize_url("issuer", issuer)?;
        let jwks_url: String = normalize_url("JWKS", jwks_url)?;
        if audience.trim().is_empty() {
            return Err(AccessTokenError::Configuration(
                "AUTH_AUDIENCE must not be empty".to_owned(),
            ));
        }
        if allowed_algorithms.is_empty() {
            return Err(AccessTokenError::Configuration(
                "at least one JWT algorithm is required".to_owned(),
            ));
        }
        if allowed_algorithms
            .iter()
            .any(|algorithm: &Algorithm| matches!(algorithm, Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512))
        {
            return Err(AccessTokenError::Configuration(
                "HMAC algorithms are not accepted for remote JWKS".to_owned(),
            ));
        }
        if jwks_refresh_interval.is_zero() || http_timeout.is_zero() {
            return Err(AccessTokenError::Configuration(
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
        })
    }
}

fn required_env(name: &str) -> Result<String, AccessTokenError> {
    std::env::var(name)
        .ok()
        .filter(|v: &String| !v.trim().is_empty())
        .ok_or_else(|| AccessTokenError::Configuration(format!("{name} is required")))
}

fn normalize_url(label: &str, value: String) -> Result<String, AccessTokenError> {
    let value: String = value.trim().trim_end_matches('/').to_owned();
    let url: reqwest::Url = reqwest::Url::parse(&value)
        .map_err(|error| AccessTokenError::Configuration(format!("invalid {label} URL: {error}")))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(AccessTokenError::Configuration(format!(
            "{label} URL must be an absolute HTTP(S) URL"
        )));
    }
    Ok(value)
}

fn parse_algorithms(value: &str) -> Result<Vec<Algorithm>, AccessTokenError> {
    value
        .split(',')
        .map(str::trim)
        .filter(|v: &&str| !v.is_empty())
        .map(|v: &str| {
            Algorithm::from_str(v)
                .map_err(|_| AccessTokenError::Configuration(format!("unsupported JWT algorithm: {value}")))
        })
        .collect()
}

fn parse_u64_env(name: &str, default: u64) -> Result<u64, AccessTokenError> {
    std::env::var(name).map_or(Ok(default), |value: String| {
        value.parse::<u64>().map_err(|error: std::num::ParseIntError| {
            AccessTokenError::Configuration(format!("{name} must be an unsigned integer: {error}"))
        })
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use jsonwebtoken::Algorithm;

    use super::ExtProviderConfig;

    #[test]
    fn normalizes_urls_and_preserves_expected_audience() {
        let config = ExtProviderConfig::new(
            "https://identity.example/realms/shepherd/".to_owned(),
            "shepherd-api".to_owned(),
            "https://identity.example/realms/shepherd/certs/".to_owned(),
            vec![Algorithm::RS256],
            Duration::from_secs(300),
            Duration::from_secs(5),
            Duration::from_secs(30),
        )
        .expect("valid configuration");

        assert_eq!(config.issuer, "https://identity.example/realms/shepherd");
        assert_eq!(config.audience, "shepherd-api");
        assert_eq!(config.jwks_url, "https://identity.example/realms/shepherd/certs");
    }

    #[test]
    fn rejects_symmetric_algorithms_for_remote_jwks() {
        let result = ExtProviderConfig::new(
            "https://identity.example/realms/shepherd".to_owned(),
            "shepherd-api".to_owned(),
            "https://identity.example/realms/shepherd/certs".to_owned(),
            vec![Algorithm::HS256],
            Duration::from_secs(300),
            Duration::from_secs(5),
            Duration::from_secs(30),
        );

        assert!(result.is_err());
    }
}
