//! Модель строки outbox, трейт хранилища и in-memory реализация.
//!
//! Реализует ADR-002 (transactional outbox): реле читает невыпущенные строки
//! таблицы `outbox` (поля `id, aggregate_id, event_type, payload, published_at,
//! attempts`), публикует их в NATS JetStream и помечает успешные. Таблица
//! физически живёт в `infra-db` (миграция `0004_requests.sql`); этот крейт
//! знает о ней только через абстракцию [`OutboxStore`].

use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::NatsError;

/// Строка outbox, захваченная реле для публикации.
///
/// Соответствует проекции таблицы `outbox` без служебных полей
/// (`aggregate_type`, `created_at`, `published_at`) — реле они не нужны.
/// Десериализуется из JSON-строки БД/фикстуры.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxRow {
    /// Идентификатор события (UUIDv7).
    pub id: Uuid,
    /// Идентификатор агрегата, к которому относится событие.
    pub aggregate_id: Uuid,
    /// Тип события, напр. `request.created`, `payment.captured`.
    /// Используется для построения subject публикации через [`subject_for`].
    pub event_type: String,
    /// Тело события (JSON). Публикуется байт-в-байт как сериализованный JSON.
    pub payload: Value,
    /// Число предыдущих неудачных попыток публикации.
    /// Реле пропускает строки с `attempts >= max_attempts`.
    pub attempts: i32,
}

/// Хранилище outbox-событий.
///
/// Продакшен-реализация (Postgres + `FOR UPDATE SKIP LOCKED`) живёт вне этого
/// крейта (воркеры); здесь — контракт и in-memory реализация для тестов/локала.
/// Все методы асинхронны и `Send + Sync`, чтобы реле могло хранить
/// `Arc<dyn OutboxStore>`.
#[async_trait]
pub trait OutboxStore: Send + Sync {
    /// Захватить батч невыпущенных событий.
    ///
    /// `limit <= 0` трактуется как пустой батч (0 строк). Строки остаются
    /// доступными для повторного захвата, пока не будут помечены
    /// [`OutboxStore::mark_published`]; ответственность за эксклюзивность
    /// (SKIP LOCKED) лежит на БД-реализации.
    async fn claim_batch(&self, limit: i64) -> Result<Vec<OutboxRow>, NatsError>;

    /// Пометить строки как успешно опубликованные (`published_at = now()`).
    ///
    /// Возвращает число реально обновлённых строк. Вызывается только для
    /// успешно опубликованных в текущем цикле.
    async fn mark_published(&self, ids: &[Uuid]) -> Result<u64, NatsError>;

    /// Пометить строку как неудачную: `attempts++` и запомнить причину.
    ///
    /// Строка остаётся невыпущенной и будет повторно захвачена следующим
    /// циклом, пока `attempts` не достигнет `max_attempts`.
    async fn mark_failed(&self, id: Uuid, error: &str) -> Result<(), NatsError>;
}

/// Внутрипроцессное хранилище outbox для тестов и локальной разработки.
///
/// Не реализует эксклюзивной блокировки строк (SKIP LOCKED) и пригодно только
/// для одного реле в одном процессе. FIFO по порядку добавления через
/// [`InMemoryOutboxStore::push`].
#[derive(Debug, Default)]
pub struct InMemoryOutboxStore {
    rows: Mutex<Vec<MemRow>>,
}

#[derive(Debug, Clone)]
struct MemRow {
    row: OutboxRow,
    published: bool,
    last_error: Option<String>,
}

impl InMemoryOutboxStore {
    /// Создать пустое хранилище.
    pub fn new() -> Self {
        Self::default()
    }

    /// Добавить строку в хранилище (для посева в тестах/локале).
    pub fn push(&self, row: OutboxRow) {
        let mut rows = self.rows.lock().expect("outbox mutex poisoned");
        rows.push(MemRow {
            row,
            published: false,
            last_error: None,
        });
    }

    /// Число ещё не опубликованных строк.
    pub fn pending_count(&self) -> usize {
        let rows = self.rows.lock().expect("outbox mutex poisoned");
        rows.iter().filter(|r| !r.published).count()
    }

    /// Число опубликованных строк.
    pub fn published_count(&self) -> usize {
        let rows = self.rows.lock().expect("outbox mutex poisoned");
        rows.iter().filter(|r| r.published).count()
    }

    /// Опубликована ли строка с данным `id`.
    pub fn is_published(&self, id: Uuid) -> bool {
        let rows = self.rows.lock().expect("outbox mutex poisoned");
        rows.iter().any(|r| r.row.id == id && r.published)
    }

    /// Текущее число попыток для строки (или `None`, если строки нет).
    pub fn attempts(&self, id: Uuid) -> Option<i32> {
        let rows = self.rows.lock().expect("outbox mutex poisoned");
        rows.iter().find(|r| r.row.id == id).map(|r| r.row.attempts)
    }

    /// Последняя записанная причина сбоя для строки (или `None`).
    pub fn last_error(&self, id: Uuid) -> Option<String> {
        let rows = self.rows.lock().expect("outbox mutex poisoned");
        rows.iter()
            .find(|r| r.row.id == id)
            .and_then(|r| r.last_error.clone())
    }
}

#[async_trait]
impl OutboxStore for InMemoryOutboxStore {
    async fn claim_batch(&self, limit: i64) -> Result<Vec<OutboxRow>, NatsError> {
        let n = usize::try_from(limit).unwrap_or(0);
        if n == 0 {
            return Ok(Vec::new());
        }
        let rows = self.rows.lock().expect("outbox mutex poisoned");
        Ok(rows
            .iter()
            .filter(|r| !r.published)
            .take(n)
            .map(|r| r.row.clone())
            .collect())
    }

    async fn mark_published(&self, ids: &[Uuid]) -> Result<u64, NatsError> {
        let mut rows = self.rows.lock().expect("outbox mutex poisoned");
        let mut changed = 0u64;
        for r in rows.iter_mut() {
            if ids.contains(&r.row.id) && !r.published {
                r.published = true;
                changed += 1;
            }
        }
        Ok(changed)
    }

    async fn mark_failed(&self, id: Uuid, error: &str) -> Result<(), NatsError> {
        let mut rows = self.rows.lock().expect("outbox mutex poisoned");
        let r = rows
            .iter_mut()
            .find(|r| r.row.id == id)
            .ok_or_else(|| NatsError::Store(format!("outbox row {id} not found")))?;
        r.row.attempts += 1;
        r.last_error = Some(error.to_string());
        Ok(())
    }
}

/// Построить subject публикации для типа события.
///
/// Формат: `dropit.outbox.{event_type}`, напр. `request.created` →
/// `dropit.outbox.request.created`. Стрим NATS должен ловить шаблон
/// `dropit.outbox.>`.
#[must_use]
pub fn subject_for(event_type: &str) -> String {
    format!("dropit.outbox.{event_type}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn subject_format() {
        assert_eq!(
            subject_for("request.created"),
            "dropit.outbox.request.created"
        );
        assert_eq!(
            subject_for("payment.captured"),
            "dropit.outbox.payment.captured"
        );
        assert_eq!(subject_for(""), "dropit.outbox.");
    }

    #[test]
    fn parse_row_from_fixture() {
        let fixture = r#"{
            "id": "01920000-0000-7fff-8000-000000000001",
            "aggregate_id": "01920000-0000-7fff-8000-000000000002",
            "event_type": "request.created",
            "payload": {"track": "abc", "n": 42, "ok": true},
            "attempts": 0
        }"#;
        let row: OutboxRow = serde_json::from_str(fixture).expect("parse fixture");
        assert_eq!(
            row.id,
            Uuid::parse_str("01920000-0000-7fff-8000-000000000001").unwrap()
        );
        assert_eq!(
            row.aggregate_id,
            Uuid::parse_str("01920000-0000-7fff-8000-000000000002").unwrap()
        );
        assert_eq!(row.event_type, "request.created");
        assert_eq!(row.payload, json!({"track": "abc", "n": 42, "ok": true}));
        assert_eq!(row.attempts, 0);
    }

    #[tokio::test]
    async fn store_claim_and_marks() {
        let s = InMemoryOutboxStore::new();
        let a = row(1, "a", json!({}), 0);
        let b = row(2, "b", json!({}), 0);
        s.push(a.clone());
        s.push(b.clone());

        // claim возвращает невыпущенные, не удаляя
        let claimed = s.claim_batch(10).await.expect("claim");
        assert_eq!(claimed.len(), 2);
        assert_eq!(s.pending_count(), 2);

        // mark_published для одного
        let marked = s.mark_published(&[a.id]).await.expect("mark");
        assert_eq!(marked, 1);
        assert!(s.is_published(a.id));
        assert!(!s.is_published(b.id));
        assert_eq!(s.pending_count(), 1);

        // повторный claim не возвращает опубликованное
        let claimed2 = s.claim_batch(10).await.expect("claim2");
        assert_eq!(claimed2.len(), 1);
        assert_eq!(claimed2[0].id, b.id);

        // mark_failed инкрементирует attempts
        s.mark_failed(b.id, "boom").await.expect("fail");
        assert_eq!(s.attempts(b.id), Some(1));
        assert_eq!(s.last_error(b.id), Some("boom".to_string()));

        // limit <= 0 → пусто
        assert!(s.claim_batch(0).await.expect("claim0").is_empty());
        assert!(s.claim_batch(-1).await.expect("claim-1").is_empty());

        // mark_failed несуществующего → ошибка
        let missing = Uuid::from_u128(999);
        assert!(s.mark_failed(missing, "x").await.is_err());
    }

    fn row(n: u128, et: &str, payload: Value, attempts: i32) -> OutboxRow {
        OutboxRow {
            id: Uuid::from_u128(n),
            aggregate_id: Uuid::from_u128(n + 1000),
            event_type: et.to_string(),
            payload,
            attempts,
        }
    }
}
