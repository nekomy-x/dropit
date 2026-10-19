//! Диспетчер push-уведомлений: читает outbox-события гостевых статусов,
//! отправляет push через [`PushSender`] и отзывает невалидные токены.
//!
//! # Источник сообщений
//!
//! Сообщения — события outbox (ADR-002) типов, которые относятся к гостю:
//! `request.accepted` («заявка принята»), `request.played` («трек играет»),
//! `request.rejected` («отклонена»). Эти события публикуются шлюзом в
//! `requests.rs` с полем `guest_device_id` в `payload`. Событие
//! `request.created` адресовано DJ-очереди (без гостя) — диспетчером
//! не обрабатывается.
//!
//! # Идемпотентность
//!
//! Диспетчер захватывает батч через [`OutboxStore::claim_batch`], фильтрует
//! push-типы, отправляет уведомления и помечает обработанные события через
//! [`OutboxStore::mark_published`]. Повторный прогон не переотправляет
//! опубликованное (стор их исключает). Невалидные токены помечаются
//! `revoked_at = now()` и исключаются из следующих отправок.
//!
//! # Сетевая безопасность в тестах
//!
//! В тестах подставляется [`NoopPush`] или фейковый sender и фейковый
//! [`PushTokenSource`]: никаких сетевых вызовов и обязательной БД.

use async_trait::async_trait;
use dropit_infra_db::jobs::PushTokenRow;
use dropit_infra_nats::{OutboxRow, OutboxStore};
use dropit_infra_push::{Platform, PushMessage, PushSender, PushToken};
use serde_json::Value;
use uuid::Uuid;

#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::Mutex;

use crate::jobs::JobReport;

/// Типы outbox-событий, адресованные гостю (источник push-сообщений).
pub const PUSH_EVENT_TYPES: &[&str] = &["request.accepted", "request.played", "request.rejected"];

/// Источник push-токенов гостей для диспетчера.
///
/// Продакшен-реализация — [`DbTokenSource`] поверх [`JobsRepo`]. В тестах
/// подставляется [`FakeTokenSource`].
#[async_trait]
pub trait PushTokenSource: Send + Sync {
    /// Активные (не отозванные) токены гостя.
    async fn active_tokens(&self, guest_id: Uuid) -> Result<Vec<PushTokenRow>, TokenError>;
    /// Отозвать невалидный токен. Идемпотентно.
    async fn revoke_token(&self, token: &str) -> Result<bool, TokenError>;
}

/// Ошибка источника токенов.
#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    /// Ошибка БД.
    #[error("token source db error: {0}")]
    Db(String),
}

/// Продакшен-источник токенов поверх [`JobsRepo`].
pub struct DbTokenSource {
    jobs: dropit_infra_db::JobsRepo,
}

impl DbTokenSource {
    /// Создать источник из репозитория.
    #[must_use]
    pub fn new(jobs: dropit_infra_db::JobsRepo) -> Self {
        Self { jobs }
    }
}

#[async_trait]
impl PushTokenSource for DbTokenSource {
    async fn active_tokens(&self, guest_id: Uuid) -> Result<Vec<PushTokenRow>, TokenError> {
        self.jobs
            .active_push_tokens(guest_id)
            .await
            .map_err(|e| TokenError::Db(e.to_string()))
    }

    async fn revoke_token(&self, token: &str) -> Result<bool, TokenError> {
        self.jobs
            .revoke_push_token(token)
            .await
            .map_err(|e| TokenError::Db(e.to_string()))
    }
}

/// Один прогон диспетчера push.
///
/// Возвращает [`JobReport`] со счётчиками: `claimed` (всего захвачено),
/// `push_events` (из них push-типов), `sent` (успешно отправлено событий),
/// `failed` (событий с ошибкой отправки), `no_tokens` (событий без токенов),
/// `invalid_tokens_revoked`, `marked_published`.
///
/// Не падает на ошибке одного события: логирует и оставляет его
/// непубликованным для повторной попытки следующим прогоном.
pub async fn run_push(
    store: &dyn OutboxStore,
    sender: &dyn PushSender,
    tokens: &dyn PushTokenSource,
    batch_size: i64,
) -> JobReport {
    let rows = match store.claim_batch(batch_size).await {
        Ok(r) => r,
        Err(e) => return JobReport::error("push", format!("claim: {e}")),
    };
    let claimed = rows.len();

    let mut push_events = 0usize;
    let mut sent = 0usize;
    let mut failed = 0usize;
    let mut no_tokens = 0usize;
    let mut invalid_revoked = 0usize;
    let mut to_publish: Vec<Uuid> = Vec::new();

    for row in rows {
        if !PUSH_EVENT_TYPES.contains(&row.event_type.as_str()) {
            // Не push-тип: оставляем реле outbox (не помечаем).
            continue;
        }
        push_events += 1;

        let Some(guest_id) = guest_device_id_of(&row) else {
            // Нет гостя в payload — помечаем опубликованным (не ретраим).
            tracing::warn!(
                event_type = %row.event_type,
                outbox_id = %row.id,
                "push: событие без guest_device_id, помечаем опубликованным"
            );
            to_publish.push(row.id);
            no_tokens += 1;
            continue;
        };

        let token_rows = match tokens.active_tokens(guest_id).await {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, outbox_id = %row.id, "push: не прочитать токены, оставляем событие");
                failed += 1;
                continue;
            }
        };

        if token_rows.is_empty() {
            // Нет активных токенов — уведомить некому, помечаем опубликованным.
            to_publish.push(row.id);
            no_tokens += 1;
            continue;
        }

        let push_tokens: Vec<PushToken> = token_rows
            .iter()
            .filter_map(|t| {
                platform_from_str(&t.platform).map(|p| PushToken {
                    token: t.token.clone(),
                    platform: p,
                })
            })
            .collect();

        if push_tokens.is_empty() {
            to_publish.push(row.id);
            no_tokens += 1;
            continue;
        }

        let message = build_message(&row);
        match sender.send(&push_tokens, message).await {
            Ok(report) => {
                // Отзываем невалидные токены.
                for tok in &report.invalid_tokens {
                    match tokens.revoke_token(tok).await {
                        Ok(true) => invalid_revoked += 1,
                        Ok(false) => {}
                        Err(e) => tracing::warn!(error = %e, token = ?tok.is_empty(), "push: не отозвать токен"),
                    }
                }
                sent += 1;
                to_publish.push(row.id);
            }
            Err(e) => {
                // Транспортная ошибка (401/503): не помечаем, ретрай следующим прогоном.
                tracing::warn!(error = %e, outbox_id = %row.id, "push: отправка не удалась, ретрай");
                failed += 1;
            }
        }
    }

    let marked = if to_publish.is_empty() {
        0
    } else {
        match store.mark_published(&to_publish).await {
            Ok(n) => n as usize,
            Err(e) => {
                tracing::warn!(error = %e, "push: mark_published не удался");
                0
            }
        }
    };

    JobReport::ok(
        "push",
        serde_json::json!({
            "claimed": claimed,
            "push_events": push_events,
            "sent": sent,
            "failed": failed,
            "no_tokens": no_tokens,
            "invalid_tokens_revoked": invalid_revoked,
            "marked_published": marked,
        }),
    )
}

/// Извлечь `guest_device_id` из payload события.
fn guest_device_id_of(row: &OutboxRow) -> Option<Uuid> {
    extract_uuid(&row.payload, "guest_device_id")
}

/// Извлечь UUID-поле из JSON-объекта.
fn extract_uuid(payload: &Value, key: &str) -> Option<Uuid> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
}

/// Построить push-сообщение по типу события.
fn build_message(row: &OutboxRow) -> PushMessage {
    let request_id = extract_uuid(&row.payload, "request_id");
    let (title, body) = match row.event_type.as_str() {
        "request.accepted" => (
            "Заявка принята".to_string(),
            "Ваш трек в очереди.".to_string(),
        ),
        "request.played" => (
            "Трек играет".to_string(),
            "Поставили ваш трек.".to_string(),
        ),
        "request.rejected" => {
            let reason = row
                .payload
                .get("reject_reason")
                .and_then(Value::as_str)
                .unwrap_or("Трек не принят.");
            ("Заявка отклонена".to_string(), reason.to_string())
        }
        other => (other.to_string(), String::new()),
    };
    let mut data: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    if let Some(rid) = request_id {
        data.insert("request_id".to_string(), rid.to_string());
    }
    data.insert("event_type".to_string(), row.event_type.clone());
    PushMessage {
        title,
        body,
        data,
        collapse_key: Some(row.event_type.clone()),
        route: request_id.map(|r| format!("request/{r}")),
    }
}

/// Сопоставить строку платформы из БД с [`Platform`].
fn platform_from_str(s: &str) -> Option<Platform> {
    match s.to_ascii_lowercase().as_str() {
        "android" => Some(Platform::Android),
        "ios" => Some(Platform::Ios),
        other => {
            tracing::warn!(platform = other, "push: неизвестная платформа, токен пропущен");
            None
        }
    }
}

/// Фейковый источник токенов для тестов: in-memory карта + журнал отзывов.
#[cfg(test)]
#[derive(Default)]
pub struct FakeTokenSource {
    tokens: Mutex<HashMap<Uuid, Vec<PushTokenRow>>>,
    revoked: Mutex<Vec<String>>,
}

#[cfg(test)]
impl FakeTokenSource {
    /// Создать пустой источник.
    pub fn new() -> Self {
        Self::default()
    }

    /// Засеять токены гостя.
    pub fn with_token(mut self, guest_id: Uuid, token: &str, platform: &str) -> Self {
        self.tokens
            .get_mut()
            .unwrap()
            .entry(guest_id)
            .or_default()
            .push(PushTokenRow {
                token: token.to_string(),
                platform: platform.to_string(),
            });
        self
    }

    /// Список отозванных токенов (порядок вызова).
    pub fn revoked(&self) -> Vec<String> {
        self.revoked.lock().unwrap().clone()
    }
}

#[cfg(test)]
#[async_trait]
impl PushTokenSource for FakeTokenSource {
    async fn active_tokens(&self, guest_id: Uuid) -> Result<Vec<PushTokenRow>, TokenError> {
        Ok(self.tokens.lock().unwrap().get(&guest_id).cloned().unwrap_or_default())
    }

    async fn revoke_token(&self, token: &str) -> Result<bool, TokenError> {
        let mut revoked = self.revoked.lock().unwrap();
        if revoked.iter().any(|t| t == token) {
            return Ok(false);
        }
        revoked.push(token.to_string());
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dropit_infra_nats::InMemoryOutboxStore;
    use dropit_infra_push::{NoopPush, PushError, PushOutcome, PushReport};
    use serde_json::json;

    fn row(n: u128, event_type: &str, payload: Value) -> OutboxRow {
        OutboxRow {
            id: Uuid::from_u128(n),
            aggregate_id: Uuid::from_u128(n + 100),
            event_type: event_type.to_string(),
            payload,
            attempts: 0,
        }
    }

    #[tokio::test]
    async fn push_sends_and_marks_published() {
        let store = InMemoryOutboxStore::new();
        let guest = Uuid::from_u128(555);
        store.push(row(
            1,
            "request.accepted",
            json!({ "guest_device_id": guest, "request_id": Uuid::from_u128(9) }),
        ));
        let tokens = FakeTokenSource::new().with_token(guest, "tok-aaaaaaaaaaaaaaa", "android");

        let report = run_push(&store, &NoopPush, &tokens, 10).await;
        assert_eq!(report.status, "ok");
        let d = &report.detail;
        assert_eq!(d["push_events"], 1);
        assert_eq!(d["sent"], 1);
        assert_eq!(d["marked_published"], 1);
        assert!(store.is_published(Uuid::from_u128(1)));
    }

    #[tokio::test]
    async fn push_no_tokens_marks_published_no_retry() {
        let store = InMemoryOutboxStore::new();
        let guest = Uuid::from_u128(777);
        store.push(row(2, "request.played", json!({ "guest_device_id": guest })));
        let tokens = FakeTokenSource::new();

        let report = run_push(&store, &NoopPush, &tokens, 10).await;
        assert_eq!(report.detail["no_tokens"], 1);
        assert_eq!(report.detail["sent"], 0);
        assert_eq!(report.detail["marked_published"], 1);
        assert!(store.is_published(Uuid::from_u128(2)));
    }

    #[tokio::test]
    async fn push_idempotent_second_run_is_noop() {
        let store = InMemoryOutboxStore::new();
        let guest = Uuid::from_u128(555);
        store.push(row(1, "request.accepted", json!({ "guest_device_id": guest })));
        let tokens = FakeTokenSource::new().with_token(guest, "tok-bbbbbbbbbbbbbbb", "ios");

        let first = run_push(&store, &NoopPush, &tokens, 10).await;
        assert_eq!(first.detail["sent"], 1);
        let calls_before = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        // Второй прогон: опубликованного нет → ничего не отправляется.
        let second = run_push(&store, &NoopPush, &tokens, 10).await;
        assert_eq!(second.detail["push_events"], 0);
        assert_eq!(second.detail["sent"], 0);
        let _ = calls_before;
    }

    #[tokio::test]
    async fn push_non_push_events_left_unmarked() {
        let store = InMemoryOutboxStore::new();
        // request.created — не push-тип (адресован DJ-очереди).
        store.push(row(3, "request.created", json!({ "session_id": Uuid::nil() })));
        let tokens = FakeTokenSource::new();

        let report = run_push(&store, &NoopPush, &tokens, 10).await;
        assert_eq!(report.detail["push_events"], 0);
        assert_eq!(report.detail["marked_published"], 0);
        // Событие осталось непубликованным — для реле outbox.
        assert!(!store.is_published(Uuid::from_u128(3)));
        assert_eq!(store.pending_count(), 1);
    }

    /// Фейковый sender, который признаёт заданный токен невалидным.
    struct InvalidatingSender {
        invalid: String,
    }

    #[async_trait]
    impl PushSender for InvalidatingSender {
        async fn send(
            &self,
            tokens: &[PushToken],
            _message: PushMessage,
        ) -> Result<PushReport, PushError> {
            let mut report = PushReport::default();
            for t in tokens {
                if t.token == self.invalid {
                    report.add(PushOutcome::invalid(&t.token, "Unregistered"));
                } else {
                    report.add(PushOutcome::success(&t.token));
                }
            }
            Ok(report)
        }
    }

    #[tokio::test]
    async fn push_revokes_invalid_tokens() {
        let store = InMemoryOutboxStore::new();
        let guest = Uuid::from_u128(555);
        store.push(row(1, "request.accepted", json!({ "guest_device_id": guest })));
        let tokens = FakeTokenSource::new()
            .with_token(guest, "tok-good-aaaaaaaaaaa", "android")
            .with_token(guest, "tok-bad-bbbbbbbbbbbb", "ios");

        let sender = InvalidatingSender {
            invalid: "tok-bad-bbbbbbbbbbbb".to_string(),
        };
        let report = run_push(&store, &sender, &tokens, 10).await;
        assert_eq!(report.detail["sent"], 1);
        assert_eq!(report.detail["invalid_tokens_revoked"], 1);
        assert!(tokens.revoked().contains(&"tok-bad-bbbbbbbbbbbb".to_string()));
    }

    /// Sender, имитирующий транспортную ошибку.
    struct FailingSender;

    #[async_trait]
    impl PushSender for FailingSender {
        async fn send(
            &self,
            _tokens: &[PushToken],
            _message: PushMessage,
        ) -> Result<PushReport, PushError> {
            Err(PushError::Unavailable("simulated".to_string()))
        }
    }

    #[tokio::test]
    async fn push_transport_failure_leaves_event_unpublished() {
        let store = InMemoryOutboxStore::new();
        let guest = Uuid::from_u128(555);
        store.push(row(1, "request.accepted", json!({ "guest_device_id": guest })));
        let tokens = FakeTokenSource::new().with_token(guest, "tok-ccccccccccccccc", "android");

        let report = run_push(&store, &FailingSender, &tokens, 10).await;
        assert_eq!(report.detail["failed"], 1);
        assert_eq!(report.detail["marked_published"], 0);
        assert!(!store.is_published(Uuid::from_u128(1)), "событие должно остаться для ретрая");
    }

    #[tokio::test]
    async fn build_message_rejected_uses_reason() {
        let r = row(
            1,
            "request.rejected",
            json!({ "reject_reason": "уже играет" }),
        );
        let m = build_message(&r);
        assert_eq!(m.title, "Заявка отклонена");
        assert_eq!(m.body, "уже играет");
    }

    #[tokio::test]
    async fn build_message_played() {
        let r = row(1, "request.played", json!({}));
        let m = build_message(&r);
        assert_eq!(m.title, "Трек играет");
    }

    #[test]
    fn platform_mapping() {
        assert!(matches!(platform_from_str("android"), Some(Platform::Android)));
        assert!(matches!(platform_from_str("IOS"), Some(Platform::Ios)));
        assert!(platform_from_str("windows").is_none());
    }

    #[tokio::test]
    async fn empty_batch_is_noop() {
        let store = InMemoryOutboxStore::new();
        let tokens = FakeTokenSource::new();
        let report = run_push(&store, &NoopPush, &tokens, 10).await;
        assert_eq!(report.detail["claimed"], 0);
        assert_eq!(report.detail["marked_published"], 0);
    }
}
