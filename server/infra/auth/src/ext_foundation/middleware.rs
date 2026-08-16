use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::Response,
};
use infra_kernel::request::PrincipalRateLimitKey;

use crate::AuthService;

use super::AuthenticatedPrincipal;

/// Accepts a standard bearer token from browser or mobile clients.
pub async fn require_authenticated(
    State(auth): State<Arc<AuthService>>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token: &str = bearer_token(request.headers()).ok_or(StatusCode::UNAUTHORIZED)?;
    let principal: AuthenticatedPrincipal =
        auth.provider
            .validate_access_token(token)
            .await
            .map_err(|error: super::AccessTokenError| {
                tracing::warn!(error = %error, "access token rejected");
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

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let authorization: &str = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token): (&str, &str) = authorization.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|token: &&str| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue, header};

    use super::bearer_token;

    #[test]
    fn extracts_bearer_token() {
        let mut headers: HeaderMap = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer mobile-token"));
        assert_eq!(bearer_token(&headers), Some("mobile-token"));
    }

    #[test]
    fn rejects_non_bearer_authorization_scheme() {
        let mut headers: HeaderMap = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Basic abc"));

        assert_eq!(bearer_token(&headers), None);
    }

    #[test]
    fn bearer_scheme_is_case_insensitive() {
        let mut headers: HeaderMap = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("bearer token"));

        assert_eq!(bearer_token(&headers), Some("token"));
    }
}
