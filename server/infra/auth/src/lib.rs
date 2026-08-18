#![cfg_attr(debug_assertions, allow(unused))]
cfg_if::cfg_if!(
    if #[cfg(feature = "internal-api")] {
        pub mod internal_api;
        pub use internal_api::*;
    } else {
        pub mod ext_foundation;
        mod service;
        pub use service::{AuthService, AuthServiceError};
    }
);
