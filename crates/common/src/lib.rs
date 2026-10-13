//! DROP IT common: типы, деньги, ошибки, идентификаторы.
//!
//! Правила (ADR-003): все суммы — целые в минорных единицах валюты (центы).
//! Никогда float.

pub mod auth;
pub mod error;
pub mod money;

pub use auth::{AccessClaims, Role};
pub use error::{ApiError, ErrorEnvelope};
pub use money::{MinorUnits, Money};
