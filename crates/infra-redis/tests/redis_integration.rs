//! Интеграционные тесты Redis-реализаций `TicketStore` и `EventBus`.
//!
//! Подключение берётся из `REDIS_URL` (по умолчанию `redis://127.0.0.1:6379`).
//! Если Redis недоступен — тест печатает `redis unavailable, skipped` и
//! завершается успешно (без `#[ignore]`): локально Redis есть, тест реально
//! проходит. Ключи уникальны (uuid-префикс), чтобы параллельные прогоны не
//! мешали друг другу.

use dropit_infra_redis::{EventBus, RedisEventBus, RedisTicketStore, TicketStore};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::{sleep, timeout};

/// URL Redis из окружения или локальный дефолт.
fn redis_url() -> String {
    std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string())
}

/// Уникальный префикс ключей для изоляции от параллельных прогонов.
fn unique_prefix() -> String {
    format!("dropit-test-{}", uuid::Uuid::now_v7())
}

#[tokio::test]
async fn redis_ticket_store_take_once_and_ttl() {
    let url = redis_url();
    let prefix = unique_prefix();
    let store = match RedisTicketStore::connect_with_prefix(&url, &prefix).await {
        Ok(s) => s,
        Err(e) => {
            println!("redis unavailable, skipped: {e}");
            return;
        }
    };

    // put + take возвращает payload.
    let ticket = format!("t-{}", uuid::Uuid::now_v7());
    store
        .put(&ticket, json!({"sub":"u1"}), Duration::from_secs(30))
        .await
        .expect("put");
    let v = store.take(&ticket).await.expect("take");
    assert_eq!(v, Some(json!({"sub":"u1"})));

    // Повторный take — None (GETDEL, одноразовость).
    let v2 = store.take(&ticket).await.expect("take again");
    assert_eq!(v2, None, "повторный take обязан вернуть None");

    // Несуществующий билет — None.
    let v3 = store
        .take(&format!("missing-{}", uuid::Uuid::now_v7()))
        .await
        .expect("take missing");
    assert_eq!(v3, None);

    // Просроченный TTL — None.
    let ticket2 = format!("e-{}", uuid::Uuid::now_v7());
    store
        .put(&ticket2, json!("exp"), Duration::from_millis(50))
        .await
        .expect("put short ttl");
    sleep(Duration::from_millis(120)).await;
    let v4 = store.take(&ticket2).await.expect("take expired");
    assert_eq!(v4, None, "просроченный тикет не отдаётся");
}

#[tokio::test]
async fn redis_event_bus_publish_subscribe_cross_instance() {
    let url = redis_url();
    let prefix = unique_prefix();
    let bus_a = match RedisEventBus::connect_with_prefix(&url, &prefix).await {
        Ok(b) => b,
        Err(e) => {
            println!("redis unavailable, skipped: {e}");
            return;
        }
    };
    let bus_b = match RedisEventBus::connect_with_prefix(&url, &prefix).await {
        Ok(b) => b,
        Err(e) => {
            println!("redis unavailable, skipped: {e}");
            return;
        }
    };

    // Подписка на одной инстанции, публикация на другой — доказывает
    // межузловую доставку (а не локальный broadcast).
    let topic = format!("topic-{}", uuid::Uuid::now_v7());
    let mut rx = bus_b.subscribe(&topic).await.expect("subscribe");
    // SUBSCRIBE уже подтверждён, но даём серверу момент для гарантии.
    sleep(Duration::from_millis(50)).await;

    let payload = json!({"hello":"world"});
    bus_a
        .publish(&topic, payload.clone())
        .await
        .expect("publish");

    let msg = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("timeout waiting for message")
        .expect("channel closed");
    let received: Value = serde_json::from_str(&msg).expect("decode payload");
    assert_eq!(received, payload);

    // Вторая подписка на тот же топик получает следующее сообщение.
    let mut rx2 = bus_b.subscribe(&topic).await.expect("subscribe again");
    sleep(Duration::from_millis(50)).await;
    bus_a.publish(&topic, json!(7)).await.expect("publish 2");
    let msg2 = timeout(Duration::from_secs(5), rx2.recv())
        .await
        .expect("timeout waiting for message 2")
        .expect("channel closed");
    assert_eq!(msg2, "7");
}
