#![cfg_attr(debug_assertions, allow(unused))]

use std::{net::SocketAddr, path::Path, time::Duration};

use infra_kernel::debug::Debugging;
use tokio::signal;
use tracing::{error, info, warn};

const DEFAULT_WORKER_SHUTDOWN_TIMEOUT_SECS: u64 = 60;

#[tokio::main]
async fn main() {
    load_environment();
    Debugging::init();

    let shepherd_runtime::RuntimeParts {
        context,
        router,
        worker,
    } = shepherd_runtime::build().await;
    let worker_shutdown_timeout: Duration = Duration::from_secs(positive_env_u64(
        "WORKER_SHUTDOWN_TIMEOUT_SECS",
        DEFAULT_WORKER_SHUTDOWN_TIMEOUT_SECS,
    ));
    info!(
        worker_shutdown_timeout_secs = worker_shutdown_timeout.as_secs(),
        "Resolved background worker shutdown timeout"
    );
    info!("Starting server on {}:{}", context.ip, context.port);

    let address: String = format!("{}:{}", context.ip, context.port);
    let result = axum::serve(
        tokio::net::TcpListener::bind(&address)
            .await
            .unwrap_or_else(|error| panic!("failed to bind server to {address}: {error}")),
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await;
    if let Err(timeout_error) = worker.shutdown_with_timeout(worker_shutdown_timeout).await {
        error!(
            timeout_ms = timeout_error.timeout().as_millis(),
            error = %timeout_error,
            "Background worker graceful shutdown timed out; remaining asynchronous tasks will be aborted with the runtime"
        );
    } else {
        info!("Background worker shutdown completed");
    }
    result.unwrap_or_else(|error| panic!("server failed: {error}"));
}

fn positive_env_u64(name: &str, default: u64) -> u64 {
    let raw_value: String = match std::env::var(name) {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return default,
        Err(std::env::VarError::NotUnicode(_value)) => {
            warn!(
                configuration = name,
                default, "Configuration is not valid Unicode; using default"
            );
            return default;
        }
    };
    match raw_value.parse::<u64>() {
        Ok(value) if value > 0 => value,
        Ok(_zero) => {
            warn!(
                configuration = name,
                default, "Configuration must be greater than zero; using default"
            );
            default
        }
        Err(error) => {
            warn!(
                configuration = name,
                default,
                error = %error,
                "Configuration is not an unsigned integer; using default"
            );
            default
        }
    }
}

fn load_environment() {
    if std::env::var("APP_ENV").as_deref() == Ok("development") {
        dotenvy::dotenv().ok();
    } else {
        dotenvy::from_path(Path::new("/run/secrets/server_prod_env"))
            .unwrap_or_else(|error: dotenvy::Error| panic!("production environment file is unavailable: {error}"));
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .unwrap_or_else(|error: std::io::Error| panic!("failed to install Ctrl+C handler: {error}"));
    };

    #[cfg(unix)]
    {
        let mut terminate: signal::unix::Signal = signal::unix::signal(signal::unix::SignalKind::terminate())
            .unwrap_or_else(|error: std::io::Error| panic!("failed to install SIGTERM handler: {error}"));
        tokio::select! {
            _ = ctrl_c => info!("Ctrl+C received, shutting down gracefully"),
            _ = terminate.recv() => info!("SIGTERM received, shutting down gracefully"),
        }
    }

    #[cfg(not(unix))]
    ctrl_c.await;
}
