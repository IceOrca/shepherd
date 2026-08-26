use std::collections::HashMap;

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, AeadCore, Generate, KeyInit, Payload},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use uuid::Uuid;

const NONCE_LENGTH: usize = 12;
const AES_256_KEY_LENGTH: usize = 32;
const MIN_LOOKUP_KEY_LENGTH: usize = 32;

#[derive(Clone)]
pub struct CitizenIdProtector {
    active_key_id: String,
    encryption_keys: HashMap<String, [u8; AES_256_KEY_LENGTH]>,
    lookup_key: Vec<u8>,
}

pub struct ProtectedCitizenId {
    pub key_id: String,
    pub ciphertext: Vec<u8>,
    pub lookup_hmac: Vec<u8>,
    pub last4: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CitizenIdProtectionError {
    #[error("HR_CITIZEN_ID_ACTIVE_KEY_ID is required")]
    MissingActiveKeyId,
    #[error("HR_CITIZEN_ID_ENCRYPTION_KEYS_JSON is required")]
    MissingEncryptionKeys,
    #[error("HR_CITIZEN_ID_LOOKUP_KEY_BASE64 is required")]
    MissingLookupKey,
    #[error("HR_CITIZEN_ID_ENCRYPTION_KEYS_JSON must be a JSON object of base64 AES-256 keys")]
    InvalidEncryptionKeySet(#[source] serde_json::Error),
    #[error(
        "citizen-ID encryption key identifiers must contain 1-32 ASCII letters, digits, dots, underscores, or hyphens"
    )]
    InvalidKeyId,
    #[error("citizen-ID encryption keys must be valid base64 values containing exactly 32 bytes")]
    InvalidEncryptionKey,
    #[error("HR_CITIZEN_ID_ACTIVE_KEY_ID does not identify a configured encryption key")]
    ActiveKeyMissing,
    #[error("HR_CITIZEN_ID_LOOKUP_KEY_BASE64 must be valid base64 containing at least 32 bytes")]
    InvalidLookupKey,
    #[error("citizen identity encryption failed")]
    Encrypt,
    #[error("citizen identity ciphertext is invalid")]
    InvalidCiphertext,
    #[error("citizen identity uses an unavailable encryption key")]
    UnknownKey,
    #[error("citizen identity decryption failed")]
    Decrypt,
    #[error("decrypted citizen identity is not valid UTF-8")]
    InvalidPlaintext(#[source] std::string::FromUtf8Error),
}

#[derive(Deserialize)]
#[serde(transparent)]
struct EncodedKeySet(HashMap<String, String>);

impl CitizenIdProtector {
    pub fn from_env() -> Result<Self, CitizenIdProtectionError> {
        let active_key_id: String =
            required_env("HR_CITIZEN_ID_ACTIVE_KEY_ID").ok_or(CitizenIdProtectionError::MissingActiveKeyId)?;
        let encoded_keys: String = required_env("HR_CITIZEN_ID_ENCRYPTION_KEYS_JSON")
            .ok_or(CitizenIdProtectionError::MissingEncryptionKeys)?;
        let lookup_key: String =
            required_env("HR_CITIZEN_ID_LOOKUP_KEY_BASE64").ok_or(CitizenIdProtectionError::MissingLookupKey)?;
        Self::new(&active_key_id, &encoded_keys, &lookup_key)
    }

    fn new(
        active_key_id: &str,
        encoded_keys_json: &str,
        encoded_lookup_key: &str,
    ) -> Result<Self, CitizenIdProtectionError> {
        if !valid_key_id(active_key_id) {
            return Err(CitizenIdProtectionError::InvalidKeyId);
        }
        let EncodedKeySet(encoded_keys): EncodedKeySet =
            serde_json::from_str(encoded_keys_json).map_err(CitizenIdProtectionError::InvalidEncryptionKeySet)?;
        let mut encryption_keys: HashMap<String, [u8; AES_256_KEY_LENGTH]> = HashMap::with_capacity(encoded_keys.len());
        for (key_id, encoded_key) in encoded_keys {
            if !valid_key_id(&key_id) {
                return Err(CitizenIdProtectionError::InvalidKeyId);
            }
            let decoded_key: Vec<u8> = STANDARD
                .decode(encoded_key)
                .map_err(|_error| CitizenIdProtectionError::InvalidEncryptionKey)?;
            let key: [u8; AES_256_KEY_LENGTH] = decoded_key
                .try_into()
                .map_err(|_value: Vec<u8>| CitizenIdProtectionError::InvalidEncryptionKey)?;
            encryption_keys.insert(key_id, key);
        }
        if !encryption_keys.contains_key(active_key_id) {
            return Err(CitizenIdProtectionError::ActiveKeyMissing);
        }
        let lookup_key: Vec<u8> = STANDARD
            .decode(encoded_lookup_key)
            .map_err(|_error| CitizenIdProtectionError::InvalidLookupKey)?;
        if lookup_key.len() < MIN_LOOKUP_KEY_LENGTH {
            return Err(CitizenIdProtectionError::InvalidLookupKey);
        }
        Ok(Self {
            active_key_id: active_key_id.to_owned(),
            encryption_keys,
            lookup_key,
        })
    }

    pub fn protect(
        &self,
        tenant_id: Uuid,
        country_code: &str,
        citizen_id: &str,
    ) -> Result<ProtectedCitizenId, CitizenIdProtectionError> {
        let encryption_key: &[u8; AES_256_KEY_LENGTH] = self
            .encryption_keys
            .get(&self.active_key_id)
            .ok_or(CitizenIdProtectionError::ActiveKeyMissing)?;
        let cipher: Aes256Gcm = Aes256Gcm::new_from_slice(encryption_key)
            .map_err(|_error| CitizenIdProtectionError::InvalidEncryptionKey)?;
        let nonce: Nonce<<Aes256Gcm as AeadCore>::NonceSize> = Nonce::generate();
        let associated_data: Vec<u8> = associated_data(tenant_id, country_code);
        let encrypted: Vec<u8> = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: citizen_id.as_bytes(),
                    aad: &associated_data,
                },
            )
            .map_err(|_error| CitizenIdProtectionError::Encrypt)?;
        let mut ciphertext: Vec<u8> = Vec::with_capacity(NONCE_LENGTH + encrypted.len());
        ciphertext.extend_from_slice(&nonce);
        ciphertext.extend_from_slice(&encrypted);

        let mut lookup_mac: Hmac<Sha256> = <Hmac<Sha256> as hmac::digest::KeyInit>::new_from_slice(&self.lookup_key)
            .map_err(|_error| CitizenIdProtectionError::InvalidLookupKey)?;
        lookup_mac.update(&associated_data);
        lookup_mac.update(&[0]);
        lookup_mac.update(citizen_id.as_bytes());
        let lookup_hmac: Vec<u8> = lookup_mac.finalize().into_bytes().to_vec();
        let last4: String = citizen_id
            .chars()
            .rev()
            .take(4)
            .collect::<String>()
            .chars()
            .rev()
            .collect();

        Ok(ProtectedCitizenId {
            key_id: self.active_key_id.clone(),
            ciphertext,
            lookup_hmac,
            last4,
        })
    }

    pub fn reveal(
        &self,
        tenant_id: Uuid,
        country_code: &str,
        key_id: &str,
        ciphertext: &[u8],
    ) -> Result<String, CitizenIdProtectionError> {
        let (nonce_bytes, encrypted): (&[u8], &[u8]) = ciphertext
            .split_at_checked(NONCE_LENGTH)
            .ok_or(CitizenIdProtectionError::InvalidCiphertext)?;
        let nonce: Nonce<<Aes256Gcm as AeadCore>::NonceSize> = nonce_bytes
            .try_into()
            .map_err(|_error| CitizenIdProtectionError::InvalidCiphertext)?;
        let encryption_key: &[u8; AES_256_KEY_LENGTH] = self
            .encryption_keys
            .get(key_id)
            .ok_or(CitizenIdProtectionError::UnknownKey)?;
        let cipher: Aes256Gcm = Aes256Gcm::new_from_slice(encryption_key)
            .map_err(|_error| CitizenIdProtectionError::InvalidEncryptionKey)?;
        let plaintext: Vec<u8> = cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: encrypted,
                    aad: &associated_data(tenant_id, country_code),
                },
            )
            .map_err(|_error| CitizenIdProtectionError::Decrypt)?;
        String::from_utf8(plaintext).map_err(CitizenIdProtectionError::InvalidPlaintext)
    }
}

fn associated_data(tenant_id: Uuid, country_code: &str) -> Vec<u8> {
    format!("shepherd:hr-citizen-id:v1:{tenant_id}:{country_code}").into_bytes()
}

fn valid_key_id(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value
            .bytes()
            .all(|byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn required_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value: String| value.trim().to_owned())
        .filter(|value: &String| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use uuid::Uuid;

    use super::CitizenIdProtector;

    fn protector() -> CitizenIdProtector {
        let encryption_key: String = STANDARD.encode([7_u8; 32]);
        let lookup_key: String = STANDARD.encode([9_u8; 32]);
        CitizenIdProtector::new("v1", &format!(r#"{{"v1":"{encryption_key}"}}"#), &lookup_key)
            .unwrap_or_else(|error| panic!("test protector must be valid: {error}"))
    }

    #[test]
    fn encrypts_and_decrypts_with_tenant_bound_authenticated_data() {
        let protector: CitizenIdProtector = protector();
        let tenant_id: Uuid = Uuid::new_v4();
        let protected = protector
            .protect(tenant_id, "VN", "012345678901")
            .unwrap_or_else(|error| panic!("test citizen ID must encrypt: {error}"));
        assert_ne!(protected.ciphertext, b"012345678901");
        assert_eq!(protected.last4, "8901");
        assert_eq!(protected.lookup_hmac.len(), 32);
        assert_eq!(
            protector
                .reveal(tenant_id, "VN", &protected.key_id, &protected.ciphertext)
                .unwrap_or_else(|error| panic!("test citizen ID must decrypt: {error}")),
            "012345678901"
        );
        assert!(
            protector
                .reveal(Uuid::new_v4(), "VN", &protected.key_id, &protected.ciphertext)
                .is_err()
        );
    }

    #[test]
    fn lookup_hash_is_stable_inside_one_tenant_and_distinct_across_tenants() {
        let protector: CitizenIdProtector = protector();
        let tenant_id: Uuid = Uuid::new_v4();
        let first = protector
            .protect(tenant_id, "VN", "012345678901")
            .unwrap_or_else(|error| panic!("first test citizen ID must encrypt: {error}"));
        let second = protector
            .protect(tenant_id, "VN", "012345678901")
            .unwrap_or_else(|error| panic!("second test citizen ID must encrypt: {error}"));
        let another_tenant = protector
            .protect(Uuid::new_v4(), "VN", "012345678901")
            .unwrap_or_else(|error| panic!("cross-tenant test citizen ID must encrypt: {error}"));
        assert_eq!(first.lookup_hmac, second.lookup_hmac);
        assert_ne!(first.ciphertext, second.ciphertext);
        assert_ne!(first.lookup_hmac, another_tenant.lookup_hmac);
    }
}
