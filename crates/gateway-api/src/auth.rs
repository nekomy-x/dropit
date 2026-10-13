//! Аутентификация F1: пароли, JWT, refresh-ротация, TOTP.
//!
//! Правила (техкарта Часть I §3, Часть VII):
//!   * пароли — Argon2id;
//!   * access JWT 15 мин (stateless), refresh 30 дней (stateful, ротация
//!     с семьями: reuse токена = кража → ревок всей семьи);
//!   * 2FA — TOTP (RFC 6238, SHA-1, 6 цифр, период 30 с);
//!   * логин — rate-limit 5/15 мин (в БД, Redis-счётчики — F8).

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{Duration, Utc};
use dropit_common::{AccessClaims, Role};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Параметры JWT/refresh (конфиг-значения, но безопасные дефолты).
pub const ACCESS_TTL_SECS: i64 = 15 * 60;
pub const REFRESH_TTL_DAYS: i64 = 30;

/// Ошибки аутентификации. Часть вариантов вводится в использование
/// построчно (TOTP-флоу в F8-hardening); все нужны контракту ошибок.
#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("2FA required")]
    TotpRequired,
    #[error("invalid TOTP code")]
    InvalidTotp,
    #[error("token expired")]
    TokenExpired,
    #[error("token invalid")]
    TokenInvalid,
    #[error("refresh token reused")]
    RefreshReused,
    #[error("hashing error: {0}")]
    Hashing(String),
}

impl From<argon2::password_hash::Error> for AuthError {
    fn from(e: argon2::password_hash::Error) -> Self {
        AuthError::Hashing(e.to_string())
    }
}

// ============================================================
// Пароли (Argon2id)
// ============================================================

/// Хеширует пароль Argon2id (случайная соль, параметры по умолчанию OWASP).
/// Используется seed-утилитами и тестами; в runtime только verify.
#[allow(dead_code)]
pub fn hash_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon = Argon2::default();
    Ok(argon.hash_password(password.as_bytes(), &salt)?.to_string())
}

/// Проверяет пароль против Argon2id-хеша. Ошибка = неверный пароль
/// (детали хеширования наружу не отдаются).
pub fn verify_password(password: &str, hash: &str) -> Result<bool, AuthError> {
    let parsed = PasswordHash::new(hash)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

// ============================================================
// JWT access
// ============================================================

/// Ключи подписи. В проде — из секретного хранилища (Vault/SOPS),
/// в local — из переменных окружения; тестовый дефолт только для CI.
#[derive(Clone)]
pub struct JwtKeys {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl JwtKeys {
    pub fn from_secret(secret: &[u8]) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
        }
    }

    /// Выпускает access-JWT с `role`/`sub`/`fam`-клеймами.
    pub fn issue_access(&self, sub: Uuid, role: Role, fam: Uuid) -> Result<String, AuthError> {
        let now = Utc::now().timestamp();
        let claims = AccessClaims {
            sub,
            role,
            fam,
            iat: now,
            exp: now + ACCESS_TTL_SECS,
        };
        encode(&Header::default(), &claims, &self.encoding).map_err(|_| AuthError::TokenInvalid)
    }

    /// Проверяет подпись/срок и возвращает клеймы.
    pub fn verify_access(&self, token: &str) -> Result<AccessClaims, AuthError> {
        let data =
            decode::<AccessClaims>(token, &self.decoding, &Validation::default()).map_err(|e| {
                match e.kind() {
                    jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::TokenExpired,
                    _ => AuthError::TokenInvalid,
                }
            })?;
        Ok(data.claims)
    }
}

// ============================================================
// Refresh-токены (stateful, ротация)
// ============================================================

/// Новый refresh-токен (128 бит энтропии). В БД — только sha256.
pub fn generate_refresh_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Хеш для хранения (sha256, hex).
pub fn hash_refresh_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Плановый срок действия refresh-токена.
pub fn refresh_expiry() -> chrono::DateTime<Utc> {
    Utc::now() + Duration::days(REFRESH_TTL_DAYS)
}

// ============================================================
// TOTP (2FA)
// ============================================================

/// Проверяет 6-значный TOTP-код с окном ±1 шаг (skew=1, период 30 с) —
/// компенсирует рассинхрон часов телефона (NTP-drift — отдельный лог, см. VII).
pub fn verify_totp(secret: &totp_rs::TOTP, code: &str) -> bool {
    secret.check_current(code).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_roundtrip() {
        let hash = hash_password("s3cret-pa55").unwrap();
        assert!(verify_password("s3cret-pa55", &hash).unwrap());
        assert!(!verify_password("wrong", &hash).unwrap());
    }

    #[test]
    fn password_hash_is_salted() {
        // Одинаковые пароли → разные хеши (случайная соль).
        let a = hash_password("same").unwrap();
        let b = hash_password("same").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn jwt_issue_and_verify_roundtrip() {
        let keys = JwtKeys::from_secret(b"test-secret-key-32-bytes-xxxxxxxx");
        let sub = Uuid::now_v7();
        let fam = Uuid::now_v7();
        let token = keys.issue_access(sub, Role::Dj, fam).unwrap();
        let claims = keys.verify_access(&token).unwrap();
        assert_eq!(claims.sub, sub);
        assert_eq!(claims.role, Role::Dj);
        assert_eq!(claims.fam, fam);
    }

    #[test]
    fn jwt_rejects_foreign_secret() {
        let a = JwtKeys::from_secret(b"secret-aaaaaaaaaaaaaaaaaaaaaaaa");
        let b = JwtKeys::from_secret(b"secret-bbbbbbbbbbbbbbbbbbbbbbbb");
        let token = a
            .issue_access(Uuid::now_v7(), Role::Admin, Uuid::now_v7())
            .unwrap();
        assert!(matches!(
            b.verify_access(&token),
            Err(AuthError::TokenInvalid)
        ));
    }

    #[test]
    fn jwt_expired_token_rejected() {
        // exp в прошлом → ExpiredSignature.
        let keys = JwtKeys::from_secret(b"test-secret-key-32-bytes-xxxxxxxx");
        let now = Utc::now().timestamp();
        let claims = AccessClaims {
            sub: Uuid::now_v7(),
            role: Role::Club,
            fam: Uuid::now_v7(),
            iat: now - 3600,
            exp: now - 600, // больше дефолтного leeway 60 с
        };
        let token = encode(&Header::default(), &claims, &keys.encoding).unwrap();
        let err = keys.verify_access(&token).expect_err("must fail");
        assert!(
            matches!(err, AuthError::TokenExpired),
            "expected TokenExpired, got: {err:?}"
        );
    }

    #[test]
    fn refresh_token_hashed_not_stored_plain() {
        let t = generate_refresh_token();
        assert_eq!(t.len(), 64); // 32 байта hex
        let h = hash_refresh_token(&t);
        assert_ne!(t, h);
        assert_eq!(hash_refresh_token(&t), h); // детерминизм
    }

    #[test]
    fn totp_accepts_valid_code_with_drift() {
        use totp_rs::{Algorithm, Secret, TOTP};
        let secret =
            Secret::Raw("DROP IT test secret 32 bytes!!!!".as_bytes().to_vec()).to_encoded();
        // skew = 1: окно ±1 шаг, поглощает небольшой drift часов.
        let totp = TOTP::new_unchecked(Algorithm::SHA1, 6, 1, 30, secret.to_bytes().unwrap());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        // Код «сейчас» и код «шаг назад» должны проходить (skew-окно).
        let code = totp.generate(now - 30);
        assert!(verify_totp(&totp, &code));
        let code_now = totp.generate(now);
        assert!(verify_totp(&totp, &code_now));
        assert!(!verify_totp(&totp, "000000"));
    }
}
