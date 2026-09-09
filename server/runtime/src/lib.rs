#![cfg_attr(debug_assertions, allow(unused))]

use std::{io, path::Path, sync::Arc};

use tokio_util::sync::CancellationToken;
use axum::Router;
use infra_host::HostInfa;
use infra_worker::Worker;
use tracing::{error, warn, info, debug, trace};

pub struct RuntimeParts {
    pub host: Arc<HostInfa>,
    pub router: Router,
    pub worker: Worker,
}

/// Load the ordinary development dotenv file or the required mounted
/// production environment files. Production callers may add a tool-specific
/// secret after the shared server environment.
pub fn load_environment(additional_production_secrets: &[&str]) -> Result<(), io::Error> {
    if std::env::var("APP_ENV").as_deref() == Ok("production") {
        for path in std::iter::once("/run/secrets/server_prod_env").chain(additional_production_secrets.iter().copied())
        {
            dotenvy::from_path(Path::new(path)).map_err(|error: dotenvy::Error| {
                io::Error::other(format!("load production environment {path}: {error}"))
            })?;
        }
    } else {
        let _loaded: Result<std::path::PathBuf, dotenvy::Error> = dotenvy::dotenv();
    }
    Ok(())
}

pub async fn build() -> RuntimeParts {
    let auth_admin: Arc<supabase_auth::SupabaseAuthAdmin> = supabase_auth::SupabaseAuthAdmin::from_env()
        .unwrap_or_else(|error: supabase_auth::ConfigErr| {
            panic!("failed to initialize Supabase Auth identity administration: {error}")
        });
    let infra: Arc<HostInfa> = HostInfa::new_arc(auth_admin).await;
    let app: Arc<shepherd::AppContext> =
        shepherd::AppContext::new_arc(Arc::clone(&infra.auth), Arc::clone(&infra.database));
    let dispatcher: Arc<shepherd::notification::NotifyDispatcher> = Arc::clone(&app.notifications);
    let worker: Worker = Worker::new();
    worker
        .asynchronous()
        .spawn(
            "notification-outbox",
            move |cancellation: CancellationToken| async move {
                dispatcher.run(cancellation).await;
            },
        )
        .unwrap_or_else(|error: infra_worker::WorkerClosed| panic!("failed to start notification dispatcher: {error}"));

    let router: Router = infra_host::route::routes(Arc::clone(&infra)) // generic host routes
        .merge(shepherd::routes(app));
    let router: Router = infra_host::route::apply_layers(router, Arc::clone(&infra));
    RuntimeParts {
        host: infra,
        router,
        worker,
    }
}

pub fn typescript_contract() -> String {
    format!(
        "// This file is generated from Rust API DTOs. Do not edit it manually.\n\n{}",
        shepherd::typescript::contract()
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn typescript_contract_uses_json_wire_types() {
        let contract = super::typescript_contract();

        assert!(contract.contains("export type CurrentUserProfile"));
        assert!(contract.contains("export type StaffingShiftCreateRequest"));
        assert!(contract.contains("worked_seconds: number"));
        assert!(!contract.contains("bigint"));
    }
}
