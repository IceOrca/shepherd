use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::Response,
};
use infra_kernel::request::PrincipalRateLimitKey;

use crate::AuthService;

use super::KeycloakPrincipal;

const FORWARDED_ACCESS_TOKEN: &str = "x-forwarded-access-token";

/// Accepts standard bearer tokens (including future mobile clients) and,
/// when explicitly enabled, oauth2-proxy's forwarded access-token header.
pub async fn require_authenticated(
    State(auth): State<Arc<AuthService>>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token: &str = access_token(request.headers(), auth.keycloak.config().accept_forwarded_access_token)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let principal: KeycloakPrincipal = auth.keycloak.validate_access_token(token).await.map_err(|error| {
        tracing::warn!(error = %error, "Keycloak access token rejected");
        if error.is_temporary() {
            StatusCode::SERVICE_UNAVAILABLE
        } else {
            StatusCode::UNAUTHORIZED
        }
    })?;

    request.extensions_mut().insert(PrincipalRateLimitKey::new(format!(
        "{}:{}",
        principal.issuer, principal.subject
    )));
    request.extensions_mut().insert(principal);
    Ok(next.run(request).await)
}

fn access_token(headers: &HeaderMap, accept_forwarded: bool) -> Option<&str> {
    bearer_token(headers).or_else(|| {
        if !accept_forwarded {
            return None;
        }
        headers
            .get(FORWARDED_ACCESS_TOKEN)?
            .to_str()
            .ok()
            .map(str::trim)
            .filter(|token| !token.is_empty())
    })
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let authorization: &str = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token): (&str, &str) = authorization.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|token| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue, header};

    use super::access_token;

    #[test]
    fn bearer_token_has_priority_over_proxy_header() {
        let mut headers: HeaderMap = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer mobile-token"));
        headers.insert("x-forwarded-access-token", HeaderValue::from_static("proxy-token"));

        assert_eq!(access_token(&headers, true), Some("mobile-token"));
    }

    #[test]
    fn forwarded_token_requires_explicit_configuration() {
        let mut headers: HeaderMap = HeaderMap::new();
        headers.insert("x-forwarded-access-token", HeaderValue::from_static("proxy-token"));

        assert_eq!(access_token(&headers, false), None);
        assert_eq!(access_token(&headers, true), Some("proxy-token"));
    }

    #[test]
    fn rejects_non_bearer_authorization_scheme() {
        let mut headers: HeaderMap = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Basic abc"));

        assert_eq!(access_token(&headers, false), None);
    }

    #[test]
    fn bearer_scheme_is_case_insensitive() {
        let mut headers: HeaderMap = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("bearer token"));

        assert_eq!(access_token(&headers, false), Some("token"));
    }
}
