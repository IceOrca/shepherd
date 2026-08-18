use std::env;

use infra_kernel::debug::warn;
use jsonwebtoken::EncodingKey;

pub(super) fn load_private_key(private_pem_path: &str) -> EncodingKey {
    let private_pem: Vec<u8> = std::fs::read(private_pem_path)
        .unwrap_or_else(|error| panic!("Cannot read JWT private key from {private_pem_path}: {error}"));
    EncodingKey::from_ed_pem(&private_pem)
        .unwrap_or_else(|error| panic!("Invalid JWT private key in {private_pem_path}: {error}"))
}

pub(super) fn read_expiration_secs(env_name: &str, default_value: usize) -> usize {
    let parsed_value: usize = match env::var(env_name) {
        Ok(value) => value.parse().unwrap_or_else(|error: std::num::ParseIntError| {
            warn!(
                "Invalid {} format: {}, using default {}s",
                env_name, error, default_value
            );
            default_value
        }),
        Err(_) => default_value,
    };
    if !(60..=86_400).contains(&parsed_value) {
        warn!(
            "{} must be between 60 and 86400 seconds, using default {}s",
            env_name, default_value
        );
        default_value
    } else {
        parsed_value
    }
}
