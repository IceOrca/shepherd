use std::sync::{Arc, Weak};

use axum::{
    Router,
    http::StatusCode,
    extract::{ConnectInfo, Request},
    http::HeaderMap,
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

use infra_kernel::debug::*;

#[derive(Clone, Debug)]
pub struct VerifiedClient {
    pub id: String, // UUID — rate limit key
}

pub async fn client_token_layer(
    State(ctx): State<Arc<ClientTokenHandle>>,
    Extension(real_ip): Extension<OriginatorIp>,
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<impl IntoResponse, StatusCode> {
    let raw_token: &str = headers
        .get("X-Client-ID")
        .and_then(|v: &HeaderValue| v.to_str().ok())
        .ok_or_else(|| {
            log_notice!("Missing X-Client-ID");
            StatusCode::BAD_REQUEST
        })?;

    match ctx.verify(raw_token, &real_ip) {
        Ok(claims) => {
            req.extensions_mut().insert(VerifiedClient { id: claims.client_id });
            Ok(next.run(req).await)
        }

        Err(e) => {
            log_notice!("Invalid client token: {}", e);
            Ok((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "err":   "invalid_client_token",
                    "msg": "Access /client-init to get a valid token"
                })),
            )
                .into_response())
        }
    }
}

pub fn clientid_extract_layer(router: Router, state: Arc<ClientTokenHandle>) -> Router {
    let host_router: Router = router.route_layer(from_fn_with_state(state, client_token_layer));
    host_router
}
