use std::collections::BTreeSet;

use serde::Deserialize;
use uuid::Uuid;

use super::AccessTokenErr;

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum Audience {
    One(String),
    Many(Vec<String>),
}

impl Audience {
    fn into_vec(self) -> Vec<String> {
        match self {
            Self::One(value) => vec![value],
            Self::Many(values) => values,
        }
    }
}

/// Standard access-token claims needed by the authentication boundary.
#[derive(Clone, Debug, Deserialize)]
pub struct AccessTokenClaims {
    pub iss: String,
    pub sub: String,
    pub aud: Audience,
    pub exp: u64,
    #[serde(default)]
    pub nbf: Option<u64>,
    #[serde(default)]
    pub iat: Option<u64>,
    #[serde(default)]
    pub jti: Option<String>,
    #[serde(default)]
    #[serde(alias = "sid")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub preferred_username: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub email_verified: Option<bool>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub tid: Option<Uuid>,
}

/// A signature-verified external identity. It is not an application account or
/// tenant authorization decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthedPrincipal {
    pub issuer: String,
    pub subject: String,
    pub audience: Vec<String>,
    pub username: Option<String>,
    pub email: Option<String>,
    pub email_verified: bool,
    pub scopes: Vec<String>,
    pub session_id: Option<String>,
    pub token_id: Option<String>,
    pub issued_at: Option<u64>,
    pub expires_at: u64,
    /// Optional tenant hint issued by an external token customization hook.
    /// The application account mapping remains authoritative.
    pub tenant_id: Option<Uuid>,
}

impl TryFrom<AccessTokenClaims> for AuthedPrincipal {
    type Error = AccessTokenErr;

    fn try_from(claims: AccessTokenClaims) -> Result<Self, Self::Error> {
        validate_identifier("issuer", &claims.iss, 2_048)?;
        validate_identifier("subject", &claims.sub, 255)?;
        validate_optional_claim("preferred_username", claims.preferred_username.as_deref(), 255)?;
        validate_optional_claim("email", claims.email.as_deref(), 320)?;
        validate_optional_claim("session id", claims.session_id.as_deref(), 255)?;
        validate_optional_claim("token id", claims.jti.as_deref(), 255)?;

        if claims.iat.is_some_and(|issued_at: u64| issued_at >= claims.exp) {
            return Err(AccessTokenErr::InvalidClaims(
                "issued-at must be earlier than expiry".to_owned(),
            ));
        }
        let audience: Vec<String> = claims.aud.into_vec();
        if audience.is_empty() || audience.iter().any(|value: &String| value.trim().is_empty()) {
            return Err(AccessTokenErr::InvalidClaims("audience must not be empty".to_owned()));
        }

        let scopes: Vec<String> = deduplicate_words(claims.scope.as_deref().unwrap_or_default().split_whitespace());
        Ok(Self {
            issuer: claims.iss,
            subject: claims.sub,
            audience,
            username: claims.preferred_username,
            email: claims.email,
            email_verified: claims.email_verified.unwrap_or(false),
            scopes,
            session_id: claims.session_id,
            token_id: claims.jti,
            issued_at: claims.iat,
            expires_at: claims.exp,
            tenant_id: claims.tid,
        })
    }
}

fn validate_identifier(name: &str, value: &str, maximum_length: usize) -> Result<(), AccessTokenErr> {
    if value.trim() != value || value.is_empty() || value.len() > maximum_length {
        return Err(AccessTokenErr::InvalidClaims(format!("{name} is malformed")));
    }
    Ok(())
}

fn validate_optional_claim(name: &str, value: Option<&str>, maximum_length: usize) -> Result<(), AccessTokenErr> {
    value.map_or(Ok(()), |value: &str| validate_identifier(name, value, maximum_length))
}

fn deduplicate_words<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    values
        .filter(|v: &&'a str| !v.is_empty() && v.len() <= 255)
        .map(str::to_owned)
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{AccessTokenClaims, Audience, AuthedPrincipal};

    fn claims() -> AccessTokenClaims {
        AccessTokenClaims {
            iss: "https://identity.example/auth/v1".to_owned(),
            sub: "external-user-id".to_owned(),
            aud: Audience::Many(vec!["account".to_owned(), "example-api".to_owned()]),
            exp: 2_000,
            nbf: None,
            iat: Some(1_000),
            jti: Some("token-id".to_owned()),
            session_id: Some("session-id".to_owned()),
            preferred_username: Some("alice".to_owned()),
            email: Some("alice@example.com".to_owned()),
            email_verified: Some(true),
            scope: Some("profile openid profile email".to_owned()),
            tid: Some(uuid::uuid!("018f4c6d-7e41-7b89-a4fd-0f8efcc57e31")),
        }
    }

    #[test]
    fn creates_external_principal_and_deduplicates_claims() {
        let principal = AuthedPrincipal::try_from(claims()).expect("valid principal");

        assert_eq!(principal.subject, "external-user-id");
        assert_eq!(principal.scopes, vec!["email", "openid", "profile"]);
        assert_eq!(principal.session_id.as_deref(), Some("session-id"));
        assert_eq!(
            principal.tenant_id,
            Some(uuid::uuid!("018f4c6d-7e41-7b89-a4fd-0f8efcc57e31"))
        );
    }

    #[test]
    fn rejects_impossible_token_time_ordering() {
        let mut invalid = claims();
        invalid.iat = Some(invalid.exp);

        assert!(AuthedPrincipal::try_from(invalid).is_err());
    }
}
