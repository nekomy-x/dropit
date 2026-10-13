//! DROP IT infra-db: доступ к PostgreSQL, репозитории, миграции.
//!
//! F0A: только каркас + подключение миграций. Репозитории появляются в F1+.

pub mod migrations;

/// Имя миграционной директории (используется sqlx::migrate!).
pub const MIGRATIONS_DIR: &str = "migrations";
