use thiserror::Error;

#[derive(Debug, Error)]
pub enum AccessTokenErr {
    #[error("invalid access-token configuration: {0}")]
    Configuration(String),
    #[error("access token is missing a key identifier")]
    MissingKeyId,
    #[error("access token uses a disallowed signing algorithm")]
    DisallowedAlgorithm,
    #[error("JWKS does not contain the token signing key")]
    UnknownKey,
    #[error("JWKS signing key is invalid")]
    InvalidSigningKey(#[source] jsonwebtoken::errors::Error),
    #[error("access token is invalid")]
    InvalidToken(#[source] jsonwebtoken::errors::Error),
    #[error("access token contains invalid identity claims: {0}")]
    InvalidClaims(String),
    #[error("could not retrieve JWKS")]
    JwksUnavailable(#[source] reqwest::Error),
    #[error("identity provider returned an empty JWKS document")]
    EmptyJwks,
}

impl AccessTokenErr {
    pub fn is_temporary(&self) -> bool {
        matches!(self, Self::JwksUnavailable(_) | Self::EmptyJwks)
    }
}
