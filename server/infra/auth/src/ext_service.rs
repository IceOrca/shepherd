//! Provider-neutral access-token validation through a remote JWKS endpoint.
//!
//! This module authenticates an OIDC subject and maps it to the reusable
//! tenant/account authorization model. Applications provide their permission
//! codes when mounting administration routes.

use std::sync::Arc;

use axum::Router;

use crate::AuthService;

pub mod access_control;
pub mod account;
pub(crate) mod account_cache;
pub mod auth_admin;
mod claims;
mod config;
mod error;
pub mod middleware;
mod service;

pub use claims::{AccessTokenClaims, Audience, AuthenticatedPrincipal};
pub use account_cache::AuthenticatedUserCacheConfigError;
pub use config::OidcJwksVerifierConfig;
pub use error::AccessTokenError;
pub use service::OidcJwksVerifier;

pub fn identity_routes(auth: Arc<AuthService>) -> Router {
    account::identity_routes(auth)
}

pub fn routes(auth: Arc<AuthService>, policy: auth_admin::AuthAdminPolicy) -> Router {
    let provisioner: Arc<dyn auth_admin::AuthAccountProvisioner> = Arc::new(auth_admin::NoopAuthAccountProvisioner);
    account::routes(Arc::clone(&auth))
        .merge(auth_admin::routes_with_provisioner(
            Arc::clone(&auth),
            policy.clone(),
            Arc::clone(&provisioner),
        ))
        .merge(access_control::routes(auth, policy, provisioner))
}

pub fn routes_with_provisioner(
    auth: Arc<AuthService>,
    policy: auth_admin::AuthAdminPolicy,
    provisioner: Arc<dyn auth_admin::AuthAccountProvisioner>,
) -> Router {
    account::routes(Arc::clone(&auth))
        .merge(auth_admin::routes_with_provisioner(
            Arc::clone(&auth),
            policy.clone(),
            Arc::clone(&provisioner),
        ))
        .merge(access_control::routes(auth, policy, provisioner))
}
