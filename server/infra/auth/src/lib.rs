#![cfg_attr(debug_assertions, allow(unused))]

#[cfg(any(feature = "jwt", feature = "password-auth"))]
pub mod account;
#[cfg(feature = "jwt")]
pub mod dto;
#[cfg(feature = "jwt")]
mod feature;
#[cfg(feature = "jwt")]
pub mod jwt;
#[cfg(feature = "keycloak")]
pub mod keycloak;
#[cfg(feature = "jwt")]
pub mod middleware;
#[cfg(feature = "keycloak")]
mod service;

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

#[cfg(feature = "jwt")]
pub use feature::{AuthenticatedUser, LegacyAuthService, TenantContext};
#[cfg(feature = "keycloak")]
pub use service::AuthService;

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
