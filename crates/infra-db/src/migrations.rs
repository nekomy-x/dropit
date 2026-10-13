//! Запуск миграций (Migrator).
//!
//! Вызывается из бинарников при `DROPIT_RUN_MIGRATIONS=true` (local-режим).
//! В production миграции выполняются отдельным шагом деплоя (expand/contract).

use sqlx::migrate::Migrator;

/// Возвращает Migrator, читая миграции из директории крейта.
/// Вызов async: `migrator().await`.
pub async fn migrator() -> Migrator {
    Migrator::new(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations"
    )))
    .await
    .expect("migrations directory exists at build time")
}

/// Применяет все неприменённые миграции (для CI и readiness).
pub async fn run(pool: &sqlx::PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    migrator().await.run(pool).await
}
