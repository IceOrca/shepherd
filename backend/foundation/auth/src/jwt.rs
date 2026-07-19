use core::panic;
use std::env;

use super::dto::AccessClaims;
use axum::{
    extract::{Extension, Request},
    http::{StatusCode, header},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{Algorithm, TokenData, Validation, decode};
use jsonwebtoken::{DecodingKey, EncodingKey};

use foundation_kernel::debug::*;
use crate::account::Role;

#[macro_export]
macro_rules! KID_MAIN {
    () => {
        "foundation-key-1"
    };
}

pub use crate::KID_MAIN;

pub struct JwtHandle {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    tenant_owner_expiration_secs: usize,
    supervisor_expiration_secs: usize,
    employee_expiration_secs: usize,
    public_key_bytes: Vec<u8>, // raw bytes for JWKS
}

impl JwtHandle {
    pub fn new(private_pem_path: &str, public_pem_path: &str) -> Self {
        let private_pem: Vec<u8> = match std::fs::read(private_pem_path) {
            Ok(data) => data,
            Err(err) => panic!("Cannot read JWT private key from {}: {}", private_pem_path, err),
        };

        let public_pem: Vec<u8> = match std::fs::read(public_pem_path) {
            Ok(data) => data,
            Err(err) => panic!("Cannot read JWT public key from {}: {}", public_pem_path, err),
        };

        let private_key: EncodingKey = match EncodingKey::from_ed_pem(&private_pem) {
            Ok(key) => key,
            Err(err) => panic!("Invalid JWT private key in {}: {}", private_pem_path, err),
        };

        let public_key: DecodingKey = match DecodingKey::from_ed_pem(&public_pem) {
            Ok(key) => key,
            Err(err) => panic!("Invalid JWT public key in {}: {}", public_pem_path, err),
        };

        let public_key_bytes: Vec<u8> = extract_ed25519_public_key_bytes(&public_pem);

        log_notice!("JWT keys loaded successfully");
        // Never format key objects: their Debug representation is not part of
        // the security contract and may expose key material in future versions.
        log_debug!(
            "JWT key pair parsed: kid={} algorithm=EdDSA public_key_length={}",
            KID_MAIN!(),
            public_key_bytes.len()
        );

        Self {
            encoding_key: private_key,
            decoding_key: public_key,
            tenant_owner_expiration_secs: read_expiration_secs("TENANT_OWNER_JWT_EXPIRATION_SECS", 900),
            supervisor_expiration_secs: read_expiration_secs("SUPERVISOR_JWT_EXPIRATION_SECS", 900),
            employee_expiration_secs: read_expiration_secs("EMPLOYEE_JWT_EXPIRATION_SECS", 900),
            public_key_bytes,
        }
    }

    pub fn encoding(&self) -> &EncodingKey {
        &self.encoding_key
    }

    pub fn decoding(&self) -> &DecodingKey {
        &self.decoding_key
    }

    pub fn expiration_for_role(&self, role: &Role) -> usize {
        if matches!(role, Role::TenantOwner) {
            self.tenant_owner_expiration_secs
        } else if matches!(role, Role::Supervisor) {
            self.supervisor_expiration_secs
        } else {
            self.employee_expiration_secs
        }
    }

    pub fn key_public(&self) -> &Vec<u8> {
        &self.public_key_bytes
    }
} /* impl JwtHandle */

fn read_expiration_secs(env_name: &str, default_value: usize) -> usize {
    let parsed_value: usize = match env::var(env_name) {
        Ok(value) => value.parse().unwrap_or_else(|err: std::num::ParseIntError| {
            log_warn!("Invalid {} format: {}, using default {}s", env_name, err, default_value);
            default_value
        }),
        Err(_) => {
            log_warn!("{} not set, using default {}s", env_name, default_value);
            default_value
        }
    };
    if !(60..=86_400).contains(&parsed_value) {
        log_warn!(
            "{} must be between 60 and 86400 seconds, using default {}s",
            env_name,
            default_value
        );
        default_value
    } else {
        parsed_value
    }
}

/// Extract raw 32 bytes from Ed25519 public key PEM
fn extract_ed25519_public_key_bytes(pem: &[u8]) -> Vec<u8> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let pem_str: &str = std::str::from_utf8(pem).unwrap_or_else(|err: std::str::Utf8Error| {
        panic!("Invalid UTF-8 in public key PEM: {}", err);
    });
    let b64_content: String = pem_str
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect::<Vec<_>>()
        .join("");

    // Decode base64 → DER bytes
    let der: Vec<u8> = STANDARD.decode(b64_content).unwrap_or_else(|err: base64::DecodeError| {
        panic!("Failed to decode public key PEM: {}", err);
    });

    // Ed25519 public key DER structure (SubjectPublicKeyInfo):
    // 30 2a          SEQUENCE
    //   30 05        SEQUENCE
    //     06 03      OID
    //       2b 65 70 (Ed25519 OID = 1.3.101.112)
    //   03 21        BIT STRING (33 bytes)
    //     00         padding bit
    //     <32 bytes> ← raw public key
    // Raw key = last 32 bytes
    if der.len() < 32 {
        panic!("Invalid DER length for Ed25519 public key: {}", der.len());
    }

    let result: Vec<u8> = der
        .get(der.len() - 32..)
        .unwrap_or_else(|| panic!("Failed to extract raw public key bytes"))
        .to_vec();
    log_debug!("Extracted Ed25519 public key for JWKS: byte_length={}", result.len());
    result
}
