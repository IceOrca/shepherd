use jsonwebtoken::DecodingKey;

use crate::account::Role;
use jsonwebtoken::EncodingKey;

#[path = "jwt/claims.rs"]
pub mod claims;
#[path = "jwt/decode.rs"]
mod decode;
#[path = "jwt/encode.rs"]
mod encode;
#[cfg(feature = "jwks")]
#[path = "jwt/public_key.rs"]
mod public_key;

#[macro_export]
macro_rules! KID_MAIN {
    () => {
        "infra-key-1"
    };
}

pub use crate::KID_MAIN;

/// JWT material enabled by the selected Cargo capabilities.
///
/// The default build loads only a public decoding key. Private signing
/// material and legacy token lifetime policy exist only with jwt.
pub struct JwtHandle {
    decoding_key: DecodingKey,
    encoding_key: EncodingKey,
    tenant_owner_expiration_secs: usize,
    supervisor_expiration_secs: usize,
    employee_expiration_secs: usize,
    public_key_bytes: Vec<u8>,
}

impl JwtHandle {
    #[cfg(not(feature = "jwt"))]
    pub fn from_public_key_path(public_pem_path: &str) -> Self {
        let public_pem: Vec<u8> = decode::read_public_key(public_pem_path);
        let decoding_key: DecodingKey = decode::parse_public_key(public_pem_path, &public_pem);

        infra_kernel::debug::info!("JWT public key loaded successfully");
        Self { decoding_key }
    }

    pub fn new(private_pem_path: &str, public_pem_path: &str) -> Self {
        let public_pem: Vec<u8> = decode::read_public_key(public_pem_path);
        let decoding_key: DecodingKey = decode::parse_public_key(public_pem_path, &public_pem);
        let encoding_key: EncodingKey = encode::load_private_key(private_pem_path);

        infra_kernel::debug::info!("JWT signing and validation keys loaded successfully");
        Self {
            decoding_key,
            encoding_key,
            tenant_owner_expiration_secs: encode::read_expiration_secs("TENANT_OWNER_JWT_EXPIRATION_SECS", 900),
            supervisor_expiration_secs: encode::read_expiration_secs("SUPERVISOR_JWT_EXPIRATION_SECS", 900),
            employee_expiration_secs: encode::read_expiration_secs("EMPLOYEE_JWT_EXPIRATION_SECS", 900),
            #[cfg(feature = "jwks")]
            public_key_bytes: public_key::extract_ed25519_public_key_bytes(&public_pem),
        }
    }

    pub fn decoding(&self) -> &DecodingKey {
        &self.decoding_key
    }

    pub fn encoding(&self) -> &EncodingKey {
        &self.encoding_key
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

    #[cfg(feature = "jwks")]
    pub fn key_public(&self) -> &[u8] {
        &self.public_key_bytes
    }
}
