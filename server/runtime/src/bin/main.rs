#![cfg_attr(debug_assertions, allow(unused))]

use std::net::SocketAddr;
use std::path::Path;

use tracing::{error, warn, info, debug, trace};
use infra_kernel::debug::Debugging;
use tokio::signal;

#[tokio::main]
async fn main() {
    load_environment();
    Debugging::init();

    let shepherd_runtime::RuntimeParts {
        context,
        router,
        worker,
    } = shepherd_runtime::build().await;
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
    worker.shutdown().await;
    result.unwrap_or_else(|error| panic!("server failed: {error}"));
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
