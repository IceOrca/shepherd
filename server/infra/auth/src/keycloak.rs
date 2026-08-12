//! Keycloak access-token validation through the realm JWKS endpoint.
//!
//! This module authenticates an OIDC subject. Mapping that subject to a
//! Shepherd tenant/account and evaluating application permissions belong to
//! application code.

mod claims;
mod config;
mod error;
pub mod middleware;
mod service;

pub use claims::{Audience, KeycloakClaims, KeycloakPrincipal, RealmAccess, ResourceAccess};
pub use config::KeycloakConfig;
pub use error::KeycloakAuthError;
pub use service::KeycloakAuth;
