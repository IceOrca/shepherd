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

#[derive(Clone, Debug)]
pub struct ListPaginationPolicy {
    pub default_limit: u16,
    pub minimum_limit: u16,
    pub maximum_limit: u16,
}

impl ListPaginationPolicy {
    pub fn try_new(default_limit: u16, minimum_limit: u16, maximum_limit: u16) -> Result<Self, String> {
        if minimum_limit == 0 || minimum_limit > default_limit || default_limit > maximum_limit {
            return Err("list pagination requires 0 < minimum <= default <= maximum".to_owned());
        }
        Ok(Self {
            default_limit,
            minimum_limit,
            maximum_limit,
        })
    }

    pub fn resolve(&self, requested: Option<u16>) -> Result<u16, String> {
        let limit: u16 = requested.unwrap_or(self.default_limit);
        if limit < self.minimum_limit || limit > self.maximum_limit {
            return Err(format!(
                "limit must be between {} and {}",
                self.minimum_limit, self.maximum_limit
            ));
        }
        Ok(limit)
    }
}

pub fn identity_routes(auth: Arc<AuthService>) -> Router {
    account::identity_routes(auth)
}

pub fn routes(auth: Arc<AuthService>, policy: auth_admin::AuthAdminPolicy, pagination: ListPaginationPolicy) -> Router {
    let provisioner: Arc<dyn auth_admin::AuthAccountProvisioner> = Arc::new(auth_admin::NoopAuthAccountProvisioner);
    account::routes(Arc::clone(&auth))
        .merge(auth_admin::routes_with_provisioner(
            Arc::clone(&auth),
            policy.clone(),
            Arc::clone(&provisioner),
            pagination.clone(),
        ))
        .merge(access_control::routes(auth, policy, provisioner, pagination))
}

pub fn routes_with_provisioner(
    auth: Arc<AuthService>,
    policy: auth_admin::AuthAdminPolicy,
    provisioner: Arc<dyn auth_admin::AuthAccountProvisioner>,
    pagination: ListPaginationPolicy,
) -> Router {
    account::routes(Arc::clone(&auth))
        .merge(auth_admin::routes_with_provisioner(
            Arc::clone(&auth),
            policy.clone(),
            Arc::clone(&provisioner),
            pagination.clone(),
        ))
        .merge(access_control::routes(auth, policy, provisioner, pagination))
}
