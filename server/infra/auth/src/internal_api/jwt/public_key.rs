use base64::{Engine, engine::general_purpose::STANDARD};

pub(super) fn extract_ed25519_public_key_bytes(pem: &[u8]) -> Vec<u8> {
    let pem_str: &str =
        std::str::from_utf8(pem).unwrap_or_else(|error| panic!("Invalid UTF-8 in public key PEM: {error}"));
    let encoded: String = pem_str
        .lines()
        .filter(|line: &&str| !line.starts_with("-----"))
        .collect::<Vec<_>>()
        .join("");
    let der: Vec<u8> = STANDARD
        .decode(encoded)
        .unwrap_or_else(|error| panic!("Failed to decode public key PEM: {error}"));
    let raw_key = der
        .get(der.len().saturating_sub(32)..)
        .filter(|key: &&[u8]| key.len() == 32)
        .unwrap_or_else(|| panic!("Invalid DER length for Ed25519 public key: {}", der.len()));
    raw_key.to_vec()
}
