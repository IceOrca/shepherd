#![cfg_attr(debug_assertions, allow(unused))]
use serde::Serialize;

/// Static metadata exposed by an application compiled into the runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct AppManifest {
    pub code: &'static str,
    pub display_name: &'static str,
    pub version: &'static str,
    pub dependencies: &'static [&'static str],
}

/// Contract implemented by each application composition crate.
pub trait InfraAppManifest: Send + Sync + 'static {
    fn manifest(&self) -> AppManifest;
}
