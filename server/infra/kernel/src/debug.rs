use std::sync::{Mutex, OnceLock};
use tracing_subscriber::{EnvFilter, Registry, fmt, prelude::*, reload};

type FilterHandle = reload::Handle<EnvFilter, Registry>;
static FILTER: OnceLock<Mutex<(FilterHandle, String)>> = OnceLock::new();

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
        let _ = FILTER.set(Mutex::new((handle, text)));
        tracing::info!("Tracing system initialized with runtime filter reload support");
    }

    pub fn filter() -> Result<String, String> {
        let state: std::sync::MutexGuard<'_, (FilterHandle, String)> = FILTER
            .get()
            .ok_or("Tracing is not initialized")?
            .lock()
            .map_err(|_| "Tracing filter lock is unavailable")?;
        Ok(state.1.clone())
    }

    pub fn set_level(level: &str) -> Result<String, String> {
        if !matches!(level, "error" | "warn" | "info" | "debug" | "trace") {
            return Err("Unsupported log level".to_owned());
        }
        // Third-party HTTP/wire tracing can include credentials. Keep those
        // libraries at warn while changing Shepherd and reusable infra targets.
        let filter_text: String = format!(
            "warn,shepherd={level},shepherd_runtime={level},infra_auth={level},infra_host={level},infra_kernel={level},infra_postgres={level},infra_worker={level},supabase_auth={level}"
        );
        let filter: EnvFilter = EnvFilter::try_new(&filter_text).map_err(|_| "Invalid tracing filter")?;
        let mut state: std::sync::MutexGuard<'_, (FilterHandle, String)> = FILTER
            .get()
            .ok_or("Tracing is not initialized")?
            .lock()
            .map_err(|_| "Tracing filter lock is unavailable")?;
        state.0.reload(filter).map_err(|_| "Tracing reload is unavailable")?;
        state.1 = filter_text.clone();
        Ok(filter_text)
    }
}
