#![cfg_attr(debug_assertions, allow(unused))]

mod codes;
pub use codes::{AuthCodeError, PermissionCode, RoleCode};

#[cfg(feature = "ext-service")]
pub mod ext_service;
#[cfg(feature = "ext-service")]
mod service;
#[cfg(feature = "ext-service")]
pub use service::{AuthService, AuthSvcErr};
