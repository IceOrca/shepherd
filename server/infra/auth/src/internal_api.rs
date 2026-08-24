//! Legacy Shepherd internal-auth implementation retained as reference material.
//!
//! This module is intentionally not declared by `lib.rs` and has no Cargo
//! feature. It is not part of the reusable `infra-auth` crate's compiled API.

pub mod account;
pub mod brute_force;
pub mod dto;
pub mod feature;
pub mod handler;
pub mod jwks;
pub mod jwt;
pub mod middleware;
pub mod password_auth;
pub mod route;
pub mod session;
pub mod session_revoke;
pub mod typescript;

pub use brute_force as bruteforce;
pub use feature::{AuthFeature, AuthenticatedUser, LegacyAuthService, TenantContext};
pub use password_auth::*;
pub use session_revoke as access_revocation;
