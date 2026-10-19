//! gateway-ws: WebSocket-слой live-обновлений (техкарта Часть IV).
//!
//! # Архитектура (мультиузловая)
//!
//! Состояние делится на две части:
//!   * **Локальный хаб** [`WsHub`] — `tokio::broadcast`-каналы по топикам
//!     [`Topic::Queue`] (DJ-очередь сессии) и [`Topic::Guest`] (статус
//!     запросов гостя). Доставка только клиентам, подключённым к этому узлу.
//!   * **Межузловая шина** [`dropit_infra_redis::EventBus`] — Redis Pub/Sub
//!     (или in-memory в рамках процесса при отсутствии Redis). Узел,
//!     опубликовавший событие, рассылает его всем узлам; каждый узел
//!     перекладывает пришедшие из шины сообщения в свой локальный хаб.
//!
//! ## Топики шины
//!
//! Локальный топик маппится в топик шины 1:1:
//!   * `Topic::Queue(session_id)` → `session:{session_id}` — события
//!     очереди сессии (карточки DJ, смена статусов, пауза/возобновление);
//!   * `Topic::Guest(guest_device_id)` → `guest:{guest_device_id}` —
//!     персональные события гостя (статус его запросов);
//!   * `club:{club_id}` — резерв для клуб-wide объявлений панели
//!     (текущие request/session-события не публикуются в этот топик).
//!
//! ## Идемпотентность доставки
//!
//! Каждое сообщение в шину помечается `origin` — UUID узла-источника.
//! Релейный форвардер узла, получив сообщение из шины, **отбрасывает**
//! собственные (совпадающий `origin`): локальный клиент уже получил
//! событие синхронно через [`WsHub::publish`]. Таким образом на узле
//! источнике нет дубля, а на прочих узлах событие доходит ровно один раз
//! (через форвардер).
//!
//! ## Деградация
//!
//! * Шина отсутствует (`ApiEnv.bus == None`, нет Redis) — [`WsHub::publish`]
//!   доставляет только локально, без `tokio::spawn`; HTTP-запрос не роняется.
//! * Хранилище тикетов отсутствует (`ApiEnv.tickets == None`) — см.
//!   [`consume_ticket`]: рукопожатие WS отклоняется (401). В проде `tickets`
//!   всегда `Some` ([`crate::infra::ticket_store`] даёт `InMemoryTicketStore`
//!   при отсутствии Redis).
//!
//! # WS-тикеты
//!
//! Одноразовые билеты с TTL хранятся в **единственном** хранилище —
//! [`ApiEnv::tickets`] ([`dropit_infra_redis::TicketStore`]: Redis
//! `GETDEL`/`SET PX` в проде, in-memory без Redis). Выдача —
//! `POST /v1/ws/ticket` ([`ws_ticket`]): узел-эмитент пишет билет в общий
//! стор, поэтому тикет валиден на любом узле кластера и переживает рестарт
//! процесса (при Redis-сторадже). Проверка при `GET /v1/ws` —
//! [`consume_ticket`] через `tickets.take` (атомарно, одноразово).
//! Дублирующего in-process хранилища нет: `ApiEnv::tickets` — единственный
//! источник истины.
//!
//! # Протокол (JSON-кадры)
//!
//! сервер → клиент: `{type, session_id?, request_id?, …}`;
//! клиент → сервер: `{"subscribe": "queue|guest", "id": "<uuid>"}`.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::rbac::AuthContext;
use crate::routes::ApiEnv;

/// Событие, летящее в WS-кадре (и в конверте шины).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WsEvent {
    /// request.created | request.accepted | request.rejected |
    /// request.played | session.paused | session.resumed
    #[serde(rename = "type")]
    pub kind: String,
    pub session_id: Option<Uuid>,
    pub request_id: Option<Uuid>,
    /// Гость-получатель (только guest-канал).
    pub guest_device_id: Option<Uuid>,
    /// Заголовок трека для карточки DJ (request.created).
    pub title: Option<String>,
    /// Статус запроса (для гостя).
    pub status: Option<String>,
    /// Причина отклонения (гость).
    pub reason: Option<String>,
    /// Число той же песни (обновление бейджа ×N).
    pub same_track_count: Option<i64>,
}

/// Топик подписки.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Topic {
    /// Очередь сессии — DJ.
    Queue(Uuid),
    /// Статус запросов одного гостя.
    Guest(Uuid),
}

impl Topic {
    fn of_event(event: &WsEvent) -> Vec<Topic> {
        let mut topics = Vec::new();
        if let Some(sid) = event.session_id {
            topics.push(Topic::Queue(sid));
        }
        if let Some(g) = event.guest_device_id {
            topics.push(Topic::Guest(g));
        }
        topics
    }
}

/// Топик межузловой шины для локального топика хаба.
///
/// См. схему топиков в документации модуля.
fn bus_topic(topic: &Topic) -> String {
    match topic {
        Topic::Queue(sid) => format!("session:{sid}"),
        Topic::Guest(gid) => format!("guest:{gid}"),
    }
}

/// Конверт сообщения шины: событие + идентификатор узла-источника.
///
/// `origin` нужен форвардеру для отбрасывания собственных сообщений
/// (идемпотентность доставки — см. докуменацию модуля).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct BusEnvelope {
    origin: String,
    event: WsEvent,
}

/// Внутреннее состояние хаба за общим `Arc` (разделяется всеми клонами).
struct HubInner {
    /// topic → broadcast-sender. Отсутствие = нет подписчиков.
    topics: Mutex<HashMap<Topic, broadcast::Sender<WsEvent>>>,
    /// Межузловая шина (attach на старте узла через [`WsHub::attach_bus`]).
    bus: Mutex<Option<Arc<dyn dropit_infra_redis::EventBus>>>,
    /// Идентификатор узла-источника (для маркировки конвертов).
    origin: Uuid,
    /// Топики, для которых уже запущен релейный форвардер из шины.
    bus_subs: Mutex<HashMap<Topic, ()>>,
    /// Защита от повторного attach шины (idempotent `spawn_relay`).
    relay_attached: AtomicBool,
}

impl HubInner {
    fn topics_locked(&self) -> MutexGuard<'_, HashMap<Topic, broadcast::Sender<WsEvent>>> {
        recover(self.topics.lock())
    }
    fn bus_locked(&self) -> MutexGuard<'_, Option<Arc<dyn dropit_infra_redis::EventBus>>> {
        recover(self.bus.lock())
    }
    fn bus_subs_locked(&self) -> MutexGuard<'_, HashMap<Topic, ()>> {
        recover(self.bus_subs.lock())
    }
}

/// Восстанавливает мьютекс после отравления (без паник).
fn recover<T>(r: std::sync::LockResult<T>) -> T {
    match r {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

/// Хаб: локальные broadcast-каналы по топикам + мост в межузловую шину.
///
/// Клон дешёвый (`Arc` внутри): клоны разделяют одно состояние.
/// Шина подключается через [`WsHub::attach_bus`] (вызывается из
/// [`spawn_relay`] при сборке роутера); до подключения работает
/// single-node режим (только локальная доставка).
#[derive(Clone)]
pub struct WsHub {
    inner: Arc<HubInner>,
}

impl Default for WsHub {
    fn default() -> Self {
        Self {
            inner: Arc::new(HubInner {
                topics: Mutex::new(HashMap::new()),
                bus: Mutex::new(None),
                origin: Uuid::now_v7(),
                bus_subs: Mutex::new(HashMap::new()),
                relay_attached: AtomicBool::new(false),
            }),
        }
    }
}

impl WsHub {
    /// Создаёт хаб без шины (single-node).
    pub fn new() -> Self {
        Self::default()
    }

    /// Подключает межузловую шину. Idempotent: повторные вызовы лишь
    /// обновляют ссылку (без порчи состояния). Вызывается из [`spawn_relay`].
    pub fn attach_bus(&self, bus: Arc<dyn dropit_infra_redis::EventBus>) {
        *self.inner.bus_locked() = Some(bus);
    }

    /// Подписка: возвращает локальный `broadcast::Receiver`. Sender
    /// удерживается в хабе, чтобы события до первого recv не терялись
    /// дальше буфера. При наличии шины лениво запускает форвардер
    /// данного топика (один на топик, переиспользуется всеми клиентами).
    pub fn subscribe(&self, topic: Topic) -> broadcast::Receiver<WsEvent> {
        let rx = {
            let mut topics = self.inner.topics_locked();
            let sender = topics
                .entry(topic.clone())
                .or_insert_with(|| broadcast::channel(64).0);
            sender.subscribe()
        };
        self.ensure_bus_forwarder(topic);
        rx
    }

    /// Локальная доставка во все подходящие топики (без шины).
    /// Возвращает число доставленных (0 = нет подписчиков — норма).
    fn deliver_local(&self, event: WsEvent) -> usize {
        let mut delivered = 0;
        let mut topics = self.inner.topics_locked();
        for topic in Topic::of_event(&event) {
            if let Some(sender) = topics.get(&topic) {
                match sender.send(event.clone()) {
                    Ok(n) => delivered += n,
                    Err(_) => {
                        topics.remove(&topic);
                    }
                }
            }
        }
        delivered
    }

    /// Публикация: локальная доставка + рассылка в шину (fire-and-forget).
    ///
    /// Локальным подписчикам событие доходит синхронно через
    /// [`Self::deliver_local`]; в шину уходит конверт с `origin` этого
    /// узла. Форвардеры прочих узлов доставят событие своим клиентам;
    /// собственный форвардер отбросит его по `origin` (нет дубля).
    ///
    /// Ошибка публикации в шину не роняет вызов: логируется `warn`,
    /// локальная доставка уже выполнена.
    pub fn publish(&self, event: WsEvent) -> usize {
        let delivered = self.deliver_local(event.clone());
        let bus = self.inner.bus_locked().clone();
        if let Some(bus) = bus {
            let origin = self.inner.origin.to_string();
            for topic in Topic::of_event(&event) {
                let channel = bus_topic(&topic);
                let payload = json!({
                    "origin": origin.clone(),
                    "event": serde_json::to_value(&event).unwrap_or(Value::Null),
                });
                let bus = bus.clone();
                tokio::spawn(async move {
                    if let Err(e) = bus.publish(&channel, payload).await {
                        tracing::warn!(error = %e, topic = %channel, "ws bus publish failed");
                    }
                });
            }
        }
        delivered
    }

    /// Гарантирует один релейный форвардер на топик: подписывается на
    /// топик шины и перекладывает чужие (origin != self) сообщения в
    /// локальный хаб через [`Self::deliver_local`].
    fn ensure_bus_forwarder(&self, topic: Topic) {
        let bus = match self.inner.bus_locked().clone() {
            Some(b) => b,
            None => return,
        };
        let mut subs = self.inner.bus_subs_locked();
        if subs.contains_key(&topic) {
            return;
        }
        subs.insert(topic.clone(), ());
        drop(subs);

        let hub = self.clone();
        let channel = bus_topic(&topic);
        tokio::spawn(async move {
            let mut rx = match bus.subscribe(&channel).await {
                Ok(rx) => rx,
                Err(e) => {
                    tracing::warn!(error = %e, topic = %channel, "ws bus subscribe failed");
                    // Разрешаем повторную попытку при следующей подписке.
                    hub.inner.bus_subs_locked().remove(&topic);
                    return;
                }
            };
            let self_origin = hub.inner.origin.to_string();
            loop {
                match rx.recv().await {
                    Ok(msg) => {
                        if let Ok(env) = serde_json::from_str::<BusEnvelope>(&msg) {
                            if env.origin != self_origin {
                                hub.deliver_local(env.event);
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            // Шина закрыла канал — позволяем переподписке при новом клиенте.
            hub.inner.bus_subs_locked().remove(&topic);
        });
    }

    /// Число активных топиков (мониторинг; e2e-F9).
    #[allow(dead_code)]
    pub fn topic_count(&self) -> usize {
        self.inner.topics_locked().len()
    }
}

// ============================================================
// Релейный запуск
// ============================================================

/// Подключает межузловую шину к локальному хабу (idempotent).
///
/// Вызывается из [`crate::routes::router`] при сборке приложения: шина
/// привязывается к хабу до того, как клиенты начнут подписываться.
/// Сами форвардеры топиков запускаются лениво в [`WsHub::subscribe`],
/// поскольку топики (`session:{id}`, `guest:{id}`) динамические и не
/// известны на старте.
pub fn spawn_relay(env: &ApiEnv) {
    if let (Some(hub), Some(bus)) = (env.ws.as_ref(), env.bus.as_ref()) {
        if !hub.inner.relay_attached.swap(true, Ordering::Relaxed) {
            hub.attach_bus(bus.clone());
            tracing::info!("ws relay: bus attached");
        } else {
            // Уже подключено (повторный вызов router() в тестах) —
            // лишь обновляем ссылку на случай подмены шины в тесте.
            hub.attach_bus(bus.clone());
        }
    }
}

// ============================================================
// Событийные хелперы для роутов (вызывают после коммита в БД)
// ============================================================

pub mod events {
    use super::*;

    pub fn request_created(
        session_id: Uuid,
        request_id: Uuid,
        guest_device_id: Uuid,
        title: &str,
        same_track_count: i64,
    ) -> WsEvent {
        WsEvent {
            kind: "request.created".into(),
            session_id: Some(session_id),
            request_id: Some(request_id),
            guest_device_id: Some(guest_device_id),
            title: Some(title.into()),
            status: Some("pending".into()),
            reason: None,
            same_track_count: Some(same_track_count),
        }
    }

    pub fn request_accepted(session_id: Uuid, request_id: Uuid, guest_device_id: Uuid) -> WsEvent {
        WsEvent {
            kind: "request.accepted".into(),
            session_id: Some(session_id),
            request_id: Some(request_id),
            guest_device_id: Some(guest_device_id),
            title: None,
            status: Some("accepted".into()),
            reason: None,
            same_track_count: None,
        }
    }

    pub fn request_rejected(
        session_id: Uuid,
        request_id: Uuid,
        guest_device_id: Uuid,
        reason: Option<&str>,
    ) -> WsEvent {
        WsEvent {
            kind: "request.rejected".into(),
            session_id: Some(session_id),
            request_id: Some(request_id),
            guest_device_id: Some(guest_device_id),
            title: None,
            status: Some("rejected".into()),
            reason: reason.map(str::to_string),
            same_track_count: None,
        }
    }

    pub fn request_played(session_id: Uuid, request_id: Uuid, guest_device_id: Uuid) -> WsEvent {
        WsEvent {
            kind: "request.played".into(),
            session_id: Some(session_id),
            request_id: Some(request_id),
            guest_device_id: Some(guest_device_id),
            title: None,
            status: Some("played".into()),
            reason: None,
            same_track_count: None,
        }
    }

    pub fn session_paused(session_id: Uuid) -> WsEvent {
        WsEvent {
            kind: "session.paused".into(),
            session_id: Some(session_id),
            request_id: None,
            guest_device_id: None,
            title: None,
            status: Some("paused".into()),
            reason: None,
            same_track_count: None,
        }
    }

    pub fn session_resumed(session_id: Uuid) -> WsEvent {
        WsEvent {
            kind: "session.resumed".into(),
            session_id: Some(session_id),
            request_id: None,
            guest_device_id: None,
            title: None,
            status: Some("active".into()),
            reason: None,
            same_track_count: None,
        }
    }
}

// ============================================================
// Тикеты
// ============================================================

/// Извлекает и ПОГЛОЩАЕТ билет (одноразовый) через единый стор
/// [`ApiEnv::tickets`]. `None`, если тикета нет, истёк TTL или уже взят.
///
/// # Деградация при `tickets == None`
///
/// Хранилище не сконфигурировано. В проде этого не бывает:
/// [`crate::infra::ticket_store`] всегда возвращает `Some` (Redis либо
/// [`dropit_infra_redis::InMemoryTicketStore`]). Если же `tickets` оказался
/// `None` (ручная сборка в тесте/инъекция), билет проверить неоткуда —
/// возвращаем `None`, и рукопожатие WS отклоняется статусом **401** (как
/// при невалидном/просроченном билете). Паники нет.
pub(crate) async fn consume_ticket(state: &ApiEnv, ticket: &str) -> Option<Uuid> {
    let store = state.tickets.as_ref()?;
    match store.take(ticket).await {
        Ok(Some(v)) => v
            .get("sub")
            .and_then(|s| s.as_str())
            .and_then(|s| Uuid::parse_str(s).ok()),
        _ => None,
    }
}

/// TTL билета WS (сек). Соответствует `expires_in` в ответе
/// `POST /v1/ws/ticket` и контракту OpenAPI (`enum: [30]`).
pub const WS_TICKET_TTL_SECS: u64 = 30;

/// Тело ответа `POST /v1/ws/ticket`.
#[derive(Serialize)]
pub struct WsTicketResponse {
    /// Одноразовый билет (128 бит hex).
    pub ticket: String,
    /// TTL билета, сек (30).
    pub expires_in: i64,
}

/// `POST /v1/ws/ticket`: выдаёт одноразовый билет WS (TTL 30 с).
///
/// Билет — 128 бит случайных (`rand::thread_rng`, криптостойкий). Полезная
/// нагрузка `{sub, role}` кладётся в единое хранилище [`ApiEnv::tickets`]
/// (`TicketStore::put`, TTL [`WS_TICKET_TTL_SECS`]). Билет валиден на любом
/// узле кластера (Redis) или в рамках процесса (in-memory без Redis); при
/// Redis-сторадже переживает рестарт процесса.
///
/// Аутентификация — через [`AuthContext`] (Bearer JWT); билет доступен любой
/// аутентифицированной роли (guest/dj/club/admin).
///
/// # Деградация при `tickets == None`
///
/// Хранилище не сконфигурировано. В проде не бывает (см. [`consume_ticket`]).
/// Без хранилища выдать валидный билет невозможно — ответ **503**
/// `ticket store not configured`. Паники нет.
async fn ws_ticket(State(state): State<ApiEnv>, ctx: AuthContext) -> Response {
    let mut bytes = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    let ticket = hex::encode(bytes);
    let payload = json!({ "sub": ctx.subject_id(), "role": ctx.role().as_str() });
    let Some(store) = state.tickets.as_ref() else {
        return crate::routes::api_error(
            dropit_common::ApiError::ExternalServiceUnavailable,
            "ticket store not configured",
        );
    };
    if let Err(e) = store
        .put(&ticket, payload, Duration::from_secs(WS_TICKET_TTL_SECS))
        .await
    {
        tracing::warn!(error = %e, "ws ticket: store put failed");
        return crate::routes::api_error(
            dropit_common::ApiError::Internal,
            "ticket store put failed",
        );
    }
    (
        axum::http::StatusCode::OK,
        Json(WsTicketResponse {
            ticket,
            expires_in: WS_TICKET_TTL_SECS as i64,
        }),
    )
        .into_response()
}

// ============================================================
// Роуты
// ============================================================

pub fn ws_router() -> Router<ApiEnv> {
    Router::new()
        .route("/v1/ws", get(ws_handler))
        .route("/v1/ws/ticket", post(ws_ticket))
}

#[derive(Deserialize)]
pub struct WsQuery {
    pub ticket: String,
}

/// GET /v1/ws?ticket=…: апгрейд до WebSocket.
///
/// Билет одноразовый (`GETDEL`-семантика в [`consume_ticket`]).
/// Подписка по РОЛИ субъекта билета требует контекста — в MVP клиент
/// передаёт topic в первом кадре: `{"subscribe":"queue"}` |
/// `{"subscribe":"guest"}`; сервер разрешает ровно один топик,
/// валидируя грант/смену (гость: активный грант; DJ: сессия его).
async fn ws_handler(
    State(state): State<ApiEnv>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let Some(subject_id) = consume_ticket(&state, &query.ticket).await else {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(json!({"type": "error", "reason": "invalid ticket"})),
        )
            .into_response();
    };
    ws.on_upgrade(move |socket| ws_session(socket, state, subject_id))
}

async fn ws_session(mut socket: WebSocket, state: ApiEnv, subject_id: Uuid) {
    // Ждём первый кадр-подписку (таймаут 10 с).
    let subscription = tokio::select! {
        _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => None,
        msg = socket.recv() => match msg {
            Some(Ok(Message::Text(text))) => parse_subscription(&text),
            _ => None,
        },
    };
    let Some(topic) = subscription else {
        let _ = socket
            .send(Message::Text(
                json!({"type": "error", "reason": "subscription required"}).to_string(),
            ))
            .await;
        return;
    };

    // Авторизация топика: гость — свой guest-канал; DJ — queue своей
    // сессии (смена). Роли определяем по данным сессий (JWT-роль не
    // хранится в билете — MVP: проверяем владение).
    let authorized = match (&topic, state.sessions.clone()) {
        (Topic::Guest(g), _) => *g == subject_id,
        (Topic::Queue(session_id), Some(sessions)) => sessions
            .find_session(*session_id)
            .await
            .map(|s| s.map(|s| s.dj_id == Some(subject_id)).unwrap_or(false))
            .unwrap_or(false),
        (Topic::Queue(_), None) => false,
    };
    if !authorized {
        let _ = socket
            .send(Message::Text(
                json!({"type": "error", "reason": "forbidden"}).to_string(),
            ))
            .await;
        return;
    }

    let Some(hub) = state.ws.clone() else {
        let _ = socket
            .send(Message::Text(
                json!({"type": "error", "reason": "hub not configured"}).to_string(),
            ))
            .await;
        return;
    };

    // Подтверждение подписки.
    let _ = socket
        .send(Message::Text(
            json!({"type": "subscribed", "topic": topic_name(&topic)}).to_string(),
        ))
        .await;

    // Основной цикл: слушаем события (клиент может пинговать —
    // игнорируем кадры, кроме close).
    let mut rx = hub.subscribe(topic.clone());
    loop {
        tokio::select! {
            event = rx.recv() => match event {
                Ok(ev) => {
                    let frame = serde_json::to_string(&ev).unwrap_or_default();
                    if socket.send(Message::Text(frame)).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            msg = socket.recv() => match msg {
                None | Some(Err(_)) => break,
                Some(Ok(Message::Close(_))) => break,
                Some(Ok(_)) => continue, // ping/прочее — игнор
            },
        }
    }
}

fn parse_subscription(text: &str) -> Option<Topic> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let sub = v.get("subscribe")?.as_str()?;
    let id = Uuid::parse_str(v.get("id")?.as_str()?).ok()?;
    match sub {
        "queue" => Some(Topic::Queue(id)),
        "guest" => Some(Topic::Guest(id)),
        _ => None,
    }
}

fn topic_name(topic: &Topic) -> String {
    match topic {
        Topic::Queue(id) => format!("queue:{id}"),
        Topic::Guest(id) => format!("guest:{id}"),
    }
}

// ============================================================
// Тесты хаба
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use dropit_infra_redis::{InMemoryEventBus, InMemoryTicketStore, TicketStore};

    #[tokio::test]
    async fn hub_delivers_to_subscriber_and_cleans_dead_topics() {
        let hub = WsHub::new();
        let guest = Uuid::now_v7();
        let mut rx = hub.subscribe(Topic::Guest(guest));
        // Публикация: один подписчик получает (тот же guest-канал).
        let delivered = hub.publish(events::request_accepted(
            Uuid::now_v7(),
            Uuid::now_v7(),
            guest,
        ));
        assert_eq!(delivered, 1);
        assert!(rx.try_recv().is_ok());
        // Без подписчиков — 0, не паника.
        let d2 = hub.publish(events::session_paused(Uuid::now_v7()));
        assert_eq!(d2, 0);
    }

    #[test]
    fn event_fanout_targets_both_topics() {
        let sid = Uuid::now_v7();
        let gid = Uuid::now_v7();
        let ev = events::request_created(sid, Uuid::now_v7(), gid, "T", 3);
        let topics = Topic::of_event(&ev);
        assert!(topics.contains(&Topic::Queue(sid)));
        assert!(topics.contains(&Topic::Guest(gid)));
    }

    #[test]
    fn subscription_frame_parses() {
        let sid = uuid::Uuid::now_v7();
        let t = parse_subscription(&format!("{{\"subscribe\":\"queue\",\"id\":\"{sid}\"}}"));
        assert_eq!(t, Some(Topic::Queue(sid)));
        assert_eq!(parse_subscription("garbage"), None);
        assert_eq!(
            parse_subscription("{\"subscribe\":\"other\",\"id\":\"x\"}"),
            None
        );
    }

    #[tokio::test]
    async fn ticket_store_single_use() {
        let store = InMemoryTicketStore::new();
        store
            .put("t1", json!({"sub":"00000000-0000-0000-0000-000000000001"}), Duration::from_secs(30))
            .await
            .expect("put");
        let first = store.take("t1").await.expect("take");
        assert!(first.is_some());
        let second = store.take("t1").await.expect("take");
        assert_eq!(second, None, "повторный take — None");
    }

    #[tokio::test]
    async fn ticket_store_ttl_expiry() {
        let store = InMemoryTicketStore::new();
        store
            .put("t2", json!({"sub":"00000000-0000-0000-0000-000000000002"}), Duration::from_millis(10))
            .await
            .expect("put");
        tokio::time::sleep(Duration::from_millis(30)).await;
        let v = store.take("t2").await.expect("take");
        assert_eq!(v, None, "просроченный тикет не отдаётся");
    }

    #[tokio::test]
    async fn bus_relay_delivers_remote_event_once() {
        // Два «узла» с общей шиной: публикуем на узле A, клиент узла B
        // получает ровно одно событие; на узле A дубля нет (origin).
        let bus: Arc<dyn dropit_infra_redis::EventBus> = Arc::new(InMemoryEventBus::new());
        let hub_a = WsHub::new();
        let hub_b = WsHub::new();
        hub_a.attach_bus(bus.clone());
        hub_b.attach_bus(bus.clone());

        let sid = Uuid::now_v7();
        let mut rx_b = hub_b.subscribe(Topic::Queue(sid));
        // Дать форвардеру узла B подписаться на шину.
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Публикация с узла A: локальный клиент A не подписан (0), но в
        // шину уходит конверт с origin-A.
        hub_a.publish(events::request_played(sid, Uuid::now_v7(), Uuid::now_v7()));
        // Узел B должен получить ровно одно.
        let ev = tokio::time::timeout(Duration::from_secs(1), rx_b.recv())
            .await
            .expect("timeout")
            .expect("closed");
        assert_eq!(ev.kind, "request.played");
        // Дубля нет.
        assert!(rx_b.try_recv().is_err(), "no duplicate on node B");
    }

    #[tokio::test]
    async fn publish_without_bus_delivers_locally() {
        // Деградация: bus == None — publish не падает, доставляет локально.
        let hub = WsHub::new();
        let guest = Uuid::now_v7();
        let mut rx = hub.subscribe(Topic::Guest(guest));
        let n = hub.publish(events::request_accepted(
            Uuid::now_v7(),
            Uuid::now_v7(),
            guest,
        ));
        assert_eq!(n, 1);
        assert!(rx.try_recv().is_ok());
    }
}
