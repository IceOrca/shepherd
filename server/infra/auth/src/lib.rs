#![cfg_attr(debug_assertions, allow(unused))]

#[cfg(any(feature = "jwt-decode", feature = "password-auth"))]
pub mod account;
#[cfg(feature = "jwt-decode")]
pub mod dto;
#[cfg(feature = "jwt-decode")]
mod feature;
#[cfg(feature = "jwt-decode")]
pub mod jwt;
#[cfg(feature = "keycloak")]
pub mod keycloak;
#[cfg(feature = "jwt-decode")]
pub mod middleware;

#[cfg(feature = "brute-force")]
pub mod brute_force;
#[cfg(feature = "legacy-api")]
pub mod handler;
#[cfg(feature = "jwks")]
pub mod jwks;
#[cfg(feature = "password-auth")]
pub mod password_auth;
#[cfg(feature = "legacy-api")]
pub mod route;
#[cfg(feature = "session")]
pub mod session;
#[cfg(feature = "session-revocation")]
pub mod session_revoke;
#[cfg(feature = "legacy-api")]
pub mod typescript;

#[cfg(feature = "jwt-decode")]
pub use feature::{AuthService, AuthenticatedUser, TenantContext};

#[cfg(feature = "legacy-api")]
pub use feature::AuthFeature;
#[cfg(feature = "password-auth")]
pub use password_auth::{
    AccountMutationError, AccountRepo, AuthMngtEntity, AuthProvider, AuthenticateUserError, ChangeOwnPasswordError,
    CreateAccountError, DynAccountRepo, StoreAccountError,
};

// Compatibility paths for applications that enable the complete legacy auth API.
#[cfg(feature = "brute-force")]
pub use brute_force as bruteforce;
#[cfg(feature = "password-auth")]
pub use password_auth::postgres;
#[cfg(feature = "session-revocation")]
pub use session_revoke as access_revocation;
#[cfg(feature = "session-revocation")]
pub use session_revoke::token_blacklist as token_blacklist_pubsub;
