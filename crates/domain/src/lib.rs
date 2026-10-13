//! DROP IT domain: чистые домены — сессии, запросы, состояния.
//!
//! Домены не знают про HTTP и БД (ADR-001): только типы, правила переходов
//! и чистые функции. Persistence живёт в infra-крейтах (будущие фазы).

pub mod catalog;
pub mod payment;
pub mod qr;
pub mod request;
pub mod session;

pub use catalog::{
    canonical_key, validate_track, CatalogError, CatalogTrack, Source, TrackCounter, ValidatedTrack,
};
pub use payment::{compute_split, PaymentAction, PaymentStatus, Split, SplitError};
pub use request::{RequestAction, RequestStatus};
pub use session::{SessionStatus, SessionTransition};
