#![cfg_attr(debug_assertions, allow(unused))]

use std::sync::Arc;

use axum::Router;
use infra_host::HostContext as HostInfa;
use infra_worker::Worker;

pub struct RuntimeParts {
    pub context: Arc<HostInfa>,
    pub router: Router,
    pub worker: Worker,
}

pub async fn build() -> RuntimeParts {
    let infra: Arc<HostInfa> = HostInfa::new_arc().await;
    let app: Arc<shepherd::AppContext> =
        shepherd::AppContext::new_arc(Arc::clone(&infra.auth), Arc::clone(&infra.database));
    let dispatcher: Arc<shepherd::notifications::NotificationDispatcher> = Arc::clone(&app.notifications);
    let worker: Worker = Worker::new();
    worker
        .asynchronous()
        .spawn("notification-outbox", move |cancellation| async move {
            dispatcher.run(cancellation).await;
        })
        .unwrap_or_else(|error| panic!("failed to start notification dispatcher: {error}"));

    let router: Router = infra_host::route::routes(Arc::clone(&infra)).merge(shepherd::routes(app));
    let router: Router = infra_host::route::apply_layers(router, Arc::clone(&infra));
    RuntimeParts {
        context: infra,
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
