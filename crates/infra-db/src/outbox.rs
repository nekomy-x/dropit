//! Postgres-реализация transactional outbox (ADR-002).
//!
//! [`PgOutboxStore`] реализует трейт [`dropit_infra_nats::OutboxStore`] поверх
//! таблицы `outbox` (миграция `0004_requests.sql`). Поля строки —
//! `id, aggregate_id, event_type, payload, published_at, attempts`
//! (плюс служебные `aggregate_type, created_at`).
//!
//! # Стратегия захвата
//!
//! `claim_batch` выполняет `SELECT ... FOR UPDATE SKIP LOCKED` внутри короткой
//! транзакции и сразу коммитит: эксклюзивное окно длится ровно столько, сколько
//! длится сам SELECT. Долджной эксклюзивности по строкам между циклами нет —
//! строки остаются перевыбираемыми, пока не помечены
//! [`OutboxStore::mark_published`]. Это согласовано с at-least-once-семантикой
//! ADR-002: потребители идемпотентны по `(aggregate_id, event_type)`, поэтому
//! переиздание строки вторым реле безопасно. `mark_published` — точка дедупа.
//!
//! # Колонка `last_error`
//!
//! Схема `0004` не содержит `last_error` (исторически). Метод
//! [`OutboxStore::mark_failed`] по спецификации должен сохранять причину сбоя,
//! поэтому стор лениво и идемпотентно добавляет колонку через
//! `ALTER TABLE outbox ADD COLUMN IF NOT EXISTS last_error TEXT` при первом
//! обращении. Операция чисто аддитивная, безопасна при параллельных агентах и
//! миграциях: если позднее появится отдельная миграция с этой колонкой,
//! `IF NOT EXISTS` превращается в no-op. При отсутствии прав на ALTER стор
//! деградирует: `mark_failed` инкрементирует только `attempts`, причина пишется
//! в лог. Рабочего кода не падает.
//!
//! # Ограничение попыток
//!
//! `claim_batch` фильтрует `attempts < max_attempts` (по умолчанию 5), чтобы
//! «отравленные» строки не забирались в батч повторно. Порог конфигурируется
//! через [`PgOutboxStore::with_max_attempts`] и должен совпадать с
//! `max_attempts` реле.

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use dropit_infra_nats::{NatsError, OutboxRow, OutboxStore};
use sqlx::PgPool;
use uuid::Uuid;

/// Порог попыток по умолчанию: строка с `attempts >= 5` не забирается.
pub const DEFAULT_MAX_ATTEMPTS: i32 = 5;

/// Postgres-реализация хранилища outbox.
///
/// Создаётся из [`PgPool`] (см. [`dropit_infra_db::connect`]). Потокобезопасен:
/// `Arc<PgOutboxStore>` можно расшаривать между реле и воркерами.
pub struct PgOutboxStore {
    pool: PgPool,
    max_attempts: i32,
    prepared: AtomicBool,
    has_last_error: AtomicBool,
}

impl PgOutboxStore {
    /// Создать стор на заданном пуле с порогом попыток по умолчанию (5).
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self::with_max_attempts(pool, DEFAULT_MAX_ATTEMPTS)
    }

    /// Создать стор с явным порогом `max_attempts`.
    ///
    /// `max_attempts <= 0` означает, что все строки считаются исчерпавшими
    /// попытки — `claim_batch` вернёт пустой батч.
    #[must_use]
    pub fn with_max_attempts(pool: PgPool, max_attempts: i32) -> Self {
        Self {
            pool,
            max_attempts,
            prepared: AtomicBool::new(false),
            has_last_error: AtomicBool::new(false),
        }
    }

    /// Пул соединений, используемый стором.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Лениво и идемпотентно гарантировать наличие колонки `last_error`.
    ///
    /// Выполняется не более одного успешного раза на инстанс. При неудаче
    /// (нет прав) стор остаётся в деградированном режиме: `mark_failed`
    /// инкрементирует только `attempts`. Повторная попытка выполняется при
    /// следующем обращении, пока не увенчается успехом.
    async fn ensure_last_error_column(&self) {
        if self.prepared.load(Ordering::Acquire) {
            return;
        }
        match sqlx::query("ALTER TABLE outbox ADD COLUMN IF NOT EXISTS last_error TEXT")
            .execute(&self.pool)
            .await
        {
            Ok(_) => {
                self.has_last_error.store(true, Ordering::Release);
                self.prepared.store(true, Ordering::Release);
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "outbox: не удалось гарантировать колонку last_error; \
                     mark_failed будет инкрементировать только attempts"
                );
                // prepared остаётся false — повторим при следующем обращении.
            }
        }
    }
}

/// Локальная проекция строки outbox для `sqlx::FromRow`.
///
/// [`OutboxRow`] живёт в крейте `dropit-infra-nats` и не зависит от sqlx, поэтому
/// реализовать для него `FromRow` извне нельзя (orphan rule). Здесь — локальный
/// DTO с теми же полями, конвертируемый в [`OutboxRow`].
#[derive(Debug, sqlx::FromRow)]
struct OutboxDbRow {
    id: Uuid,
    aggregate_id: Uuid,
    event_type: String,
    payload: serde_json::Value,
    attempts: i32,
}

impl From<OutboxDbRow> for OutboxRow {
    fn from(r: OutboxDbRow) -> Self {
        OutboxRow {
            id: r.id,
            aggregate_id: r.aggregate_id,
            event_type: r.event_type,
            payload: r.payload,
            attempts: r.attempts,
        }
    }
}

/// Обёрнуть ошибку sqlx в [`NatsError::Store`].
fn store_err(e: sqlx::Error) -> NatsError {
    NatsError::Store(e.to_string())
}

#[async_trait]
impl OutboxStore for PgOutboxStore {
    async fn claim_batch(&self, limit: i64) -> Result<Vec<OutboxRow>, NatsError> {
        self.ensure_last_error_column().await;
        let n = limit.max(0);
        if n == 0 {
            return Ok(Vec::new());
        }
        // Короткая транзакция: FOR UPDATE SKIP LOCKED даёт disjoint-батчи
        // только в момент перекрытия транзакций двух реле; коммит сразу
        // освобождает строки. Дедуп — в mark_published (at-least-once).
        let mut tx = self.pool.begin().await.map_err(store_err)?;
        let rows: Vec<OutboxDbRow> = sqlx::query_as(
            "SELECT id, aggregate_id, event_type, payload, attempts \
             FROM outbox \
             WHERE published_at IS NULL AND attempts < $2 \
             ORDER BY created_at \
             LIMIT $1 \
             FOR UPDATE SKIP LOCKED",
        )
        .bind(n)
        .bind(self.max_attempts)
        .fetch_all(&mut *tx)
        .await
        .map_err(store_err)?;
        tx.commit().await.map_err(store_err)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn mark_published(&self, ids: &[Uuid]) -> Result<u64, NatsError> {
        self.ensure_last_error_column().await;
        let res = sqlx::query("UPDATE outbox SET published_at = now() WHERE id = ANY($1)")
            .bind(ids)
            .execute(&self.pool)
            .await
            .map_err(store_err)?;
        Ok(res.rows_affected())
    }

    async fn mark_failed(&self, id: Uuid, error: &str) -> Result<(), NatsError> {
        self.ensure_last_error_column().await;
        if self.has_last_error.load(Ordering::Acquire) {
            sqlx::query(
                "UPDATE outbox SET attempts = attempts + 1, last_error = $2 \
                 WHERE id = $1",
            )
            .bind(id)
            .bind(error)
            .execute(&self.pool)
            .await
            .map_err(store_err)?;
        } else {
            // Деградация: колонки last_error нет — инкрементируем только attempts.
            sqlx::query("UPDATE outbox SET attempts = attempts + 1 WHERE id = $1")
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(store_err)?;
            tracing::warn!(outbox_id = %id, "outbox mark_failed: last_error недоступен, причина не сохранена: {error}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! E2e-тесты outbox-стора на локальной Postgres.
    //!
    //! Запускать через лок-скрипт (общая БД, параллельные агенты):
    //! `./scripts/db-test.sh cargo test -p dropit-infra-db outbox -- --test-threads=1`.
    //!
    //! При отсутствии `DATABASE_URL` тесты завершаются раньше (не ignored —
    //! ранний возврат, как в остальной кодовой базе e2e).

    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    /// Подключение к тестовой БД или ранний возврат.
    async fn store() -> Option<PgOutboxStore> {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://dropit:dropit_local@localhost:5432/dropit".into());
        let pool = crate::connect(&url).await.ok()?;
        // Чистим outbox от прошлых прогонов.
        let _ = sqlx::query("TRUNCATE TABLE outbox").execute(&pool).await;
        Some(PgOutboxStore::with_max_attempts(pool, 3))
    }

    /// Вставить строку outbox и вернуть её id.
    async fn insert(pool: &PgPool, event_type: &str, payload: serde_json::Value) -> Uuid {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO outbox (id, aggregate_type, aggregate_id, event_type, payload) \
             VALUES ($1, 'request', $2, $3, $4)",
        )
        .bind(id)
        .bind(Uuid::now_v7())
        .bind(event_type)
        .bind(payload)
        .execute(pool)
        .await
        .unwrap();
        id
    }

    /// Счётчик невыпущенных строк.
    async fn pending(pool: &PgPool) -> i64 {
        let (c,): (i64,) = sqlx::query_as("SELECT count(*) FROM outbox WHERE published_at IS NULL")
            .fetch_one(pool)
            .await
            .unwrap();
        c
    }

    /// Текущее значение attempts строки.
    async fn attempts_of(pool: &PgPool, id: Uuid) -> i32 {
        let (a,): (i32,) = sqlx::query_as("SELECT attempts FROM outbox WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
        a
    }

    /// Опубликована ли строка.
    async fn is_published(pool: &PgPool, id: Uuid) -> bool {
        let (o,): (Option<chrono::DateTime<chrono::Utc>>,) =
            sqlx::query_as("SELECT published_at FROM outbox WHERE id = $1")
                .bind(id)
                .fetch_one(pool)
                .await
                .unwrap();
        o.is_some()
    }

    #[tokio::test]
    async fn claim_returns_unpublished_and_keeps_them() {
        let s = match store().await {
            Some(s) => s,
            None => {
                eprintln!("DATABASE_URL недоступен — outbox e2e пропущен");
                return;
            }
        };
        let pool = s.pool().clone();
        let a = insert(&pool, "request.created", json!({"n": 1})).await;
        let b = insert(&pool, "request.accepted", json!({"n": 2})).await;

        // claim возвращает обе, не удаляя.
        let claimed = s.claim_batch(10).await.expect("claim");
        assert_eq!(claimed.len(), 2);
        assert_eq!(pending(&pool).await, 2);

        // mark_published для одной.
        let marked = s.mark_published(&[a]).await.expect("mark");
        assert_eq!(marked, 1);
        assert!(is_published(&pool, a).await);
        assert!(!is_published(&pool, b).await);

        // повторный claim не возвращает опубликованную.
        let claimed2 = s.claim_batch(10).await.expect("claim2");
        assert_eq!(claimed2.len(), 1);
        assert_eq!(claimed2[0].id, b);
    }

    #[tokio::test]
    async fn mark_failed_increments_attempts_and_skips_poisoned() {
        let s = match store().await {
            Some(s) => s,
            None => {
                eprintln!("DATABASE_URL недоступен — outbox e2e пропущен");
                return;
            }
        };
        let pool = s.pool().clone();
        let id = insert(&pool, "request.played", json!({})).await;

        // Два сбоя → attempts = 2 (< max=3, ещё забирается).
        s.mark_failed(id, "boom-1").await.expect("fail1");
        assert_eq!(attempts_of(&pool, id).await, 1);
        s.mark_failed(id, "boom-2").await.expect("fail2");
        assert_eq!(attempts_of(&pool, id).await, 2);

        // Третий сбой доводит до attempts = 3 == max → больше не claimится.
        s.mark_failed(id, "boom-3").await.expect("fail3");
        assert_eq!(attempts_of(&pool, id).await, 3);
        let claimed = s.claim_batch(10).await.expect("claim after poison");
        assert!(
            claimed.is_empty(),
            "отравленная строка не должна забираться"
        );

        // last_error сохранён (колонка гарантирована ensure).
        let (err,): (Option<String>,) =
            sqlx::query_as("SELECT last_error FROM outbox WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(err.as_deref(), Some("boom-3"));
    }

    #[tokio::test]
    async fn limit_zero_returns_empty() {
        let s = match store().await {
            Some(s) => s,
            None => {
                eprintln!("DATABASE_URL недоступен — outbox e2e пропущен");
                return;
            }
        };
        let pool = s.pool().clone();
        insert(&pool, "request.created", json!({})).await;
        assert!(s.claim_batch(0).await.expect("claim0").is_empty());
        assert!(s.claim_batch(-1).await.expect("claim-1").is_empty());
        // Очередь не тронута.
        assert_eq!(pending(&pool).await, 1);
    }

    #[tokio::test]
    async fn empty_queue_claim_is_noop_and_marks_zero() {
        let s = match store().await {
            Some(s) => s,
            None => {
                eprintln!("DATABASE_URL недоступен — outbox e2e пропущен");
                return;
            }
        };
        // Пустая очередь: claim → 0, mark_published → 0.
        assert!(s.claim_batch(100).await.expect("claim empty").is_empty());
        let marked = s
            .mark_published(&[Uuid::now_v7()])
            .await
            .expect("mark empty");
        assert_eq!(marked, 0);
    }
}
