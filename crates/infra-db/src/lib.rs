//! DROP IT infra-db: доступ к PostgreSQL, репозитории, миграции.
//!
//! F0A: каркас + миграции. F1: репозиторий идентичности (identity).

pub mod identity;
pub mod migrations;

pub use identity::{
    connect, GuestDevice, IdentityError, IdentityRepo, PasswordSubject, RefreshTokenRow,
};

/// Имя миграционной директории (используется sqlx::migrate!).
pub const MIGRATIONS_DIR: &str = "migrations";
