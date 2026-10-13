//! DROP IT infra-db: доступ к PostgreSQL, репозитории, миграции.
//!
//! F0A: каркас + миграции. F1: идентичность (identity).
//! F2: сессии, QR, join-гранты (sessions).

pub mod billing;
pub mod identity;
pub mod jobs;
pub mod migrations;
pub mod payouts;
pub mod requests;
pub mod sessions;

pub use billing::{BillingError, BillingRepo, CommissionParams, PaymentRow, SplitSnapshot};
pub use identity::{
    connect, GuestDevice, IdentityError, IdentityRepo, PasswordSubject, RefreshTokenRow,
};
pub use jobs::{ClosedSession, JobsError, JobsRepo};
pub use payouts::{Payee, PayoutError, PayoutRepo, PayoutRow};
pub use requests::{QueueItem, RequestError, RequestRepo, RequestRow};
pub use sessions::{
    QrTokenRow, SessionError, SessionGuestRow, SessionInfo, SessionRepo, SessionRow,
};

/// Имя миграционной директории (используется sqlx::migrate!).
pub const MIGRATIONS_DIR: &str = "migrations";
