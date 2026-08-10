use std::sync::{Arc, Weak};

use axum::{
    Router,
    extract::{ConnectInfo, Request},
    http::HeaderMap,
    response::Response,
    middleware,
    middleware::Next,
    middleware::{from_fn, from_fn_with_state},
    extract::{Extension, Json, State},
};
use std::net::{IpAddr, SocketAddr};
use axum::http::header::{HeaderValue, InvalidHeaderValue};

use crate::{HostContext, ip_extract::OriginatorIp};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use super::{
    dto::{ClientIdToken, CitHeader, CitPayload},
};

use infra_kernel::debug::*;

// ─────────────────────────────────────────
// Public types
// ─────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ClientTokenError {
    #[error("invalid token format")]
    InvalidFormat,
    #[error("invalid signature — token forged or tampered")]
    InvalidSignature,
    #[error("Expired token")]
    InvalidExpiration,
    #[error("Client ID and IP not match")]
    InvalidClientIp,
}

// ─────────────────────────────────────────
// Key handle
// ─────────────────────────────────────────

pub struct ClientTokenKey {
    /// private pem key
    signing_key: SigningKey,
    /// public pem key
    verifying_key: VerifyingKey,

    expiration_secs: u64,
    // raw bytes for CTKS
    public_key_bytes: Vec<u8>,
}

impl ClientTokenKey {
    pub fn new() -> Self {
        let expiration_secs: u64 = match std::env::var("CIT_EXPIRATION_SECS") {
            Ok(val) => val.parse().unwrap_or_else(|err: std::num::ParseIntError| {
                log_warn!("Invalid CIT_EXPIRATION_SECS format: {}, using default 216000s", err);
                216000
            }),
            Err(_) => {
                log_warn!("CIT_EXPIRATION_SECS not set, using default 216000s");
                216000
            }
        };

        let mut self_inst: Self = Self::load_from_pem(
            &std::env::var("CLIENT_TOKEN_PRIVATE_KEY_PATH").unwrap_or_else(|_| {
                log_warn!("CLIENT_TOKEN_PRIVATE_KEY_PATH not set, using default path");
                "./security/clienttokenkey/ctk_private.pem".into()
            }),
            &std::env::var("CLIENT_TOKEN_PUBLIC_KEY_PATH").unwrap_or_else(|_| {
                log_warn!("CLIENT_TOKEN_PUBLIC_KEY_PATH not set, using default path");
                "./security/clienttokenkey/ctk_public.pem".into()
            }),
        );

        self_inst.expiration_secs = expiration_secs;

        self_inst
    }

    pub fn load_from_pem(private_pem_path: &str, public_pem_path: &str) -> Self {
        use std::fs;

        let private_pem: String = match fs::read_to_string(private_pem_path) {
            Ok(data) => data,
            Err(err) => panic!(
                "Cannot read client token private key from {}: {}",
                private_pem_path, err
            ),
        };
        let public_pem: String = match fs::read_to_string(public_pem_path) {
            Ok(data) => data,
            Err(err) => panic!("Cannot read client token public key from {}: {}", public_pem_path, err),
        };

        let signing_key: SigningKey = extract_ed25519_private_pem(&private_pem);
        let verifying_key: VerifyingKey = extract_ed25519_public_pem(&public_pem);
        let public_key_bytes: Vec<u8> = verifying_key.to_bytes().to_vec();
        log_notice!("Client token keys loaded successfully");
        log_debug!("signing_key: {:?}", signing_key);
        log_debug!("verifying_key: {:?}", verifying_key);
        log_debug!(
            "public key bytes: {:02x?}",
            public_key_bytes
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<String>>()
        );

        Self {
            signing_key,
            verifying_key,
            public_key_bytes,
            expiration_secs: 0,
        }
    }

    // pub fn generate_ephemeral() -> Self {
    //     use rand::rngs::OsRng;

    //     let signing_key: SigningKey = SigningKey::generate(&mut OsRng);
    //     let verifying_key: VerifyingKey = signing_key.verifying_key();
    //     let public_key_bytes: Vec<u8> = verifying_key.to_bytes().to_vec();

    //     Self {
    //         signing_key,
    //         verifying_key,
    //         public_key_bytes,
    //         expiration_secs: 0,
    //     }
    // }
}

// ─────────────────────────────────────────
// Token operations
// ─────────────────────────────────────────

pub struct ClientTokenHandle {
    pub host_ctx: Weak<HostContext>, // parent
    key: ClientTokenKey,
}

impl ClientTokenHandle {
    pub fn new_arc(parent: &Weak<HostContext>) -> Arc<Self> {
        Arc::new(Self {
            host_ctx: Weak::clone(parent),
            key: ClientTokenKey::new(),
        })
    }

    pub fn key_public(&self) -> &Vec<u8> {
        &self.key.public_key_bytes
    }

    pub fn generate(&self, real_ip: &OriginatorIp) -> Result<String, String> {
        let key: &ClientTokenKey = &self.key;
        let header: CitHeader = CitHeader {
            alg: "EdDSA".to_string(),
            typ: "CIT".to_string(),
        };

        let now: u64 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err: std::time::SystemTimeError| {
                log_error!("System time error: {}", err);
                format!("System time error: {}", err)
            })?
            .as_secs() as u64;

        let payload: CitPayload = CitPayload {
            cid: Uuid::new_v4().to_string(),
            iat: now,
            exp: now + self.key.expiration_secs,
            cip: real_ip.ip(),
        };

        // Encode header + payload
        let header_b64: String =
            URL_SAFE_NO_PAD.encode(serde_json::to_string(&header).expect("convert json to str error"));
        let payload_b64: String =
            URL_SAFE_NO_PAD.encode(serde_json::to_string(&payload).expect("convert json to str error"));

        let signing_input: String = format!("{}.{}", header_b64, payload_b64);

        // Sign
        let signature: Signature = key.signing_key.sign(signing_input.as_bytes());
        let sig_b64: String = URL_SAFE_NO_PAD.encode(signature.to_bytes());

        Ok(format!("{}.{}", signing_input, sig_b64))
    }

    /// Verify and extract client_id
    pub fn verify(&self, token: &str, real_ip: &OriginatorIp) -> Result<ClientIdToken, ClientTokenError> {
        let key: &ClientTokenKey = &self.key;
        let parts: Vec<&str> = token.splitn(3, '.').collect();
        if parts.len() != 3 {
            return Err(ClientTokenError::InvalidFormat);
        }

        let header_b64: &str = parts.get(0).expect("get token[0] error after checked len");
        let payload_b64: &str = parts.get(1).expect("get token[1] error after checked len");
        let sig_b64: &str = parts.get(2).expect("get token[2] error after checked len");

        // Verify signature
        let signing_input: String = format!("{}.{}", header_b64, payload_b64);

        let sig_bytes: Vec<u8> = URL_SAFE_NO_PAD
            .decode(sig_b64)
            .map_err(|_| ClientTokenError::InvalidFormat)?;

        let signature: Signature = Signature::from_slice(&sig_bytes).map_err(|_| ClientTokenError::InvalidSignature)?;

        key.verifying_key
            .verify(signing_input.as_bytes(), &signature)
            .map_err(|_| ClientTokenError::InvalidSignature)?;

        // Decode header — verify typ and alg
        let header_json: Vec<u8> = URL_SAFE_NO_PAD
            .decode(header_b64)
            .map_err(|_| ClientTokenError::InvalidFormat)?;
        let header: CitHeader = serde_json::from_slice(&header_json).map_err(|_| ClientTokenError::InvalidFormat)?;

        if header.typ != "CIT" || header.alg != "EdDSA" {
            return Err(ClientTokenError::InvalidFormat);
        }

        // Decode payload
        let payload_json: Vec<u8> = URL_SAFE_NO_PAD
            .decode(payload_b64)
            .map_err(|_| ClientTokenError::InvalidFormat)?;
        let payload: CitPayload = serde_json::from_slice(&payload_json).map_err(|_| ClientTokenError::InvalidFormat)?;

        // Validate UUID format
        Uuid::try_parse(&payload.cid).map_err(|_| ClientTokenError::InvalidFormat)?;

        let now: u64 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err: std::time::SystemTimeError| {
                log_error!("System time error: {}", err);
                ClientTokenError::InvalidFormat
            })?
            .as_secs() as u64;
        if now < payload.iat || now > payload.exp {
            log_notice!("iat {} or exp {} is invalid, now {}", payload.iat, payload.exp, now);
            return Err(ClientTokenError::InvalidExpiration);
        }

        if payload.cip != real_ip.ip() {
            log_notice!(
                "cip {} in token and originator IP {} is not match",
                payload.cip,
                real_ip.ip()
            );
            return Err(ClientTokenError::InvalidClientIp);
        }

        Ok(ClientIdToken { client_id: payload.cid })
    }
}

// ─────────────────────────────────────────
// PEM parsing helpers
// ─────────────────────────────────────────

fn extract_ed25519_private_pem(pem: &str) -> SigningKey {
    use base64::engine::general_purpose::STANDARD;

    let b64: String = pem.lines().filter(|l: &&str| !l.starts_with("-----")).collect();
    let der: Vec<u8> = STANDARD.decode(b64).expect("Invalid base64 in private key PEM");

    // PKCS#8 Ed25519 private key
    // DER structure last 32 bytes = raw private key
    let raw: &[u8] = &der.get(der.len() - 32..).expect("Ed25519 private der length invalid");

    SigningKey::from_bytes(raw.try_into().expect("Invalid Ed25519 private key length"))
}

fn extract_ed25519_public_pem(pem: &str) -> VerifyingKey {
    use base64::engine::general_purpose::STANDARD;

    let b64: String = pem.lines().filter(|l: &&str| !l.starts_with("-----")).collect();
    let der: Vec<u8> = STANDARD.decode(b64).expect("Invalid base64 in public key PEM");

    // SubjectPublicKeyInfo last 32 bytes = raw public key
    let raw: &[u8] = &der.get(der.len() - 32..).expect("Ed25519 public der length invalid");

    VerifyingKey::from_bytes(raw.try_into().expect("Invalid Ed25519 public key length"))
        .expect("Invalid Ed25519 public key")
}
