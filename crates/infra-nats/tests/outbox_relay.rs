//! Интеграционные тесты реле outbox на in-memory сторе и фейковой шине.
//! Без сети: проверяют построение subject, payload байт-в-байт и цикл релия.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};

use dropit_infra_nats::{
    InMemoryOutboxStore, MessageBus, NatsError, OutboxRelay, OutboxRow, RelayReport,
};
use uuid::Uuid;

/// Фейковая шина: записывает все вызовы и падает на заданном (1-базируемом)
/// номере вызова publish, если задан `fail_at`.
#[derive(Default)]
struct FakeBus {
    calls: Mutex<Vec<(String, Vec<u8>)>>,
    next: AtomicUsize,
    fail_at: Option<usize>,
}

impl FakeBus {
    fn ok() -> Self {
        Self::default()
    }
    fn failing(fail_at: usize) -> Self {
        Self {
            fail_at: Some(fail_at),
            ..Self::default()
        }
    }
}

#[async_trait]
impl MessageBus for FakeBus {
    async fn publish(&self, subject: &str, payload: &[u8]) -> Result<(), NatsError> {
        let idx = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        self.calls
            .lock()
            .expect("bus mutex")
            .push((subject.to_string(), payload.to_vec()));
        if let Some(f) = self.fail_at {
            if idx == f {
                return Err(NatsError::Transport("fake failure".to_string()));
            }
        }
        Ok(())
    }
}

fn make_row(n: u128, event_type: &str, payload: Value, attempts: i32) -> OutboxRow {
    OutboxRow {
        id: Uuid::from_u128(n),
        aggregate_id: Uuid::from_u128(n + 1000),
        event_type: event_type.to_string(),
        payload,
        attempts,
    }
}

async fn run(relay: &OutboxRelay<InMemoryOutboxStore>) -> RelayReport {
    relay.run_once().await.expect("run_once")
}

#[tokio::test]
async fn successful_release() {
    let store = Arc::new(InMemoryOutboxStore::new());
    store.push(make_row(1, "request.created", json!({"a": 1}), 0));
    store.push(make_row(2, "request.accepted", json!({"b": 2}), 0));
    store.push(make_row(3, "request.played", json!({"c": 3}), 0));

    let bus = Arc::new(FakeBus::ok());
    let relay = OutboxRelay::new(store.clone(), bus.clone(), 10, 5);

    let report = run(&relay).await;
    assert_eq!(
        report,
        RelayReport {
            claimed: 3,
            published: 3,
            failed: 0,
            skipped: 0
        }
    );
    assert_eq!(store.published_count(), 3);
    assert_eq!(store.pending_count(), 0);
    assert_eq!(bus.calls.lock().expect("c").len(), 3);
}

#[tokio::test]
async fn repeat_release_publishes_nothing_new() {
    let store = Arc::new(InMemoryOutboxStore::new());
    store.push(make_row(1, "request.created", json!({}), 0));
    let bus = Arc::new(FakeBus::ok());
    let relay = OutboxRelay::new(store.clone(), bus.clone(), 10, 5);

    let first = run(&relay).await;
    assert_eq!(first.published, 1);
    let calls_after_first = bus.calls.lock().expect("c").len();

    // Повторный цикл: опубликованного больше нет → пустой батч, к шине не лезем.
    let second = run(&relay).await;
    assert_eq!(second, RelayReport::default());
    assert_eq!(
        bus.calls.lock().expect("c").len(),
        calls_after_first,
        "шине не должны были обращаться"
    );
}

#[tokio::test]
async fn partial_failure_isolates_row_and_keeps_rest() {
    let store = Arc::new(InMemoryOutboxStore::new());
    store.push(make_row(1, "request.created", json!({"i": 1}), 0));
    store.push(make_row(2, "request.accepted", json!({"i": 2}), 0));
    store.push(make_row(3, "request.played", json!({"i": 3}), 0));

    // Второй вызов publish падает.
    let bus = Arc::new(FakeBus::failing(2));
    let relay = OutboxRelay::new(store.clone(), bus, 10, 5);

    let report = run(&relay).await;
    assert_eq!(
        report,
        RelayReport {
            claimed: 3,
            published: 2,
            failed: 1,
            skipped: 0
        }
    );
    // Успешные (1 и 3) помечены, упавшая (2) — нет, attempts инкрементирован.
    assert!(store.is_published(Uuid::from_u128(1)));
    assert!(store.is_published(Uuid::from_u128(3)));
    assert!(!store.is_published(Uuid::from_u128(2)));
    assert_eq!(store.attempts(Uuid::from_u128(2)), Some(1));
    assert_eq!(
        store.last_error(Uuid::from_u128(2)).as_deref(),
        Some("nats transport error: fake failure")
    );
    assert_eq!(store.pending_count(), 1);
}

#[tokio::test]
async fn failed_row_is_retried_next_cycle() {
    let store = Arc::new(InMemoryOutboxStore::new());
    store.push(make_row(1, "request.created", json!({}), 0));
    store.push(make_row(2, "request.accepted", json!({}), 0));

    // Первый цикл: второй publish падает.
    let bus_fail = Arc::new(FakeBus::failing(2));
    let relay = OutboxRelay::new(store.clone(), bus_fail, 10, 5);
    let first = run(&relay).await;
    assert_eq!(first.published, 1);
    assert_eq!(first.failed, 1);

    // Второй цикл со здоровой шиной: упавшая строка переиздаётся.
    let bus_ok = Arc::new(FakeBus::ok());
    let relay2 = OutboxRelay::new(store.clone(), bus_ok, 10, 5);
    let second = run(&relay2).await;
    assert_eq!(second.claimed, 1);
    assert_eq!(second.published, 1);
    assert_eq!(store.published_count(), 2);
    assert!(store.is_published(Uuid::from_u128(2)));
}

#[tokio::test]
async fn exhausted_attempts_are_skipped() {
    let store = Arc::new(InMemoryOutboxStore::new());
    // attempts >= max_attempts (5 >= 3) → skip, к шине не обращаемся.
    store.push(make_row(1, "request.created", json!({}), 5));
    let bus = Arc::new(FakeBus::ok());
    let relay = OutboxRelay::new(store.clone(), bus.clone(), 10, 3);

    let report = run(&relay).await;
    assert_eq!(
        report,
        RelayReport {
            claimed: 1,
            published: 0,
            failed: 0,
            skipped: 1
        }
    );
    assert_eq!(store.published_count(), 0);
    assert_eq!(store.pending_count(), 1);
    assert_eq!(bus.calls.lock().expect("c").len(), 0);
}

#[tokio::test]
async fn subject_is_built_from_event_type() {
    let store = Arc::new(InMemoryOutboxStore::new());
    store.push(make_row(1, "request.created", json!({}), 0));
    let bus = Arc::new(FakeBus::ok());
    let relay = OutboxRelay::new(store.clone(), bus.clone(), 10, 5);

    run(&relay).await;
    let calls = bus.calls.lock().expect("c");
    assert_eq!(calls[0].0, "dropit.outbox.request.created");
}

#[tokio::test]
async fn payload_published_byte_exact_as_json() {
    let store = Arc::new(InMemoryOutboxStore::new());
    let payload = json!({"track": "abc", "n": 42, "ok": true, "arr": [1, 2, 3]});
    store.push(make_row(1, "request.created", payload.clone(), 0));
    let bus = Arc::new(FakeBus::ok());
    let relay = OutboxRelay::new(store.clone(), bus.clone(), 10, 5);

    run(&relay).await;
    let calls = bus.calls.lock().expect("c");
    let expected = serde_json::to_vec(&payload).expect("encode");
    assert_eq!(
        calls[0].1, expected,
        "payload должен уйти байт-в-байт как JSON"
    );
}

#[tokio::test]
async fn empty_batch_does_not_touch_bus() {
    let store = Arc::new(InMemoryOutboxStore::new());
    let bus = Arc::new(FakeBus::ok());
    let relay = OutboxRelay::new(store.clone(), bus.clone(), 10, 5);

    let report = run(&relay).await;
    assert_eq!(report, RelayReport::default());
    assert_eq!(bus.calls.lock().expect("c").len(), 0);
}

#[tokio::test]
async fn batch_size_zero_is_empty_batch() {
    let store = Arc::new(InMemoryOutboxStore::new());
    store.push(make_row(1, "request.created", json!({}), 0));
    let bus = Arc::new(FakeBus::ok());
    let relay = OutboxRelay::new(store.clone(), bus.clone(), 0, 5);

    let report = run(&relay).await;
    assert_eq!(report, RelayReport::default());
    assert_eq!(bus.calls.lock().expect("c").len(), 0);
    assert_eq!(store.published_count(), 0);
}

#[tokio::test]
async fn max_attempts_zero_skips_everything() {
    let store = Arc::new(InMemoryOutboxStore::new());
    store.push(make_row(1, "request.created", json!({}), 0));
    let bus = Arc::new(FakeBus::ok());
    let relay = OutboxRelay::new(store.clone(), bus.clone(), 10, 0);

    let report = run(&relay).await;
    assert_eq!(
        report,
        RelayReport {
            claimed: 1,
            published: 0,
            failed: 0,
            skipped: 1
        }
    );
    assert_eq!(bus.calls.lock().expect("c").len(), 0);
}
