//! Шифрование секретов (F8): AES-256-GCM для TOTP-секретов.
//!
//! Прод-режим: ключ 32 байта из `DROPIT_TOTP_KEY` (hex; секретное
//! хранилище/Vault — задача деплоя). Формат хранения:
//! `v2:<nonce-hex>:<ciphertext-hex>`.
//!
//! Совместимость: старый формат (plain base32, F1-тесты) читается
//! как есть и перешивается при следующей записи — миграция без
//! downtime. Отсутствие ключа → ошибка конфигурации (не тихий plain!).

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};

#[derive(Debug, thiserror::Error)]
pub enum SecretCryptoError {
    #[error("crypto backend error")]
    Backend,
    #[error("missing DROPIT_TOTP_KEY (hex, 32 bytes)")]
    MissingKey,
    #[error("malformed encrypted secret")]
    Malformed,
    #[error("malformed key")]
    BadKey,
}

const V2_PREFIX: &str = "v2:";

/// Шифрует base32-секрет AES-256-GCM. Ключ — hex 32 байта из env.
pub fn encrypt_totp_secret(plain_base32: &str) -> Result<Vec<u8>, SecretCryptoError> {
    let key = load_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| SecretCryptoError::BadKey)?;
    let nonce_bytes = rand::random::<[u8; 12]>();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plain_base32.as_bytes())
        .map_err(|_| SecretCryptoError::Backend)?;
    // v2:nonce:ciphertext (hex)
    Ok(format!(
        "{V2_PREFIX}{}:{}",
        hex::encode(nonce_bytes),
        hex::encode(ct)
    )
    .into_bytes())
}

/// Расшифровывает секрет. Формат v2 — AES-GCM; любой другой —
/// легаси-plain base32 (возвращаем как есть: читать можно,
/// запись перешьёт).
pub fn decrypt_totp_secret(enc: &[u8]) -> Result<String, SecretCryptoError> {
    let text = std::str::from_utf8(enc).map_err(|_| SecretCryptoError::Malformed)?;
    let Some(rest) = text.strip_prefix(V2_PREFIX) else {
        // Легаси-plain: base32 без префикса.
        return Ok(text.to_string());
    };
    let mut parts = rest.split(':');
    let (Some(nonce_hex), Some(ct_hex), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(SecretCryptoError::Malformed);
    };
    let key = load_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| SecretCryptoError::BadKey)?;
    let nonce = hex::decode(nonce_hex).map_err(|_| SecretCryptoError::Malformed)?;
    let ct = hex::decode(ct_hex).map_err(|_| SecretCryptoError::Malformed)?;
    if nonce.len() != 12 {
        return Err(SecretCryptoError::Malformed);
    }
    let plain = cipher
        .decrypt(Nonce::from_slice(&nonce), ct.as_ref())
        .map_err(|_| SecretCryptoError::Backend)?;
    String::from_utf8(plain).map_err(|_| SecretCryptoError::Malformed)
}

/// Ключ из env (hex 32 байта). В тестах задают переменную явно.
fn load_key() -> Result<[u8; 32], SecretCryptoError> {
    let hex_key = std::env::var("DROPIT_TOTP_KEY").map_err(|_| SecretCryptoError::MissingKey)?;
    let bytes = hex::decode(hex_key.trim()).map_err(|_| SecretCryptoError::BadKey)?;
    bytes.try_into().map_err(|_| SecretCryptoError::BadKey)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тесты меняют env — сериализуем (параллельные потоки cargo test).
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_test_key(f: impl FnOnce()) {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // Детерминированный тестовый ключ (НЕ прод!).
        let prev = std::env::var("DROPIT_TOTP_KEY").ok();
        std::env::set_var("DROPIT_TOTP_KEY", "00".repeat(32));
        f();
        match prev {
            Some(v) => std::env::set_var("DROPIT_TOTP_KEY", v),
            None => std::env::remove_var("DROPIT_TOTP_KEY"),
        }
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        with_test_key(|| {
            let secret = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP";
            let enc = encrypt_totp_secret(secret).unwrap();
            // Формат v2.
            let text = String::from_utf8(enc.clone()).unwrap();
            assert!(text.starts_with("v2:"));
            // Расшифровка.
            let dec = decrypt_totp_secret(&enc).unwrap();
            assert_eq!(dec, secret);
        });
    }

    #[test]
    fn nonce_unique_per_encryption() {
        with_test_key(|| {
            let a = encrypt_totp_secret("X").unwrap();
            let b = encrypt_totp_secret("X").unwrap();
            assert_ne!(a, b, "случайный nonce → разные шифротексты");
            // Но оба расшифровываются.
            assert_eq!(decrypt_totp_secret(&a).unwrap(), "X");
            assert_eq!(decrypt_totp_secret(&b).unwrap(), "X");
        });
    }

    #[test]
    fn legacy_plain_still_readable() {
        with_test_key(|| {
            // Легаси-формат F1 (plain base32): читается без ошибки.
            let legacy = b"JBSWY3DPEHPK3PXP";
            assert_eq!(decrypt_totp_secret(legacy).unwrap(), "JBSWY3DPEHPK3PXP");
        });
    }

    #[test]
    fn missing_key_is_error_not_plain() {
        let prev = std::env::var("DROPIT_TOTP_KEY").ok();
        std::env::remove_var("DROPIT_TOTP_KEY");
        let err = encrypt_totp_secret("X").unwrap_err();
        assert!(matches!(err, SecretCryptoError::MissingKey));
        if let Some(v) = prev {
            std::env::set_var("DROPIT_TOTP_KEY", v);
        }
    }

    #[test]
    fn tampered_ciphertext_rejected() {
        with_test_key(|| {
            let enc = encrypt_totp_secret("JBSWY3DPEHPK3PXP").unwrap();
            // Портируем последний байт.
            let mut broken = enc.clone();
            let last = broken.len() - 1;
            broken[last] ^= 0xFF;
            assert!(
                decrypt_totp_secret(&broken).is_err(),
                "GCM-тег ловит подмену"
            );
        });
    }
}
