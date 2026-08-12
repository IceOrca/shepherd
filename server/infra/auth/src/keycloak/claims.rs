use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use super::KeycloakAuthError;

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

#[derive(Clone, Debug, Default, Deserialize)]
pub struct RealmAccess {
    #[serde(default)]
    pub roles: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ResourceAccess {
    #[serde(default)]
    pub roles: Vec<String>,
}

/// Keycloak access-token claims needed by the authentication boundary.
#[derive(Clone, Debug, Deserialize)]
pub struct KeycloakClaims {
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
    pub sid: Option<String>,
    #[serde(default)]
    pub azp: Option<String>,
    #[serde(default)]
    pub preferred_username: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub email_verified: Option<bool>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub realm_access: RealmAccess,
    #[serde(default)]
    pub resource_access: BTreeMap<String, ResourceAccess>,
}

/// A signature-verified external identity. It is not a Shepherd account or
/// tenant authorization decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeycloakPrincipal {
    pub issuer: String,
    pub subject: String,
    pub audience: Vec<String>,
    pub authorized_party: Option<String>,
    pub username: Option<String>,
    pub email: Option<String>,
    pub email_verified: bool,
    pub scopes: Vec<String>,
    pub realm_roles: Vec<String>,
    pub resource_roles: BTreeMap<String, Vec<String>>,
    pub session_id: Option<String>,
    pub token_id: Option<String>,
    pub issued_at: Option<u64>,
    pub expires_at: u64,
}

impl TryFrom<KeycloakClaims> for KeycloakPrincipal {
    type Error = KeycloakAuthError;

    fn try_from(claims: KeycloakClaims) -> Result<Self, Self::Error> {
        validate_identifier("issuer", &claims.iss, 2_048)?;
        validate_identifier("subject", &claims.sub, 255)?;
        validate_optional_claim("preferred_username", claims.preferred_username.as_deref(), 255)?;
        validate_optional_claim("email", claims.email.as_deref(), 320)?;
        validate_optional_claim("session id", claims.sid.as_deref(), 255)?;
        validate_optional_claim("token id", claims.jti.as_deref(), 255)?;

        if claims.iat.is_some_and(|issued_at| issued_at >= claims.exp) {
            return Err(KeycloakAuthError::InvalidClaims(
                "issued-at must be earlier than expiry".to_owned(),
            ));
        }
        let audience = claims.aud.into_vec();
        if audience.is_empty() || audience.iter().any(|value| value.trim().is_empty()) {
            return Err(KeycloakAuthError::InvalidClaims(
                "audience must not be empty".to_owned(),
            ));
        }

        let scopes = deduplicate_words(claims.scope.as_deref().unwrap_or_default().split_whitespace());
        let realm_roles = deduplicate_words(claims.realm_access.roles.iter().map(String::as_str));
        let resource_roles = claims
            .resource_access
            .into_iter()
            .filter_map(|(resource, access)| {
                let roles = deduplicate_words(access.roles.iter().map(String::as_str));
                (!resource.trim().is_empty() && !roles.is_empty()).then_some((resource, roles))
            })
            .collect();

        Ok(Self {
            issuer: claims.iss,
            subject: claims.sub,
            audience,
            authorized_party: claims.azp,
            username: claims.preferred_username,
            email: claims.email,
            email_verified: claims.email_verified.unwrap_or(false),
            scopes,
            realm_roles,
            resource_roles,
            session_id: claims.sid,
            token_id: claims.jti,
            issued_at: claims.iat,
            expires_at: claims.exp,
        })
    }
}

fn validate_identifier(name: &str, value: &str, maximum_length: usize) -> Result<(), KeycloakAuthError> {
    if value.trim() != value || value.is_empty() || value.len() > maximum_length {
        return Err(KeycloakAuthError::InvalidClaims(format!("{name} is malformed")));
    }
    Ok(())
}

fn validate_optional_claim(name: &str, value: Option<&str>, maximum_length: usize) -> Result<(), KeycloakAuthError> {
    value.map_or(Ok(()), |value| validate_identifier(name, value, maximum_length))
}

fn deduplicate_words<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    values
        .filter(|value| !value.is_empty() && value.len() <= 255)
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{Audience, KeycloakClaims, KeycloakPrincipal, RealmAccess, ResourceAccess};

    fn claims() -> KeycloakClaims {
        KeycloakClaims {
            iss: "https://identity.example/realms/shepherd".to_owned(),
            sub: "external-user-id".to_owned(),
            aud: Audience::Many(vec!["account".to_owned(), "shepherd-api".to_owned()]),
            exp: 2_000,
            nbf: None,
            iat: Some(1_000),
            jti: Some("token-id".to_owned()),
            sid: Some("session-id".to_owned()),
            azp: Some("shepherd-web".to_owned()),
            preferred_username: Some("alice".to_owned()),
            email: Some("alice@example.com".to_owned()),
            email_verified: Some(true),
            scope: Some("profile openid profile email".to_owned()),
            realm_access: RealmAccess {
                roles: vec!["staff".to_owned(), "staff".to_owned()],
            },
            resource_access: BTreeMap::from([(
                "shepherd-api".to_owned(),
                ResourceAccess {
                    roles: vec!["viewer".to_owned(), "viewer".to_owned()],
                },
            )]),
        }
    }

    #[test]
    fn creates_external_principal_and_deduplicates_claims() {
        let principal = KeycloakPrincipal::try_from(claims()).expect("valid principal");

        assert_eq!(principal.subject, "external-user-id");
        assert_eq!(principal.scopes, vec!["email", "openid", "profile"]);
        assert_eq!(principal.realm_roles, vec!["staff"]);
        assert_eq!(
            principal.resource_roles.get("shepherd-api"),
            Some(&vec!["viewer".to_owned()])
        );
    }

    #[test]
    fn rejects_impossible_token_time_ordering() {
        let mut invalid = claims();
        invalid.iat = Some(invalid.exp);

        assert!(KeycloakPrincipal::try_from(invalid).is_err());
    }
}
