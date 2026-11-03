//! Redis-адаптеры DROP IT: одноразовые WS-тикеты и межнодовый pub/sub.
//!
//! Крейт определяет два абстрактных трейта — [`TicketStore`] (одноразовые
//! билеты WebSocket-рукопожатия с TTL) и [`EventBus`] (шина live-событий между
//! узлами), а также их in-memory и Redis-реализации. Redis-варианты позволяют
//! `gateway-api` вынести состояние тикетов за пределы процесса и рассылать
//! live-события между узлами; in-memory — деградация без Redis и unit-тесты.
//!
//! ## Ключи и топики
//! Билеты хранятся по ключу `{prefix}:ws:ticket:{ticket}` и забираются атомарно
//! через `GETDEL` (одноразовость). События публикуются в канал
//! `{prefix}:bus:{topic}`. Префикс по умолчанию — [`DEFAULT_PREFIX`].
//!
//! ## Деградация
//! Если Redis недоступен, потребитель может построить [`InMemoryTicketStore`] и
//! [`InMemoryEventBus`]: они корректны в рамках одного процесса, но не разделяют
//! состояние между узлами.

#![deny(missing_docs)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::broadcast;
use tracing::warn;

/// Префикс ключей по умолчанию: `dropit:ws:ticket:…`, `dropit:bus:…`.
pub const DEFAULT_PREFIX: &str = "dropit";

/// TTL билета по умолчанию (30 секунд) — соответствует ADR-005.
pub const DEFAULT_TICKET_TTL: Duration = Duration::from_secs(30);

/// Вместимость broadcast-канала события: сколько сообщений буферизуется на
/// подписчика до того, как медленный получатель начнёт терять сообщения.
pub const EVENT_CHANNEL_CAPACITY: usize = 256;

// ----------------------------------------------------------------------------
// Ошибки
// ----------------------------------------------------------------------------

/// Ошибка хранилища Redis-адаптеров.
///
/// Не содержит паник и `unwrap`: все ошибки Redis маппятся сюда.
#[derive(Debug, thiserror::Error)]
pub enum RedisStoreError {
    /// Redis недоступен: соединение не установлено или разорвано.
    #[error("redis unavailable: {0}")]
    Unavailable(String),
    /// Не удалось декодировать значение (невалидный JSON и т.п.).
    #[error("decode error: {0}")]
    Decode(String),
    /// Операция превысила таймаут.
    #[error("redis operation timed out")]
    Timeout,
}

/// Преобразует ошибку `redis` в [`RedisStoreError`]: таймауты → [`RedisStoreError::Timeout`],
/// прочее → [`RedisStoreError::Unavailable`].
fn map_redis_error(err: redis::RedisError) -> RedisStoreError {
    if err.is_timeout() {
        RedisStoreError::Timeout
    } else {
        RedisStoreError::Unavailable(err.to_string())
    }
}

// ----------------------------------------------------------------------------
// Конфигурация
// ----------------------------------------------------------------------------

/// Параметры подключения к Redis и соглашения об именовании ключей.
#[derive(Debug, Clone)]
pub struct RedisConfig {
    /// URL подключения, например `redis://127.0.0.1:6379`.
    pub url: String,
    /// TTL одноразового билета.
    pub ticket_ttl: Duration,
    /// Префикс ключей: `{prefix}:ws:ticket:{ticket}`, `{prefix}:bus:{topic}`.
    pub prefix: String,
}

/// Строит конфигурацию из переменных окружения.
///
/// Возвращает `None`, если переменная `REDIS_URL` не задана или пуста.
/// Необязательные переменные:
/// * `REDIS_PREFIX` — префикс ключей (по умолчанию [`DEFAULT_PREFIX`]);
/// * `REDIS_TICKET_TTL_SECS` — TTL билета в секундах (по умолчанию 30).
///
/// Некорректные значения необязательных переменных игнорируются с предупреждением.
pub fn from_env() -> Option<RedisConfig> {
    let raw = std::env::var("REDIS_URL").ok()?;
    let url = raw.trim().to_string();
    if url.is_empty() {
        return None;
    }
    let prefix = std::env::var("REDIS_PREFIX")
        .ok()
        .map(|p| {
            let p = p.trim().to_string();
            if p.is_empty() {
                DEFAULT_PREFIX.to_string()
            } else {
                p
            }
        })
        .unwrap_or_else(|| DEFAULT_PREFIX.to_string());
    let ticket_ttl = std::env::var("REDIS_TICKET_TTL_SECS")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok().map(Duration::from_secs))
        .unwrap_or(DEFAULT_TICKET_TTL);
    if std::env::var("REDIS_TICKET_TTL_SECS")
        .map(|s| s.trim().parse::<u64>().is_err())
        .unwrap_or(false)
    {
        warn!("REDIS_TICKET_TTL_SECS не парсится как u64, используется значение по умолчанию");
    }
    Some(RedisConfig {
        url,
        ticket_ttl,
        prefix,
    })
}

// ----------------------------------------------------------------------------
// TicketStore
// ----------------------------------------------------------------------------

/// Хранилище одноразовых билетов с TTL.
///
/// `put` сохраняет билет с заданным TTL; `take` атомарно извлекает и удаляет
/// его (GETDEL-семантика): повторный `take` того же билета обязан вернуть `None`.
/// Просроченный по TTL билет также не отдаётся.
#[async_trait]
pub trait TicketStore: Send + Sync {
    /// Сохраняет билет `ticket` с полезной нагрузкой `payload` и TTL `ttl`.
    async fn put(&self, ticket: &str, payload: Value, ttl: Duration)
        -> Result<(), RedisStoreError>;
    /// Атомарно забирает билет; `None`, если его нет, истёк или уже взят.
    async fn take(&self, ticket: &str) -> Result<Option<Value>, RedisStoreError>;

    /// Проверка доступности бэкенда для публичной статус-страницы
    /// (`GET /v1/status/services`). In-memory бэкенд всегда доступен.
    async fn ping(&self) -> Result<(), RedisStoreError> {
        Ok(())
    }

    /// Имя бэкенда для статус-страницы: `redis` или `memory`.
    fn backend(&self) -> &'static str {
        "memory"
    }
}

/// In-memory реализация [`TicketStore`]: `Mutex<HashMap>` с честным TTL и
/// одноразовым `take`. Предназначена для деградации без Redis и unit-тестов.
///
/// Клон дешёвый (`Arc` внутри): клоны разделяют одно хранилище.
#[derive(Debug, Clone, Default)]
pub struct InMemoryTicketStore {
    inner: Arc<Mutex<HashMap<String, TicketEntry>>>,
}

/// Запись билета в in-memory хранилище.
#[derive(Debug, Clone)]
struct TicketEntry {
    /// Полезная нагрузка билета.
    payload: Value,
    /// Момент, после которого билет считается просроченным.
    deadline: Instant,
}

impl InMemoryTicketStore {
    /// Создаёт пустое хранилище.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Захватывает мьютекс, восстанавливаясь после отравления (без паник).
    fn locked(&self) -> MutexGuard<'_, HashMap<String, TicketEntry>> {
        match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[async_trait]
impl TicketStore for InMemoryTicketStore {
    async fn put(
        &self,
        ticket: &str,
        payload: Value,
        ttl: Duration,
    ) -> Result<(), RedisStoreError> {
        let mut map = self.locked();
        let now = Instant::now();
        // Чистим просроченные, чтобы хранилище не росло бесконечно.
        map.retain(|_, entry| entry.deadline > now);
        let deadline = now.checked_add(ttl).unwrap_or(now);
        map.insert(ticket.to_string(), TicketEntry { payload, deadline });
        Ok(())
    }

    async fn take(&self, ticket: &str) -> Result<Option<Value>, RedisStoreError> {
        let mut map = self.locked();
        match map.remove(ticket) {
            None => Ok(None),
            Some(entry) => {
                if entry.deadline <= Instant::now() {
                    Ok(None)
                } else {
                    Ok(Some(entry.payload))
                }
            }
        }
    }
}

/// Redis-реализация [`TicketStore`] на `redis::aio::ConnectionManager`.
///
/// Одноразовость обеспечивается командой `GETDEL`, TTL — опцией `PX` в `SET`.
pub struct RedisTicketStore {
    /// Менеджер соединений с автоматическим переподключением.
    manager: redis::aio::ConnectionManager,
    /// Префикс ключей.
    prefix: String,
}

impl RedisTicketStore {
    /// Подключается к Redis по `url` с префиксом по умолчанию ([`DEFAULT_PREFIX`]).
    pub async fn connect(url: &str) -> Result<Self, RedisStoreError> {
        Self::connect_with_prefix(url, DEFAULT_PREFIX).await
    }

    /// Подключается к Redis по `url` с заданным префиксом ключей.
    pub async fn connect_with_prefix(
        url: &str,
        prefix: impl Into<String>,
    ) -> Result<Self, RedisStoreError> {
        let client = redis::Client::open(url).map_err(map_redis_error)?;
        let manager = build_manager(&client).await.map_err(map_redis_error)?;
        Ok(Self {
            manager,
            prefix: prefix.into(),
        })
    }

    /// Ключ билета: `{prefix}:ws:ticket:{ticket}`.
    fn ticket_key(&self, ticket: &str) -> String {
        format!("{}:ws:ticket:{}", self.prefix, ticket)
    }
}

#[async_trait]
impl TicketStore for RedisTicketStore {
    async fn put(
        &self,
        ticket: &str,
        payload: Value,
        ttl: Duration,
    ) -> Result<(), RedisStoreError> {
        // Нулевой TTL — билет мгновенно просрочен, хранить нечего.
        if ttl.is_zero() {
            return Ok(());
        }
        let body =
            serde_json::to_vec(&payload).map_err(|e| RedisStoreError::Decode(e.to_string()))?;
        let ms = (ttl.as_millis().max(1)) as u64;
        let key = self.ticket_key(ticket);
        let mut conn = self.manager.clone();
        redis::cmd("SET")
            .arg(key.as_str())
            .arg(body.as_slice())
            .arg("PX")
            .arg(ms)
            .query_async::<()>(&mut conn)
            .await
            .map_err(map_redis_error)?;
        Ok(())
    }

    async fn ping(&self) -> Result<(), RedisStoreError> {
        let mut conn = self.manager.clone();
        redis::cmd("PING")
            .query_async::<String>(&mut conn)
            .await
            .map(|_| ())
            .map_err(map_redis_error)
    }

    fn backend(&self) -> &'static str {
        "redis"
    }

    async fn take(&self, ticket: &str) -> Result<Option<Value>, RedisStoreError> {
        let key = self.ticket_key(ticket);
        let mut conn = self.manager.clone();
        let raw: Option<Vec<u8>> = redis::cmd("GETDEL")
            .arg(key.as_str())
            .query_async(&mut conn)
            .await
            .map_err(map_redis_error)?;
        match raw {
            None => Ok(None),
            Some(bytes) => {
                let value: Value = serde_json::from_slice(&bytes)
                    .map_err(|e| RedisStoreError::Decode(e.to_string()))?;
                Ok(Some(value))
            }
        }
    }
}

// ----------------------------------------------------------------------------
// EventBus
// ----------------------------------------------------------------------------

/// Подписка на события шины: приёмник broadcast-канала со строками JSON.
pub type EventSubscription = broadcast::Receiver<String>;

/// Шина live-событий между узлами.
///
/// `publish` сериализует `payload` в JSON и публикует в топик; `subscribe`
/// возвращает [`EventSubscription`], доставляющую сообщения этого топика.
#[async_trait]
pub trait EventBus: Send + Sync {
    /// Публикует `payload` (JSON) в топик `topic`.
    async fn publish(&self, topic: &str, payload: Value) -> Result<(), RedisStoreError>;
    /// Подписывается на топик `topic`, возвращая приёмник событий.
    async fn subscribe(&self, topic: &str) -> Result<EventSubscription, RedisStoreError>;
}

/// In-memory реализация [`EventBus`] на `tokio::broadcast` в одном процессе.
///
/// Клон дешёвый (`Arc` внутри): клоны разделяют одну шину.
#[derive(Debug, Clone, Default)]
pub struct InMemoryEventBus {
    inner: Arc<Mutex<HashMap<String, broadcast::Sender<String>>>>,
}

impl InMemoryEventBus {
    /// Создаёт пустую шину.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Захватывает мьютекс, восстанавливаясь после отравления (без паник).
    fn locked(&self) -> MutexGuard<'_, HashMap<String, broadcast::Sender<String>>> {
        match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[async_trait]
impl EventBus for InMemoryEventBus {
    async fn publish(&self, topic: &str, payload: Value) -> Result<(), RedisStoreError> {
        let body =
            serde_json::to_string(&payload).map_err(|e| RedisStoreError::Decode(e.to_string()))?;
        let sender = {
            let mut map = self.locked();
            match map.get(topic) {
                Some(sender) if sender.receiver_count() > 0 => sender.clone(),
                _ => {
                    // Нет активных подписчиков — очищаем топик и не храним сообщение
                    // (семантика pub/sub: сообщение теряется без подписчика).
                    map.remove(topic);
                    return Ok(());
                }
            }
        };
        // Поздние получатели пропустят — это допустимо для broadcast.
        let _ = sender.send(body);
        Ok(())
    }

    async fn subscribe(&self, topic: &str) -> Result<EventSubscription, RedisStoreError> {
        let mut map = self.locked();
        let sender = map
            .entry(topic.to_string())
            .or_insert_with(|| broadcast::channel::<String>(EVENT_CHANNEL_CAPACITY).0);
        Ok(sender.subscribe())
    }
}

/// Redis-реализация [`EventBus`] на `redis::aio::ConnectionManager`.
///
/// Публикация — команда `PUBLISH`; подписка — отдельное pub/sub-соединение с
/// фоновой задачей, пересылающей сообщения в `tokio::broadcast`.
pub struct RedisEventBus {
    /// Менеджер соединений для публикации.
    manager: redis::aio::ConnectionManager,
    /// Клиент для открытия pub/sub-соединений на каждую подписку.
    client: redis::Client,
    /// Префикс топиков.
    prefix: String,
}

impl RedisEventBus {
    /// Подключается к Redis по `url` с префиксом по умолчанию ([`DEFAULT_PREFIX`]).
    pub async fn connect(url: &str) -> Result<Self, RedisStoreError> {
        Self::connect_with_prefix(url, DEFAULT_PREFIX).await
    }

    /// Подключается к Redis по `url` с заданным префиксом топиков.
    pub async fn connect_with_prefix(
        url: &str,
        prefix: impl Into<String>,
    ) -> Result<Self, RedisStoreError> {
        let client = redis::Client::open(url).map_err(map_redis_error)?;
        let manager = build_manager(&client).await.map_err(map_redis_error)?;
        Ok(Self {
            manager,
            client,
            prefix: prefix.into(),
        })
    }

    /// Канал топика: `{prefix}:bus:{topic}`.
    fn topic_channel(&self, topic: &str) -> String {
        format!("{}:bus:{}", self.prefix, topic)
    }
}

#[async_trait]
impl EventBus for RedisEventBus {
    async fn publish(&self, topic: &str, payload: Value) -> Result<(), RedisStoreError> {
        let body =
            serde_json::to_string(&payload).map_err(|e| RedisStoreError::Decode(e.to_string()))?;
        let channel = self.topic_channel(topic);
        let mut conn = self.manager.clone();
        redis::cmd("PUBLISH")
            .arg(channel.as_str())
            .arg(body)
            .query_async::<i64>(&mut conn)
            .await
            .map_err(map_redis_error)?;
        Ok(())
    }

    async fn subscribe(&self, topic: &str) -> Result<EventSubscription, RedisStoreError> {
        let channel = self.topic_channel(topic);
        let mut pubsub = self
            .client
            .get_async_pubsub()
            .await
            .map_err(map_redis_error)?;
        // Дожидаемся подтверждения SUBSCRIBE, прежде чем вернуть приёмник:
        // гарантирует, что последующая публикация не будет пропущена из-за гонки.
        pubsub
            .subscribe(channel.as_str())
            .await
            .map_err(map_redis_error)?;
        let (tx, rx) = broadcast::channel::<String>(EVENT_CHANNEL_CAPACITY);
        tokio::spawn(async move {
            use futures_util::StreamExt;
            let mut stream = pubsub.into_on_message();
            while let Some(msg) = stream.next().await {
                if let Ok(payload) = msg.get_payload::<String>() {
                    if tx.send(payload).is_err() {
                        // Все приёмники закрыты — выходим.
                        break;
                    }
                }
            }
        });
        Ok(rx)
    }
}

// ----------------------------------------------------------------------------
// Вспомогательное
// ----------------------------------------------------------------------------

/// Строит `ConnectionManager` с быстрым отказом при недоступности Redis:
/// 2 попытки переподключения, таймаут соединения 2 с, таймаут ответа 5 с.
async fn build_manager(
    client: &redis::Client,
) -> Result<redis::aio::ConnectionManager, redis::RedisError> {
    let config = redis::aio::ConnectionManagerConfig::new()
        .set_number_of_retries(2)
        .set_connection_timeout(Duration::from_secs(2))
        .set_response_timeout(Duration::from_secs(5));
    redis::aio::ConnectionManager::new_with_config(client.clone(), config).await
}

// ----------------------------------------------------------------------------
// Unit-тесты (in-memory, без сети)
// ----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::time::sleep;

    #[tokio::test]
    async fn inmemory_ticket_put_take() {
        let store = InMemoryTicketStore::new();
        store
            .put("t1", json!({"sub":"u1"}), Duration::from_secs(30))
            .await
            .expect("put");
        let v = store.take("t1").await.expect("take");
        assert_eq!(v, Some(json!({"sub":"u1"})));
    }

    #[tokio::test]
    async fn inmemory_ticket_take_once() {
        let store = InMemoryTicketStore::new();
        store
            .put("t2", json!(42), Duration::from_secs(30))
            .await
            .expect("put");
        let first = store.take("t2").await.expect("take");
        assert!(first.is_some());
        let second = store.take("t2").await.expect("take");
        assert_eq!(second, None, "повторный take обязан вернуть None");
    }

    #[tokio::test]
    async fn inmemory_ticket_expired_ttl() {
        let store = InMemoryTicketStore::new();
        store
            .put("t3", json!("x"), Duration::from_millis(10))
            .await
            .expect("put");
        sleep(Duration::from_millis(30)).await;
        let v = store.take("t3").await.expect("take");
        assert_eq!(v, None, "просроченный тикет не отдаётся");
    }

    #[tokio::test]
    async fn inmemory_ticket_unknown_returns_none() {
        let store = InMemoryTicketStore::new();
        let v = store.take("nope").await.expect("take");
        assert_eq!(v, None);
    }

    #[tokio::test]
    async fn inmemory_ticket_overwrite_resets_ttl() {
        let store = InMemoryTicketStore::new();
        store
            .put("t4", json!(1), Duration::from_secs(30))
            .await
            .expect("put");
        store
            .put("t4", json!(2), Duration::from_secs(30))
            .await
            .expect("put overwrite");
        let v = store.take("t4").await.expect("take");
        assert_eq!(v, Some(json!(2)), "перезапись обновляет значение");
    }

    #[tokio::test]
    async fn inmemory_bus_publish_subscribe() {
        let bus = InMemoryEventBus::new();
        let mut rx = bus.subscribe("topic").await.expect("subscribe");
        bus.publish("topic", json!({"k":"v"}))
            .await
            .expect("publish");
        let msg = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timeout")
            .expect("channel closed");
        assert_eq!(
            msg,
            serde_json::to_string(&json!({"k":"v"})).expect("encode")
        );
    }

    #[tokio::test]
    async fn inmemory_bus_multiple_subscribers() {
        let bus = InMemoryEventBus::new();
        let mut rx1 = bus.subscribe("t").await.expect("subscribe");
        let mut rx2 = bus.subscribe("t").await.expect("subscribe");
        bus.publish("t", json!(1)).await.expect("publish");
        let m1 = tokio::time::timeout(Duration::from_secs(1), rx1.recv())
            .await
            .expect("timeout")
            .expect("closed");
        let m2 = tokio::time::timeout(Duration::from_secs(1), rx2.recv())
            .await
            .expect("timeout")
            .expect("closed");
        assert_eq!(m1, "1");
        assert_eq!(m2, "1");
    }

    #[tokio::test]
    async fn inmemory_bus_no_subscriber_ok() {
        let bus = InMemoryEventBus::new();
        bus.publish("empty", json!(null)).await.expect("publish");
    }

    #[tokio::test]
    async fn inmemory_bus_topic_isolation() {
        let bus = InMemoryEventBus::new();
        let mut rx_a = bus.subscribe("a").await.expect("subscribe");
        bus.publish("b", json!("hi")).await.expect("publish b");
        bus.publish("a", json!("yo")).await.expect("publish a");
        let m = tokio::time::timeout(Duration::from_secs(1), rx_a.recv())
            .await
            .expect("timeout")
            .expect("closed");
        assert_eq!(m, "\"yo\"");
    }

    #[tokio::test]
    async fn inmemory_bus_clones_share_state() {
        let bus = InMemoryEventBus::new();
        let bus2 = bus.clone();
        let mut rx = bus.subscribe("shared").await.expect("subscribe");
        bus2.publish("shared", json!(true)).await.expect("publish");
        let m = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timeout")
            .expect("closed");
        assert_eq!(m, "true");
    }
}
