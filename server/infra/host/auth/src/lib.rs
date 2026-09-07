#![cfg_attr(debug_assertions, allow(unused))]

mod codes;
pub use codes::{AuthCodeError, PermissionCode, RoleCode};

cfg_if::cfg_if! {
if #[cfg(feature = "ext-service")] {
    pub mod ext_service;
    mod service;
    pub use service::{AuthService, AuthSvcErr};
}
}
