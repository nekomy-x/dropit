//! DROP IT common: типы, деньги, ошибки, идентификаторы.
//!
//! Правила (ADR-003): все суммы — целые в минорных единицах валюты (центы).
//! Никогда float.

pub mod api_key;
pub mod auth;
pub mod crypto;
pub mod error;
pub mod money;

pub use api_key::{
    generate_api_key, hash_api_key, ip_allowed, normalize_scopes, parse_api_key, scopes_allow,
    scopes_to_strings, secret_eq, validate_ip_rules, Environment, GeneratedApiKey, ParsedApiKey,
    Scope, ScopeError, ScopeInfo, ScopePreset, DEFAULT_RATE_LIMIT_PER_MIN,
    DEFAULT_ROTATION_GRACE_HOURS, MAX_ACTIVE_KEYS_PER_CLUB, MAX_RATE_LIMIT_PER_MIN,
    MAX_ROTATION_GRACE_HOURS, MIN_RATE_LIMIT_PER_MIN, SCOPE_CATALOGUE, SCOPE_PRESETS,
    USAGE_RETENTION_DAYS,
};
pub use auth::{AccessClaims, Role};
pub use crypto::{decrypt_totp_secret, encrypt_totp_secret, SecretCryptoError};
pub use error::{ApiError, ErrorEnvelope};
pub use money::{MinorUnits, Money};
