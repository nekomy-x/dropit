//! Библиотечный фасад gateway-api (для seed-бинарей).

/// Пароль Argon2id (как auth.rs).
pub fn hash_password(password: &str) -> Result<String, String> {
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
    use argon2::Argon2;
    let salt = SaltString::generate(&mut OsRng);
    let argon = Argon2::default();
    argon
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

/// TOTP-секрет → v2-шифрование (если задан DROPIT_TOTP_KEY; иначе —
/// plain base32, читается легаси-путём — Test Mode).
pub fn encrypt_totp_secret_for_seed(base32: &str) -> Result<Vec<u8>, String> {
    match dropit_common::encrypt_totp_secret(base32) {
        Ok(enc) => Ok(enc),
        Err(_) => Ok(base32.as_bytes().to_vec()),
    }
}
