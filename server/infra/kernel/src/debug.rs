use std::sync::OnceLock;
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use tracing_subscriber::{EnvFilter, Registry, fmt, prelude::*, reload};

type FilterHandle = reload::Handle<EnvFilter, Registry>;
static FILTER: OnceLock<RwLock<LogMutationCtx>> = OnceLock::new();

/// Reload authority and its current projection are owned by the same lock.
/// This context cannot be constructed or cloned outside this module.
///
/// A read guard cannot change the filter:
/// ```compile_fail
/// # async fn example() -> Result<(), String> {
/// let mut context = infra_kernel::debug::Debugging::read().await?;
/// context.set_level("debug")?;
/// # Ok(())
/// # }
/// ```
pub struct LogMutationCtx {
    handle: FilterHandle,
    filter_text: String,
}

pub struct Debugging;

impl Debugging {
    pub fn init() {
        let filter: EnvFilter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        let text: String = filter.to_string();
        let (layer, handle) = reload::Layer::new(filter);
        tracing_subscriber::registry()
            .with(layer)
            .with(
                fmt::layer()
                    .with_level(true)
                    .with_target(true)
                    .with_file(false)
                    .with_line_number(true)
                    .with_thread_ids(true)
                    .pretty(),
            )
            .init();
        let _ = FILTER.set(RwLock::new(LogMutationCtx {
            handle,
            filter_text: text,
        }));
        tracing::info!("Tracing system initialized with runtime filter reload support");
    }

    pub async fn read() -> Result<RwLockReadGuard<'static, LogMutationCtx>, String> {
        Ok(FILTER.get().ok_or("Tracing is not initialized")?.read().await)
    }

    /// Hold this guard across the entire read/audit/reload workflow.
    pub async fn write() -> Result<RwLockWriteGuard<'static, LogMutationCtx>, String> {
        Ok(FILTER.get().ok_or("Tracing is not initialized")?.write().await)
    }
}

impl LogMutationCtx {
    pub fn filter(&self) -> &str {
        &self.filter_text
    }

    pub fn set_level(&mut self, level: &str) -> Result<String, String> {
        if !matches!(level, "error" | "warn" | "info" | "debug" | "trace") {
            return Err("Unsupported log level".to_owned());
        }
        // Third-party HTTP/wire tracing can include credentials. Keep those
        // libraries at warn while changing Shepherd and reusable infra targets.
        let filter_text: String = format!(
            "warn,shepherd={level},shepherd_runtime={level},infra_auth={level},infra_host={level},infra_kernel={level},infra_postgres={level},infra_worker={level},supabase_auth={level}"
        );
        let filter: EnvFilter = EnvFilter::try_new(&filter_text).map_err(|_| "Invalid tracing filter")?;
        self.handle
            .reload(filter)
            .map_err(|_| "Tracing reload is unavailable")?;
        self.filter_text = filter_text.clone();
        Ok(filter_text)
    }
}

#[cfg(test)]
#[path = "debug/tests.rs"]
mod tests;
