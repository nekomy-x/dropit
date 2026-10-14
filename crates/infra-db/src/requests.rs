//! Репозиторий запросов и очереди DJ (F4, без денег).
//!
//! Правила (техкарта Часть IV/V):
//!   * POST /requests: живой грант session_guest + session.status ==
//!     active (клиентский session_id на веру НЕ принимается);
//!   * пауза приёма (paused_acceptance) блокирует создание;
//!   * Idempotency-Key: дубль → replay сохранённого ответа
//!     (idempotency_response), второй TX не создаётся;
//!   * очередь DJ: сортировка new (created_at DESC) | hot — по
//!     счётчику запросов той же песни (canonical-ключ), не по id;
//!   * DJ-действия: no-op на дубли (стейт-машина domain::request),
//!     optimistic locking по version;
//!   * каждое изменение — событие в outbox (ADR-002) — источник
//!     уведомлений WS (ADR-005).

use chrono::{DateTime, Utc};
use dropit_domain::{RequestAction, RequestStatus};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    #[error("database error")]
    Db(#[from] sqlx::Error),
    #[error("session not active")]
    SessionNotActive,
    #[error("no live grant")]
    NoGrant,
    #[error("idempotency key already used by another guest")]
    IdempotencyConflict,
}

#[derive(Debug, sqlx::FromRow)]
pub struct RequestRow {
    pub id: Uuid,
    pub session_id: Uuid,
    pub guest_device_id: Uuid,
    pub track_cache_id: Uuid,
    pub style: Option<String>,
    pub price_minor: i64,
    pub status: String,
    pub reject_reason: Option<String>,
    pub played_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub version: i32,
}

impl RequestRow {
    pub fn request_status(&self) -> Option<RequestStatus> {
        match self.status.as_str() {
            "pending" => Some(RequestStatus::Pending),
            "accepted" => Some(RequestStatus::Accepted),
            "rejected" => Some(RequestStatus::Rejected),
            "played" => Some(RequestStatus::Played),
            "expired" => Some(RequestStatus::Expired),
            _ => None,
        }
    }
}

/// Карточка запроса для очереди DJ / гостя.
#[derive(Debug, sqlx::FromRow)]
pub struct QueueItem {
    pub id: Uuid,
    pub session_id: Uuid,
    pub guest_device_id: Uuid,
    pub track_cache_id: Uuid,
    pub style: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub title: String,
    pub artist: String,
    pub cover_url: Option<String>,
    pub preview_url: Option<String>,
    pub isrc: Option<String>,
    /// Сколько раз эту же песню (canonical-ключ) запросили в сессии.
    pub same_track_count: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct ReplayRow {
    request_id: Uuid,
    response: serde_json::Value,
}

#[derive(Clone)]
pub struct RequestRepo {
    pool: PgPool,
}

impl RequestRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Пул для построения других репозиториев.
    pub fn pool(&self) -> PgPool {
        self.pool.clone()
    }

    // ---------------------------------------------------------
    // Создание запроса (гейт + идемпотентность + outbox)
    // ---------------------------------------------------------

    /// Создаёт запрос. Атомарно:
    /// (1) проверка session.status == active (FOR SHARE — гонка с паузой);
    /// (2) проверка живого гранта session_guest;
    /// (3) INSERT request;
    /// (4) INSERT idempotency_response (UNIQUE — дубль ключа отловит
    ///     вызывающий ДО второго TX);
    /// (5) outbox(request.created).
    /// Возвращает Ok(Ok(request)) при создании либо Ok(Err(replay))
    /// — если ключ уже использован этим гостем (дубль POST).
    pub async fn create_request(
        &self,
        session_id: Uuid,
        guest_device_id: Uuid,
        track_cache_id: Uuid,
        style: Option<&str>,
        idempotency_key: &str,
    ) -> Result<Result<RequestRow, serde_json::Value>, RequestError> {
        let mut tx = self.pool.begin().await?;

        // Replay: тот же ключ + тот же гость → сохранённый ответ.
        if let Some(replay) = sqlx::query_as::<_, ReplayRow>(
            "SELECT request_id, response FROM idempotency_response WHERE key = $1",
        )
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            if replay.request_id != Uuid::nil() {
                // Тот же ключ, тот же гость? Чужой гость с тем же ключом — конфликт.
                let (owner,): (Uuid,) = sqlx::query_as(
                    "SELECT guest_device_id FROM idempotency_response WHERE key = $1",
                )
                .bind(idempotency_key)
                .fetch_one(&mut *tx)
                .await?;
                if owner != guest_device_id {
                    return Err(RequestError::IdempotencyConflict);
                }
                return Ok(Err(replay.response));
            }
        }

        // Гейт 1: сессия active. FOR SHARE не даёт параллельной паузе
        // закрыть окно между проверкой и INSERT.
        let (status,): (String,) =
            sqlx::query_as("SELECT status FROM session WHERE id = $1 FOR SHARE")
                .bind(session_id)
                .fetch_one(&mut *tx)
                .await?;
        if status != "active" {
            return Err(RequestError::SessionNotActive);
        }

        // Гейт 2: живой грант гостя.
        let grant: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM session_guest WHERE session_id = $1 \
             AND guest_device_id = $2 AND revoked_at IS NULL FOR SHARE",
        )
        .bind(session_id)
        .bind(guest_device_id)
        .fetch_optional(&mut *tx)
        .await?;
        if grant.is_none() {
            return Err(RequestError::NoGrant);
        }

        // INSERT запроса (F4: цена 0 — деньги в F5).
        let row = sqlx::query_as::<_, RequestRow>(
            "INSERT INTO request (id, session_id, guest_device_id, track_cache_id, style) \
             VALUES ($1, $2, $3, $4, $5) \
             RETURNING id, session_id, guest_device_id, track_cache_id, style, price_minor, \
             status, reject_reason, played_at, created_at, version",
        )
        .bind(Uuid::now_v7())
        .bind(session_id)
        .bind(guest_device_id)
        .bind(track_cache_id)
        .bind(style)
        .fetch_one(&mut *tx)
        .await?;

        // Outbox-событие request.created.
        sqlx::query(
            "INSERT INTO outbox (id, aggregate_type, aggregate_id, event_type, payload) \
             VALUES ($1, 'request', $2, 'request.created', $3)",
        )
        .bind(Uuid::now_v7())
        .bind(row.id)
        .bind(serde_json::json!({ "request_id": row.id, "session_id": session_id }))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(Ok(row))
    }

    /// Сохраняет replay-ответ первого POST (вызывается создателем
    /// после успешного TX).
    pub async fn save_idempotency_response(
        &self,
        key: &str,
        guest_device_id: Uuid,
        request_id: Uuid,
        response: &serde_json::Value,
    ) -> Result<(), RequestError> {
        sqlx::query(
            "INSERT INTO idempotency_response (key, guest_device_id, request_id, response) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (key) DO NOTHING",
        )
        .bind(key)
        .bind(guest_device_id)
        .bind(request_id)
        .bind(response)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ---------------------------------------------------------
    // Очередь DJ
    // ---------------------------------------------------------

    /// Очередь активных запросов сессии.
    /// `sort`: "new" — по времени; "hot" — по счётчику той же песни
    /// (canonical-ключ трека), затем по времени.
    pub async fn dj_queue(
        &self,
        session_id: Uuid,
        sort: &str,
    ) -> Result<Vec<QueueItem>, RequestError> {
        let order = match sort {
            "hot" => "same_track_count DESC, r.created_at ASC",
            _ => "r.created_at DESC",
        };
        let rows = sqlx::query_as::<_, QueueItem>(&format!(
            "SELECT r.id, r.session_id, r.guest_device_id, r.track_cache_id, r.style, \
             r.status, r.created_at, t.title, t.artist, t.cover_url, t.preview_url, t.isrc, \
             (SELECT count(*) FROM request r2 \
              JOIN track_cache t2 ON t2.id = r2.track_cache_id \
              WHERE r2.session_id = r.session_id AND t2.canonical_key = t.canonical_key) \
             AS same_track_count \
             FROM request r JOIN track_cache t ON t.id = r.track_cache_id \
             WHERE r.session_id = $1 AND r.status IN ('pending', 'accepted') \
             ORDER BY {order}"
        ))
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Запросы гостя в сессии (для snapshot при reconnect WS).
    pub async fn guest_requests(
        &self,
        session_id: Uuid,
        guest_device_id: Uuid,
    ) -> Result<Vec<QueueItem>, RequestError> {
        let rows = sqlx::query_as::<_, QueueItem>(
            "SELECT r.id, r.session_id, r.guest_device_id, r.track_cache_id, r.style, \
             r.status, r.created_at, t.title, t.artist, t.cover_url, t.preview_url, t.isrc, \
             (SELECT count(*) FROM request r2 \
              JOIN track_cache t2 ON t2.id = r2.track_cache_id \
              WHERE r2.session_id = r.session_id AND t2.canonical_key = t.canonical_key) \
             AS same_track_count \
             FROM request r JOIN track_cache t ON t.id = r.track_cache_id \
             WHERE r.session_id = $1 AND r.guest_device_id = $2 \
             ORDER BY r.created_at DESC",
        )
        .bind(session_id)
        .bind(guest_device_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Название трека из кэша (для WS-карточки).
    pub async fn find_queue_item_title(
        &self,
        track_cache_id: Uuid,
    ) -> Result<Option<String>, RequestError> {
        let title: Option<String> =
            sqlx::query_scalar("SELECT title FROM track_cache WHERE id = $1")
                .bind(track_cache_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(title)
    }

    /// Сколько раз этот трек (canonical) запрошен в сессии (бейдж ×N).
    pub async fn same_track_count(
        &self,
        session_id: Uuid,
        track_cache_id: Uuid,
    ) -> Result<i64, RequestError> {
        let (c,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM request r \
             JOIN track_cache t ON t.id = r.track_cache_id \
             WHERE r.session_id = $1 AND t.canonical_key = \
             (SELECT canonical_key FROM track_cache WHERE id = $2)",
        )
        .bind(session_id)
        .bind(track_cache_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(c)
    }

    pub async fn find_request(&self, id: Uuid) -> Result<Option<RequestRow>, RequestError> {
        let row = sqlx::query_as::<_, RequestRow>(
            "SELECT id, session_id, guest_device_id, track_cache_id, style, price_minor, \
             status, reject_reason, played_at, created_at, version \
             FROM request WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    // ---------------------------------------------------------
    // DJ-действия: no-op семантика + optimistic locking + outbox
    // ---------------------------------------------------------

    /// Применяет действие DJ. Возвращает (новое состояние, был ли это
    /// no-op). No-op — успешный повтор, НЕ ошибка (техкарта Часть III).
    pub async fn apply_action(
        &self,
        request_id: Uuid,
        action: RequestAction,
        reject_reason: Option<&str>,
    ) -> Result<(RequestRow, bool), RequestError> {
        let row = self
            .find_request(request_id)
            .await?
            .ok_or(RequestError::Db(sqlx::Error::RowNotFound))?;
        let current = row
            .request_status()
            .ok_or(RequestError::Db(sqlx::Error::Decode(
                "unknown request status".into(),
            )))?;

        let next = match current.apply(action) {
            Ok(n) => n,
            Err(_) => {
                return Err(RequestError::Db(sqlx::Error::Decode(
                    format!("invalid transition {current:?} + {action:?}").into(),
                )))
            }
        };
        let is_noop = next == current;

        let status_str_ = status_str(next);
        let played_at = if matches!(action, RequestAction::Play) && !is_noop {
            Some(Utc::now())
        } else {
            row.played_at
        };

        // Optimistic locking: UPDATE только при совпадении version —
        // гонка accept/played на двух устройствах: второй увидит
        // no-op-результат по свежему статусу, не перезапишет.
        let updated = sqlx::query_as::<_, RequestRow>(
            "UPDATE request SET status = $2, reject_reason = $3, played_at = $4, \
             version = version + 1 \
             WHERE id = $1 AND version = $5 \
             RETURNING id, session_id, guest_device_id, track_cache_id, style, price_minor, \
             status, reject_reason, played_at, created_at, version",
        )
        .bind(request_id)
        .bind(status_str_)
        .bind(reject_reason)
        .bind(played_at)
        .bind(row.version)
        .fetch_optional(&self.pool)
        .await?;

        let new_row = match updated {
            Some(r) => r,
            None => {
                // Гонка: перечитываем — кто-то уже применил действие.
                let fresh = self
                    .find_request(request_id)
                    .await?
                    .ok_or(RequestError::Db(sqlx::Error::RowNotFound))?;
                return Ok((fresh, true)); // трактуем как no-op
            }
        };

        // Outbox-событие (кроме no-op).
        if !is_noop {
            let event = match action {
                RequestAction::Accept => "request.accepted",
                RequestAction::Reject => "request.rejected",
                RequestAction::Play => "request.played",
                RequestAction::Expire => "request.expired",
            };
            sqlx::query(
                "INSERT INTO outbox (id, aggregate_type, aggregate_id, event_type, payload) \
                 VALUES ($1, 'request', $2, $3, $4)",
            )
            .bind(Uuid::now_v7())
            .bind(request_id)
            .bind(event)
            .bind(serde_json::json!({
                "request_id": request_id,
                "session_id": new_row.session_id,
                "guest_device_id": new_row.guest_device_id,
                "status": status_str_,
            }))
            .execute(&self.pool)
            .await?;
        }

        Ok((new_row, is_noop))
    }
}

fn status_str(s: RequestStatus) -> &'static str {
    match s {
        RequestStatus::Pending => "pending",
        RequestStatus::Accepted => "accepted",
        RequestStatus::Rejected => "rejected",
        RequestStatus::Played => "played",
        RequestStatus::Expired => "expired",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_strings_roundtrip() {
        assert_eq!(status_str(RequestStatus::Pending), "pending");
        assert_eq!(status_str(RequestStatus::Accepted), "accepted");
        assert_eq!(status_str(RequestStatus::Played), "played");
    }
}
