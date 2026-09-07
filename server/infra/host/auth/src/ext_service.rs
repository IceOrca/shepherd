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

pub use claims::{AccessTokenClaims, Audience, AuthedPrincipal};
pub use account_cache::AuthedCacheCfgErr;
pub use config::OidcJwksVerifierCfg;
pub use error::AccessTokenErr;
pub use service::OidcJwksVerifier;

#[derive(Clone, Debug)]
pub struct ListPaginationPolicy {
    pub def_limit: u16,
    pub min_limit: u16,
    pub max_limit: u16,
}

impl ListPaginationPolicy {
    pub fn try_new(def_limit: u16, min_limit: u16, max_limit: u16) -> Result<Self, String> {
        if min_limit == 0 || min_limit > def_limit || def_limit > max_limit {
            return Err("list pagination requires 0 < minimum <= default <= maximum".to_owned());
        }
        Ok(Self {
            def_limit,
            min_limit,
            max_limit,
        })
    }

    pub fn resolve(&self, requested: Option<u16>) -> Result<u16, String> {
        let limit: u16 = requested.unwrap_or(self.def_limit);
        if limit < self.min_limit || limit > self.max_limit {
            return Err(format!(
                "limit must be between {} and {}",
                self.min_limit, self.max_limit
            ));
        }
        Ok(limit)
    }
}

pub fn identity_routes(auth: Arc<AuthService>) -> Router {
    account::identity_routes(auth)
}

pub fn routes_with_provisioner(
    auth: Arc<AuthService>,
    policy: auth_admin::AuthAdminPolicy,
    provisioner: Arc<dyn auth_admin::AuthProvisioner>,
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
