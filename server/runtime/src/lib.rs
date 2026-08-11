use std::sync::Arc;

use axum::{Json, Router, routing::get};
use infra_host::HostContext as FoundationHostState;
use utoipa::OpenApi;

pub async fn build() -> (Arc<FoundationHostState>, Router) {
    let infra: Arc<FoundationHostState> = FoundationHostState::new_arc().await;
    let hrm: Arc<shepherd::AppContext> =
        shepherd::AppContext::new_arc(Arc::clone(&infra.auth), Arc::clone(&infra.database));

    let router: Router = infra_host::route::routes(Arc::clone(&infra))
        .merge(shepherd::routes(hrm))
        .route("/openapi.json", get(openapi_json));
    let router: Router = infra_host::route::apply_layers(router, Arc::clone(&infra));
    (infra, router)
}

pub fn api_document() -> utoipa::openapi::OpenApi {
    let mut document: utoipa::openapi::OpenApi = infra_auth::openapi::AuthApiDoc::openapi();
    document.merge(shepherd::openapi::ShepherdApiDoc::openapi());
    document
}

async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(api_document())
}
