use std::sync::Arc;

use axum::{
    Extension,
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    middleware::{Next, from_fn_with_state},
    response::Response,
    routing::MethodRouter,
};
use infra_kernel::request::PrincipalRateLimitKey;
use tracing::{debug, error, info, trace, warn};
use super::AccessTokenErr;
use crate::{AuthService, PermissionCode};
use super::{AuthedPrincipal, account::AuthedUser};
use crate::AuthCodeErr;

#[derive(Clone, Debug)]
pub enum PermissionRequirement {
    Any(Arc<[PermissionCode]>),
    All(Arc<[PermissionCode]>),
}

impl PermissionRequirement {
    pub fn one(permission: &str) -> Self {
        Self::Any(Arc::from([parse_permission(permission)]))
    }

    pub fn any<const N: usize>(permissions: [&str; N]) -> Self {
        assert!(N > 0, "an any-permission requirement cannot be empty");
        Self::Any(permissions.into_iter().map(parse_permission).collect())
    }

    pub fn all<const N: usize>(permissions: [&str; N]) -> Self {
        assert!(N > 0, "an all-permission requirement cannot be empty");
        Self::All(permissions.into_iter().map(parse_permission).collect())
    }

    fn allows(&self, user: &AuthedUser) -> bool {
        match self {
            Self::Any(permissions) => permissions
                .iter()
                .any(|permission| user.has_permission(permission.as_str())),
            Self::All(permissions) => permissions
                .iter()
                .all(|permission| user.has_permission(permission.as_str())),
        }
    }

    fn display_codes(&self) -> String {
        let permissions: &Arc<[PermissionCode]> = match self {
            Self::Any(permissions) | Self::All(permissions) => permissions,
        };
        permissions
            .iter()
            .map(PermissionCode::as_str)
            .collect::<Vec<_>>()
            .join(",")
    }
}

pub trait PermissionRouteExt<S>: Sized {
    fn require_one(self, permission: &str) -> Self;
    fn require_any<const N: usize>(self, permissions: [&str; N]) -> Self;
    fn require_all<const N: usize>(self, permissions: [&str; N]) -> Self;
}

impl<S> PermissionRouteExt<S> for MethodRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    fn require_one(self, permission: &str) -> Self {
        self.route_layer(from_fn_with_state(
            PermissionRequirement::one(permission),
            require_permissions,
        ))
    }

    fn require_any<const N: usize>(self, permissions: [&str; N]) -> Self {
        self.route_layer(from_fn_with_state(
            PermissionRequirement::any(permissions),
            require_permissions,
        ))
    }

    fn require_all<const N: usize>(self, permissions: [&str; N]) -> Self {
        self.route_layer(from_fn_with_state(
            PermissionRequirement::all(permissions),
            require_permissions,
        ))
    }
}

fn parse_permission(permission: &str) -> PermissionCode {
    PermissionCode::parse(permission)
        .unwrap_or_else(|err: AuthCodeErr| panic!("invalid route permission requirement `{permission}`: {err}"))
}

async fn require_permissions(
    State(requirement): State<PermissionRequirement>,
    Extension(user): Extension<AuthedUser>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if requirement.allows(&user) {
        return Ok(next.run(request).await);
    }
    debug!(
        operation = "authorize_route_permissions",
        tenant_id = %user.tenant_id,
        account_id = %user.account_id,
        permission_mode = match requirement {
            PermissionRequirement::Any(_) => "any",
            PermissionRequirement::All(_) => "all",
        },
        required_permissions = %requirement.display_codes(),
        "Request denied by route permission middleware"
    );
    Err(StatusCode::FORBIDDEN)
}

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
    let principal: AuthedPrincipal =
        auth.token_verifier
            .validate_access_token(token)
            .await
            .map_err(|err: AccessTokenErr| {
                if err.is_temporary() {
                    error!(
                        method = %method,
                        path = %path,
                        reason = %err,
                        "Protected request could not validate bearer token because the auth provider is unavailable"
                    );
                    StatusCode::SERVICE_UNAVAILABLE
                } else {
                    warn!(
                        method = %method,
                        path = %path,
                        reason = %err,
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
    use axum::{
        Extension, Router,
        body::Body,
        http::{HeaderMap, HeaderValue, Request, StatusCode, header},
        response::Response,
        routing::get,
    };

    use tower::ServiceExt;
    use uuid::Uuid;

    use super::{AuthedUser, PermissionRequirement, PermissionRouteExt, bearer_token};
    use crate::{PermissionCode, RoleCode};

    fn permission(code: &str) -> PermissionCode {
        PermissionCode::try_from(code).expect("test permission code must be valid")
    }

    fn user_with_permissions(permissions: &[&str]) -> AuthedUser {
        AuthedUser {
            tenant_id: Uuid::from_u128(1),
            account_id: Uuid::from_u128(2),
            username: "route-user".to_owned(),
            email: None,
            primary_role: RoleCode::try_from("member").expect("test role must be valid"),
            roles: Vec::new(),
            permissions: permissions.iter().map(|code| permission(code)).collect(),
            branch_ids: Vec::new(),
            active_branch_id: None,
            authz_roles: Vec::new(),
            authz_perms: Vec::new(),
        }
    }

    #[test]
    fn one_permission_requires_the_declared_code() {
        let requirement: PermissionRequirement = PermissionRequirement::one("business.records.read");
        assert!(requirement.allows(&user_with_permissions(&["business.records.read"])));
        assert!(!requirement.allows(&user_with_permissions(&["business.records.manage"])));
    }

    #[test]
    fn any_and_all_have_distinct_semantics() {
        let user: AuthedUser = user_with_permissions(&["business.records.read"]);
        assert!(PermissionRequirement::any(["business.records.read", "business.records.manage"]).allows(&user));
        assert!(!PermissionRequirement::all(["business.records.read", "business.records.manage"]).allows(&user));
    }

    async fn successful_route() -> StatusCode {
        StatusCode::NO_CONTENT
    }

    #[tokio::test]
    async fn method_route_layer_enforces_any_and_all_requirements() {
        let any_router: Router = Router::new()
            .route(
                "/",
                get(successful_route).require_any(["business.records.read", "business.records.manage"]),
            )
            .layer(Extension(user_with_permissions(&["business.records.read"])));
        let any_request: Request<Body> = Request::builder()
            .uri("/")
            .body(Body::empty())
            .expect("test request must be valid");
        let any_response: Response = any_router
            .oneshot(any_request)
            .await
            .expect("infallible Axum route must respond");
        assert_eq!(any_response.status(), StatusCode::NO_CONTENT);

        let all_router: Router = Router::new()
            .route(
                "/",
                get(successful_route).require_all(["business.records.read", "business.records.manage"]),
            )
            .layer(Extension(user_with_permissions(&["business.records.read"])));
        let all_request: Request<Body> = Request::builder()
            .uri("/")
            .body(Body::empty())
            .expect("test request must be valid");
        let all_response: Response = all_router
            .oneshot(all_request)
            .await
            .expect("infallible Axum route must respond");
        assert_eq!(all_response.status(), StatusCode::FORBIDDEN);
    }

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
