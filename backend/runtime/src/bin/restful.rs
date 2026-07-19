use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use foundation_kernel::debug::*;
use foundation_host::HostContext;
use tokio::signal;

#[tokio::main]
async fn main() {
    load_environment();
    Debugging::init();

    let (context, router): (Arc<HostContext>, Router) = shepherd_runtime::build().await;
    log_notice!("Starting server on {}:{}", context.ip, context.port);

    let address: String = format!("{}:{}", context.ip, context.port);
    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .unwrap_or_else(|error| panic!("failed to bind server to {address}: {error}"));
    axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap_or_else(|error| panic!("server failed: {error}"));
}

fn load_environment() {
    if std::env::var("APP_ENV").as_deref() == Ok("development") {
        dotenvy::dotenv().ok();
    } else {
        dotenvy::from_path(Path::new("/run/secrets/server_prod_env"))
            .unwrap_or_else(|error| panic!("production environment file is unavailable: {error}"));
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .unwrap_or_else(|error| panic!("failed to install Ctrl+C handler: {error}"));
    };

    #[cfg(unix)]
    {
        let mut terminate = signal::unix::signal(signal::unix::SignalKind::terminate())
            .unwrap_or_else(|error| panic!("failed to install SIGTERM handler: {error}"));
        tokio::select! {
            _ = ctrl_c => log_notice!("Ctrl+C received, shutting down gracefully"),
            _ = terminate.recv() => log_notice!("SIGTERM received, shutting down gracefully"),
        }
    }

    #[cfg(not(unix))]
    ctrl_c.await;
}
