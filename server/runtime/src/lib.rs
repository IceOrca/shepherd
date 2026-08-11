#![cfg_attr(debug_assertions, allow(unused))]

use std::sync::Arc;

use axum::Router;
use infra_host::HostContext as FoundationHostState;

pub async fn build() -> (Arc<FoundationHostState>, Router) {
    let infra: Arc<FoundationHostState> = FoundationHostState::new_arc().await;
    let shepherd: Arc<shepherd::AppContext> =
        shepherd::AppContext::new_arc(Arc::clone(&infra.auth), Arc::clone(&infra.database));

    let router: Router = infra_host::route::routes(Arc::clone(&infra)).merge(shepherd::routes(shepherd));
    let router: Router = infra_host::route::apply_layers(router, Arc::clone(&infra));
    (infra, router)
}

pub fn typescript_contract() -> String {
    format!(
        "// This file is generated from Rust API DTOs. Do not edit it manually.\n\n{}{}",
        infra_auth::typescript::contract(),
        shepherd::typescript::contract()
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn typescript_contract_uses_json_wire_types() {
        let contract = super::typescript_contract();

        assert!(contract.contains("export type AuthRequest"));
        assert!(contract.contains("export type StaffingShiftCreateRequest"));
        assert!(contract.contains("worked_seconds: number"));
        assert!(!contract.contains("bigint"));
    }
}
