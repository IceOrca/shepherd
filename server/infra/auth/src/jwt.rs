use jsonwebtoken::DecodingKey;

#[cfg(feature = "jwt-encode")]
use crate::account::Role;
#[cfg(feature = "jwt-encode")]
use jsonwebtoken::EncodingKey;

pub mod claims;
mod decode;
#[cfg(feature = "jwt-encode")]
mod encode;
#[cfg(feature = "jwks")]
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
/// material and legacy token lifetime policy exist only with jwt-encode.
pub struct JwtHandle {
    decoding_key: DecodingKey,
    #[cfg(feature = "jwt-encode")]
    encoding_key: EncodingKey,
    #[cfg(feature = "jwt-encode")]
    tenant_owner_expiration_secs: usize,
    #[cfg(feature = "jwt-encode")]
    supervisor_expiration_secs: usize,
    #[cfg(feature = "jwt-encode")]
    employee_expiration_secs: usize,
    #[cfg(feature = "jwks")]
    public_key_bytes: Vec<u8>,
}

impl JwtHandle {
    #[cfg(not(feature = "jwt-encode"))]
    pub fn from_public_key_path(public_pem_path: &str) -> Self {
        let public_pem: Vec<u8> = decode::read_public_key(public_pem_path);
        let decoding_key: DecodingKey = decode::parse_public_key(public_pem_path, &public_pem);

        infra_kernel::debug::log_notice!("JWT public key loaded successfully");
        Self { decoding_key }
    }

    #[cfg(feature = "jwt-encode")]
    pub fn new(private_pem_path: &str, public_pem_path: &str) -> Self {
        let public_pem: Vec<u8> = decode::read_public_key(public_pem_path);
        let decoding_key: DecodingKey = decode::parse_public_key(public_pem_path, &public_pem);
        let encoding_key: EncodingKey = encode::load_private_key(private_pem_path);

        infra_kernel::debug::log_notice!("JWT signing and validation keys loaded successfully");
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

    #[cfg(feature = "jwt-encode")]
    pub fn encoding(&self) -> &EncodingKey {
        &self.encoding_key
    }

    #[cfg(feature = "jwt-encode")]
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
