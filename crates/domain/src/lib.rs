//! DROP IT domain: чистые домены — сессии, запросы, состояния.
//!
//! Домены не знают про HTTP и БД (ADR-001): только типы, правила переходов
//! и чистые функции. Persistence живёт в infra-крейтах (будущие фазы).

pub mod request;
pub mod session;

pub use request::{RequestAction, RequestStatus};
pub use session::{SessionStatus, SessionTransition};
