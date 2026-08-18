use axum::body::Body;
use axum::extract::{MatchedPath, Request};
use axum::http::header;
use axum::{middleware::Next, response::Response};
use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, MakeSpan, TraceLayer};
use tracing::{Level, Span, info_span};
use tracing::{error, warn, info, debug, trace};
use tracing_subscriber::{EnvFilter, Registry, fmt, prelude::*};

pub struct Debugging;

impl Debugging {
    pub fn init() {
        let tracing_registry: Registry = tracing_subscriber::registry();
        tracing_registry
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                println!("RUST_LOG environment variable not set, defaulting to 'info' level");
                "info".into()
            }))
            .with(
                fmt::layer()
                    .with_level(true) // log level
                    .with_target(true) // module name
                    .with_file(false) // file name
                    .with_line_number(true) // line number
                    .with_thread_ids(true) // thread IDs (multithreading)
                    .pretty(), // Format with colors and newlines for better readability in console
            )
            .init();

        tracing::info!("Tracing System initialized");
    }
}
