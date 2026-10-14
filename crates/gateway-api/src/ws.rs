//! gateway-ws: WebSocket-слой live-обновлений (техкарта Часть IV).
//!
//! Архитектура MVP (single-node):
//!   * `WsHub` — in-memory broadcast-канал (tokio broadcast) с
//!     топика-подписками: `queue:{session_id}` (DJ) и
//!     `guest:{guest_device_id}` (гость);
//!   * POST /v1/ws/ticket — одноразовый билет (JWT-субъект);
//!   * GET /v1/ws?ticket=… — апгрейд; билет ПОГЛОЩАЕТСЯ (GETDEL),
//!     подписка по роли субъекта;
//!   * продакшен: Redis Pub/Sub-релей между узлами + NATS outbox
//!     (документировано в IMPLEMENTATION_STATUS — multi-node TODO).
//!
//! Протокол (JSON-кадры):
//!   сервер → клиент: {type, session_id?, request_id?, …}
//!   клиент → сервер: {"subscribe": "topic"} (MVP: подписка на
//!   апгрейде, кадры-подписки — расширение).
//!
//! Клиенты: приложение DJ (обновление очереди), гость (статус
//! своего запроса).

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::requests_routes::consume_ws_ticket;
use crate::routes::ApiEnv;

/// Событие, летящее в WS-кадре.
#[derive(Debug, Clone, serde::Serialize)]
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

/// Хаб: broadcast-канал на топик.
///
/// MVP: один узел API; multi-node — Redis Pub/Sub-релей (прод-TODO,
/// техкарта Часть IV «WS»).
#[derive(Clone, Default)]
pub struct WsHub {
    /// topic → sender. Отсутствие = нет подписчиков.
    topics: Arc<Mutex<HashMap<Topic, broadcast::Sender<WsEvent>>>>,
}

impl WsHub {
    pub fn new() -> Self {
        Self::default()
    }

    /// Подписка: возвращает Receiver. Sender удерживается в хабе,
    /// чтобы события до первого recv не терялись дальше буфера.
    pub fn subscribe(&self, topic: Topic) -> broadcast::Receiver<WsEvent> {
        let mut topics = self.topics.lock().expect("ws hub lock");
        let sender = topics
            .entry(topic)
            .or_insert_with(|| broadcast::channel(64).0);
        sender.subscribe()
    }

    /// Публикация во все подходящие топики. Возвращает число
    /// доставленных (0 = нет подписчиков — норма).
    pub fn publish(&self, event: WsEvent) -> usize {
        let mut delivered = 0;
        let mut topics = self.topics.lock().expect("ws hub lock");
        for topic in Topic::of_event(&event) {
            if let Some(sender) = topics.get(&topic) {
                // Err = нет живых ресиверов: чистим мёртвый топик.
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

    /// Число активных топиков (мониторинг; e2e-F9).
    #[allow(dead_code)]
    pub fn topic_count(&self) -> usize {
        self.topics.lock().expect("ws hub lock").len()
    }
}

/// Событийные хелперы для роутов (вызывают после коммита в БД).
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
// Роуты
// ============================================================

pub fn ws_router() -> Router<ApiEnv> {
    Router::new().route("/v1/ws", get(ws_handler))
}

#[derive(Deserialize)]
pub struct WsQuery {
    pub ticket: String,
}

/// GET /v1/ws?ticket=…: апгрейд до WebSocket.
///
/// Билет одноразовый (GETDEL-семантика в consume_ws_ticket).
/// Подписка по РОЛИ субъекта билета требует контекста — в MVP
/// клиент передаёт topic в первом кадре: {"subscribe":"queue"} |
/// {"subscribe":"guest"}; сервер разрешает ровно один топик,
/// валидируя грант/смену (гость: активный грант; DJ: сессия его).
async fn ws_handler(
    State(state): State<ApiEnv>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let Some(subject_id) = consume_ws_ticket(&query.ticket) else {
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
}
