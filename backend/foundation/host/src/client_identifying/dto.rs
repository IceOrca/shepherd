use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::{
    net::IpAddr,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

// ─────────────────────────────────────────
// Token structure
// ─────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct CitHeader {
    pub alg: String, // "EdDSA"
    pub typ: String, // "CIT"
}

#[derive(Serialize, Deserialize)]
pub struct CitPayload {
    /// Client ID — UUID
    pub cid: String,
    /// Issued at — Unix timestamp
    pub iat: u64,
    /// Expiration Time
    pub exp: u64,
    /// Originator Client IP
    pub cip: IpAddr,
}

#[derive(Debug, Clone)]
pub struct ClientIdToken {
    pub client_id: String, // UUID — use as rate limit key
}
