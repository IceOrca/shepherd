use std::sync::Arc;

use axum::{
    response::IntoResponse,
    extract::{Extension, Json, State},
    http::StatusCode,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Serialize;

use crate::jwt;
use crate::AuthService;

use infra_kernel::debug::*;

#[derive(Serialize)]
pub struct JwksResponse {
    keys: Vec<JwkKey>,
}

#[derive(Serialize)]
pub struct JwkKey {
    kty: String, // key type
    crv: String, // curve
    #[serde(rename = "use")]
    use_: String, // usage
    kid: String, // key id — for rotation
    x: String,   // base64url encoded public key bytes
}

pub async fn jwks_handler(State(auth_ctx): State<Arc<AuthService>>) -> impl IntoResponse {
    let public_key: &[u8] = auth_ctx.jwt.key_public();

    let jwk: JwkKey = JwkKey {
        kty: "OKP".to_string(),
        crv: "Ed25519".to_string(),
        use_: "sig".to_string(),
        kid: jwt::KID_MAIN!().to_string(),
        x: URL_SAFE_NO_PAD.encode(public_key),
    };

    Json(JwksResponse { keys: vec![jwk] })
}
