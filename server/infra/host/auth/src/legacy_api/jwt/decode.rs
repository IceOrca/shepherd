use jsonwebtoken::DecodingKey;

pub(super) fn read_public_key(public_pem_path: &str) -> Vec<u8> {
    std::fs::read(public_pem_path)
        .unwrap_or_else(|error| panic!("Cannot read JWT public key from {public_pem_path}: {error}"))
}

pub(super) fn parse_public_key(public_pem_path: &str, public_pem: &[u8]) -> DecodingKey {
    DecodingKey::from_ed_pem(public_pem)
        .unwrap_or_else(|error| panic!("Invalid JWT public key in {public_pem_path}: {error}"))
}
