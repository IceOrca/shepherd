use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    middleware::Next,
    response::Response,
};
use infra_kernel::request::PrincipalRateLimitKey;
use tracing::{debug, error, info, trace, warn};

use crate::AuthService;

use super::AuthenticatedPrincipal;

/// Accepts a standard bearer token from browser or mobile clients.
pub async fn require_authenticated(
    State(auth): State<Arc<AuthService>>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let method: Method = request.method().clone();
    let path: String = request.uri().path().to_owned();
    trace!(method = %method, path = %path, "Auth middleware received protected request");
    let token: &str = bearer_token(request.headers()).ok_or_else(|| {
        warn!(method = %method, path = %path, "Protected request rejected because bearer token is missing or malformed");
        StatusCode::UNAUTHORIZED
    })?;
    let principal: AuthenticatedPrincipal =
        auth.provider
            .validate_access_token(token)
            .await
            .map_err(|validation_error: super::AccessTokenError| {
                if validation_error.is_temporary() {
                    error!(
                        method = %method,
                        path = %path,
                        reason = %validation_error,
                        "Protected request could not validate bearer token because the auth provider is unavailable"
                    );
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    warn!(
                        method = %method,
                        path = %path,
                        reason = %validation_error,
                        "Protected request rejected because bearer token validation failed"
                    );
                    StatusCode::UNAUTHORIZED
                }
            })?;
    let issuer: String = principal.issuer.clone();
    let subject: String = principal.subject.clone();
    debug!(
        method = %method,
        path = %path,
        issuer = %issuer,
        subject = %subject,
        "Bearer token validated; forwarding request for application account resolution"
    );

    request
        .extensions_mut()
        .insert(PrincipalRateLimitKey::new(format!("{issuer}:{subject}")));
    request.extensions_mut().insert(principal);
    let response: Response = next.run(request).await;
    info!(
        method = %method,
        path = %path,
        issuer = %issuer,
        subject = %subject,
        status = response.status().as_u16(),
        "Protected request completed after authentication"
    );
    Ok(response)
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
