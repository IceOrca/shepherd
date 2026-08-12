use thiserror::Error;

#[derive(Debug, Error)]
pub enum KeycloakAuthError {
    #[error("invalid Keycloak configuration: {0}")]
    Configuration(String),
    #[error("Keycloak token is missing a key identifier")]
    MissingKeyId,
    #[error("Keycloak token uses a disallowed signing algorithm")]
    DisallowedAlgorithm,
    #[error("Keycloak JWKS does not contain the token signing key")]
    UnknownKey,
    #[error("Keycloak JWKS signing key is invalid")]
    InvalidSigningKey(#[source] jsonwebtoken::errors::Error),
    #[error("Keycloak access token is invalid")]
    InvalidToken(#[source] jsonwebtoken::errors::Error),
    #[error("Keycloak access token contains invalid identity claims: {0}")]
    InvalidClaims(String),
    #[error("could not retrieve Keycloak JWKS")]
    JwksUnavailable(#[source] reqwest::Error),
    #[error("Keycloak returned an empty JWKS document")]
    EmptyJwks,
}

impl KeycloakAuthError {
    pub fn is_temporary(&self) -> bool {
        matches!(self, Self::JwksUnavailable(_) | Self::EmptyJwks)
    }
}
