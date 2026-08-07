use std::sync::Arc;

use axum::{Json, Router, routing::get};
use foundation_host::HostContext as FoundationHostState;
use utoipa::OpenApi;

pub async fn build() -> (Arc<FoundationHostState>, Router) {
    let foundation: Arc<FoundationHostState> = FoundationHostState::new_arc().await;
    let hrm: Arc<app_hrm::AppContext> =
        app_hrm::AppContext::new_arc(Arc::clone(&foundation.auth), Arc::clone(&foundation.database));

    let router: Router = foundation_host::route::routes(Arc::clone(&foundation))
        .merge(app_hrm::routes(hrm))
        .route("/openapi.json", get(openapi_json));
    let router: Router = foundation_host::route::apply_layers(router, Arc::clone(&foundation));
    (foundation, router)
}

pub fn api_document() -> utoipa::openapi::OpenApi {
    let mut document: utoipa::openapi::OpenApi = foundation_auth::openapi::AuthApiDoc::openapi();
    document.merge(app_hrm::openapi::HrmApiDoc::openapi());
    document
}

async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(api_document())
}
