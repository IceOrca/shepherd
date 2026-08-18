use std::sync::{Arc, Weak};

use axum::{
    Router,
    extract::{ConnectInfo, Request},
    http::{HeaderMap, StatusCode},
    response::{Response, IntoResponse},
    middleware,
    middleware::Next,
    middleware::{from_fn, from_fn_with_state},
    extract::{Extension, Json, State},
};
use std::net::{IpAddr, SocketAddr};
use axum::http::header::{HeaderValue, InvalidHeaderValue};

use crate::HostContext;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use super::{
    dto::{CitHeader, CitPayload},
    client_token::{ClientTokenHandle, ClientTokenError, ClientTokenKey},
};

use crate::ip_extract::OriginatorIp;

use tracing::{error, warn, info, debug, trace};

pub async fn client_init(
    State(ctx): State<Arc<ClientTokenHandle>>,
    Extension(real_ip): Extension<OriginatorIp>,
) -> Result<impl IntoResponse, StatusCode> {
    if let Ok(token) = ctx.generate(&real_ip) {
        info!("Client token issued to ip={}", real_ip.ip());

        Ok((
            StatusCode::OK,
            Json(serde_json::json!({
                "client_token": token,
                "token_type":   "CIT",
            })),
        ))
    } else {
        Ok((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "msg": "Try again later",
            })),
        ))
    }
}

#[derive(Serialize)]
pub struct CtksResponse {
    keys: Vec<CtkKey>,
}

#[derive(Serialize)]
pub struct CtkKey {
    kty: String, // key type
    crv: String, // curve
    #[serde(rename = "use")]
    use_: String, // usage
    kid: String, // key id — for rotation
    x: String,   // base64url encoded public key bytes
}

pub async fn ctks_handler(State(ctx): State<Arc<ClientTokenHandle>>) -> Result<impl IntoResponse, StatusCode> {
    let public_key: &Vec<u8> = ctx.key_public();

    let jwk: CtkKey = CtkKey {
        kty: "OKP".to_string(),
        crv: "Ed25519".to_string(),
        use_: "sig".to_string(),
        kid: "infra-client-cti-1".to_string(),
        x: URL_SAFE_NO_PAD.encode(public_key),
    };

    Ok((StatusCode::OK, Json(CtksResponse { keys: vec![jwk] })))
}
