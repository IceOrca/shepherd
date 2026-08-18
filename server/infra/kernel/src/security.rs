use argon2::{
    Argon2, password_hash,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};

use tracing::{error, warn, info, debug, trace};

pub fn hash_passphrase(passphrase: &str) -> Result<String, String> {
    let salt: SaltString = SaltString::generate(&mut OsRng);
    let argon2: Argon2<'_> = Argon2::default(); // default is Argon2id

    argon2
        .hash_password(passphrase.as_bytes(), &salt)
        .map_err(|err: password_hash::Error| {
            error!("Failed to hash passphrase: {}", err);
            "Error hashing passphrase".to_string()
        })
        .map(|hash: PasswordHash<'_>| hash.to_string())
}

pub fn verify_passphrase(passphrase: &str, passkey: &str) -> bool {
    let parsed_hash: PasswordHash<'_> = match PasswordHash::new(passkey) {
        Ok(hash) => hash,
        Err(err) => {
            error!("Failed to parse passphrase hash: {}", err);
            return false;
        }
    };

    match Argon2::default().verify_password(passphrase.as_bytes(), &parsed_hash) {
        Ok(_) => true,
        Err(_) => {
            info!("passphrase is unmatch");
            false
        }
    }
}
