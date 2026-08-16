#![cfg_attr(debug_assertions, allow(unused))]

#[cfg(any(feature = "jwt", feature = "password-auth"))]
#[path = "internal_api/account.rs"]
pub mod account;
#[cfg(feature = "jwt")]
#[path = "internal_api/dto.rs"]
pub mod dto;
#[cfg(feature = "ext-foundation")]
pub mod ext_foundation;
#[cfg(feature = "jwt")]
#[path = "internal_api/feature.rs"]
mod feature;
#[cfg(feature = "jwt")]
#[path = "internal_api/jwt.rs"]
pub mod jwt;
#[cfg(feature = "jwt")]
#[path = "internal_api/middleware.rs"]
pub mod middleware;
#[cfg(feature = "ext-foundation")]
mod service;

#[cfg(feature = "brute-force")]
#[path = "internal_api/brute_force.rs"]
pub mod brute_force;
#[cfg(feature = "internal-api")]
#[path = "internal_api/handler.rs"]
pub mod handler;
#[cfg(feature = "jwks")]
#[path = "internal_api/jwks.rs"]
pub mod jwks;
#[cfg(feature = "password-auth")]
#[path = "internal_api/password_auth.rs"]
pub mod password_auth;
#[cfg(feature = "internal-api")]
#[path = "internal_api/route.rs"]
pub mod route;
#[cfg(feature = "session")]
#[path = "internal_api/session.rs"]
pub mod session;
#[cfg(feature = "session-revocation")]
#[path = "internal_api/session_revoke.rs"]
pub mod session_revoke;
#[cfg(feature = "internal-api")]
#[path = "internal_api/typescript.rs"]
pub mod typescript;

#[cfg(feature = "jwt")]
pub use feature::{AuthenticatedUser, LegacyAuthService, TenantContext};
#[cfg(feature = "ext-foundation")]
pub use service::{AuthService, AuthServiceError};

#[cfg(feature = "internal-api")]
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
