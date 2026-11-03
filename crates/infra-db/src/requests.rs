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

/// Исход апгрейда гостя (POST /v1/guest/upgrade).
#[derive(Debug)]
pub enum UpgradeOutcome {
    /// E-mail привязан к гостю. `already_upgraded=true` — повторный вызов
    /// с тем же нормализованным e-mail (идемпотентность).
    Bound {
        email: String,
        already_upgraded: bool,
    },
    /// E-mail уже привязан к другому гостю → 409 EMAIL_TAKEN.
    EmailTaken,
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

/// Строка лога смены DJ (`GET /v1/dj/history`).
///
/// Лог строго по проигранным заявкам: `played` — единственный статус, у
/// которого есть время события (`played_at`). Отказ живёт без метки времени
/// (`request` не хранит `rejected_at`), поэтому в этот лог не попадает:
/// показывать «сыгранное» и «отклонённое» в одном списке под одной подписью
/// значило бы выдавать одно за другое.
#[derive(Debug, sqlx::FromRow)]
pub struct DjHistoryItem {
    pub id: Uuid,
    pub title: String,
    pub artist: String,
    pub style: Option<String>,
    pub played_at: Option<DateTime<Utc>>,
    /// Ключ сортировки страницы: `COALESCE(played_at, created_at)`.
    ///
    /// Отдаётся отдельным полем, чтобы курсор следующей страницы кодировал
    /// ровно тот ключ, по которому шла сортировка: иначе заявка без
    /// `played_at` даёт курсор, не совпадающий с `ORDER BY`, и следующая
    /// страница повторяет или теряет строки.
    pub sort_key: DateTime<Utc>,
    /// Когда заявку приняли. Колонки `accepted_at` в `request` нет, а миграция
    /// вне рамок этой задачи, поэтому время берётся из единственного
    /// фактического источника — outbox-события `request.accepted`, которое
    /// пишется тем же действием DJ, что и смена статуса. События нет →
    /// `None` (в ответе `null`), а не выдуманное время.
    pub accepted_at: Option<DateTime<Utc>>,
    /// Сколько раз эту же песню (canonical-ключ) заказали в сессии.
    pub same_track_count: i64,
    pub cover_url: Option<String>,
}

/// Счётчики текущей смены DJ (`GET /v1/dj/stats/session`).
///
/// Отличие от `/v1/dj/stats/today`: окно здесь — смена (`session.id`), а не
/// последние 24 часа, поэтому числа не «плывут» через смену и совпадают с
/// тем, что DJ видит в логе.
#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct DjSessionTotals {
    pub requests: i64,
    /// Заявки, которые DJ принял в очередь за смену: `accepted` + `played`.
    /// Сыгранная заявка была принята раньше, поэтому `WHERE status='accepted'`
    /// показал бы ноль к концу смены — а подпись в консоли «Requests you moved
    /// into the queue» обещает именно «сколько принял».
    pub accepted: i64,
    pub played: i64,
    pub rejected: i64,
    /// DJ-доля по этой смене (`split.payee_type = 'dj'`), minor units.
    pub earned_minor: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct ReplayRow {
    /// Владелец ключа (guest_device_id) — для проверки «чужой гость».
    guest_device_id: Uuid,
    response: serde_json::Value,
}

/// Карточка одной заявки гостя для экрана «моя заявка»
/// (GET /v1/my/requests/:id). Содержит данные трека и временные метки;
/// позиция в очереди и счётчики считаются отдельными методами.
#[derive(Debug, sqlx::FromRow)]
pub struct RequestDetail {
    pub id: Uuid,
    pub session_id: Uuid,
    pub guest_device_id: Uuid,
    pub track_cache_id: Uuid,
    pub status: String,
    pub reject_reason: Option<String>,
    pub played_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub title: String,
    pub artist: String,
    pub cover_url: Option<String>,
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

    /// Создаёт запрос атомарно с идемпотентным ответом и outbox-событием.
    ///
    /// Все четыре записи — гейты (`FOR SHARE`), `INSERT request`,
    /// `INSERT idempotency_response` и `INSERT outbox` — выполняются в
    /// **одной** транзакции и коммитятся вместе. Идемпотентный ответ
    /// сохраняется до коммита: `UNIQUE(key)` — единственный арбитр при
    /// гонке двух параллельных POST с одним `Idempotency-Key`.
    ///
    /// Поведение при дубле ключа:
    ///   * тот же гость — `Ok(Err(replay))` (сохранённый ответ первого
    ///     POST); вставка заявки откатывается, outbox не дублируется;
    ///   * чужой гость — `Err(IdempotencyConflict)` (→ 409).
    ///
    /// При любой ошибке (нет гранта, сессия не active, сбой БД) ни одна
    /// из записей не остаётся «полузаписанной»: транзакция откатывается.
    pub async fn create_request(
        &self,
        session_id: Uuid,
        guest_device_id: Uuid,
        track_cache_id: Uuid,
        style: Option<&str>,
        idempotency_key: &str,
    ) -> Result<Result<RequestRow, serde_json::Value>, RequestError> {
        let mut tx = self.pool.begin().await?;

        // Быстрый путь replay: ключ уже зафиксирован ранее (коммитнут до
        // старта этой транзакции). Возвращает сохранённый ответ без
        // захвата блокировок сессии — корректно даже если сессия уже
        // закрыта/на паузе (идемпотентность важнее текущего состояния).
        if let Some(replay) = sqlx::query_as::<_, ReplayRow>(
            "SELECT guest_device_id, response FROM idempotency_response WHERE key = $1",
        )
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            if replay.guest_device_id != guest_device_id {
                return Err(RequestError::IdempotencyConflict);
            }
            return Ok(Err(replay.response));
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

        // INSERT заявки (F4: цена 0 — деньги в F5).
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

        // Идемпотентный ответ формируем из строки и фиксируем ВНУТРИ
        // транзакции. ON CONFLICT (key) DO NOTHING: при гонке двух POST
        // с одним ключом второй TX блокируется до коммита первого —
        //   * первый закоммитил ключ → второй получает 0 строк, откатывает
        //     свою заявку и возвращает сохранённый ответ (replay);
        //   * первый откатился → второй вставляет ключ и создаёт заявку.
        // Заявка уже вставлена (FK request_id), но будет откатана вместе
        // с транзакцией при конфликте — никакого «полузаписанного» состояния.
        let response_body = serde_json::json!({
            "request_id": row.id,
            "status": row.status,
            "session_id": row.session_id,
            "queue_position": null,
        });
        let inserted = sqlx::query(
            "INSERT INTO idempotency_response (key, guest_device_id, request_id, response) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (key) DO NOTHING",
        )
        .bind(idempotency_key)
        .bind(guest_device_id)
        .bind(row.id)
        .bind(&response_body)
        .execute(&mut *tx)
        .await?;

        if inserted.rows_affected() == 0 {
            // Конфликт ключа: параллельный POST (или строка, коммитнутая
            // уже после нашего быстрого пути) владеет ключом. Откатываем
            // нашу вставку заявки, затем читаем зафиксированный ответ
            // владельца из пула (свежий READ COMMITTED-снапшот видит
            // коммитнутую строку).
            tx.rollback().await?;
            let owner = sqlx::query_as::<_, ReplayRow>(
                "SELECT guest_device_id, response FROM idempotency_response WHERE key = $1",
            )
            .bind(idempotency_key)
            .fetch_optional(&self.pool)
            .await?;
            return match owner {
                Some(r) if r.guest_device_id == guest_device_id => Ok(Err(r.response)),
                Some(_) => Err(RequestError::IdempotencyConflict),
                // Невозможно при отсутствии DELETE idempotency_response:
                // ON CONFLICT DO NOTHING = 0 строк означает, что
                // коммитнутая строка-владелец существует. Если будущий
                // TTL-воркер удалит её в окне между конфликтом и чтением —
                // вернём 500; повторный POST создаст заявку заново
                // (ключ снова свободен).
                None => Err(RequestError::Db(sqlx::Error::RowNotFound)),
            };
        }

        // Outbox-событие request.created — в той же транзакции, что и
        // заявка + идемпотентный ответ: либо все три коммитятся, либо ни
        // один (атомарность transactional outbox, ADR-002). При replay
        // (конфликт ключа) сюда не доходим — outbox не дублируется.
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

    /// Лог проигранных заявок смены (keyset-страница).
    ///
    /// Ключ сортировки — `(COALESCE(played_at, created_at), id)` по убыванию.
    /// `COALESCE` нужен как страховка: `played` без `played_at` теоретически
    /// возможен (строка могла прийти из бэкфилла), и без него такая заявка
    /// выпала бы из лога навсегда — то есть «пропала», а не «не сыграла».
    /// `played_at` при этом возвращается как есть: `null` в ответе честно
    /// означает «время неизвестно».
    ///
    /// `after` — последняя строка предыдущей страницы (курсор); `limit`
    /// ограничивает размер страницы (вызывающий просит `limit + 1`, чтобы
    /// понять, есть ли следующая).
    pub async fn dj_history(
        &self,
        session_id: Uuid,
        after: Option<(DateTime<Utc>, Uuid)>,
        limit: i64,
    ) -> Result<Vec<DjHistoryItem>, RequestError> {
        let (after_at, after_id) = match after {
            Some((at, id)) => (Some(at), Some(id)),
            None => (None, None),
        };
        let rows = sqlx::query_as::<_, DjHistoryItem>(
            "SELECT r.id, t.title, t.artist, r.style, r.played_at, \
             COALESCE(r.played_at, r.created_at) AS sort_key, \
             (SELECT min(o.created_at) FROM outbox o \
               WHERE o.aggregate_type = 'request' AND o.aggregate_id = r.id \
                 AND o.event_type = 'request.accepted') AS accepted_at, \
             (SELECT count(*) FROM request r2 \
               JOIN track_cache t2 ON t2.id = r2.track_cache_id \
               WHERE r2.session_id = r.session_id AND t2.canonical_key = t.canonical_key) \
              AS same_track_count, \
             t.cover_url \
             FROM request r JOIN track_cache t ON t.id = r.track_cache_id \
             WHERE r.session_id = $1 AND r.status = 'played' \
               AND ($2::timestamptz IS NULL \
                    OR (COALESCE(r.played_at, r.created_at), r.id) < ($2::timestamptz, $3::uuid)) \
             ORDER BY COALESCE(r.played_at, r.created_at) DESC, r.id DESC \
             LIMIT $4",
        )
        .bind(session_id)
        .bind(after_at)
        .bind(after_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Счётчики смены целиком: заявки, решения, DJ-доля.
    ///
    /// `dj_id` — из JWT-контекста (не из запроса), поэтому чужие сплиты в
    /// сумму не попадут даже если клиент подсунет чужой `session_id`.
    pub async fn dj_session_totals(
        &self,
        session_id: Uuid,
        dj_id: Uuid,
    ) -> Result<DjSessionTotals, RequestError> {
        let totals = sqlx::query_as::<_, DjSessionTotals>(
            "SELECT \
             (SELECT count(*) FROM request WHERE session_id = $1) AS requests, \
             (SELECT count(*) FROM request WHERE session_id = $1 \
                AND status IN ('accepted', 'played')) AS accepted, \
             (SELECT count(*) FROM request WHERE session_id = $1 AND status = 'played') \
               AS played, \
             (SELECT count(*) FROM request WHERE session_id = $1 AND status = 'rejected') \
               AS rejected, \
             (SELECT COALESCE(sum(sp.amount_minor - sp.reversed_minor), 0)::bigint \
                FROM split sp \
                JOIN payment p ON p.id = sp.payment_id \
                JOIN request r ON r.id = p.request_id \
               WHERE r.session_id = $1 AND sp.payee_type = 'dj' AND sp.payee_id = $2) \
               AS earned_minor",
        )
        .bind(session_id)
        .bind(dj_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(totals)
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
    // Экран «моя заявка»: GET /v1/my/requests/:id
    // ---------------------------------------------------------

    /// Находит заявку гостя в конкретной сессии. Тройной фильтр
    /// (request_id + session_id + guest_device_id) гарантирует:
    /// чужая заявка, заявка из другой сессии или несуществующий id —
    /// всё равно вернёт `None` → 404 (без раскрытия существования).
    pub async fn find_guest_request_detail(
        &self,
        session_id: Uuid,
        guest_device_id: Uuid,
        request_id: Uuid,
    ) -> Result<Option<RequestDetail>, RequestError> {
        let row = sqlx::query_as::<_, RequestDetail>(
            "SELECT r.id, r.session_id, r.guest_device_id, r.track_cache_id, r.status, \
             r.reject_reason, r.played_at, r.created_at, t.title, t.artist, t.cover_url \
             FROM request r JOIN track_cache t ON t.id = r.track_cache_id \
             WHERE r.id = $1 AND r.session_id = $2 AND r.guest_device_id = $3",
        )
        .bind(request_id)
        .bind(session_id)
        .bind(guest_device_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Позиция заявки в активной очереди сессии (1 = первая). Та же
    /// сортировка поля `created_at`, что и в `dj_queue(sort="new")`,
    /// но FIFO-порядок (старейшая = 1) — «место в очереди» = порядок
    /// обработки. Вернёт `None`, если заявка не в активной очереди
    /// (pending/accepted).
    pub async fn active_queue_position(
        &self,
        session_id: Uuid,
        request_id: Uuid,
    ) -> Result<Option<i64>, RequestError> {
        let pos: Option<i64> = sqlx::query_scalar(
            "SELECT pos FROM ( \
               SELECT r.id, ROW_NUMBER() OVER (ORDER BY r.created_at ASC) AS pos \
               FROM request r \
               WHERE r.session_id = $1 AND r.status IN ('pending', 'accepted') \
             ) q WHERE q.id = $2",
        )
        .bind(session_id)
        .bind(request_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(pos)
    }

    /// Всего активных заявок в сессии (pending + accepted) — тот же
    /// фильтр статусов, что и в очереди DJ.
    pub async fn active_queue_size(&self, session_id: Uuid) -> Result<i64, RequestError> {
        let (c,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM request \
             WHERE session_id = $1 AND status IN ('pending', 'accepted')",
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(c)
    }

    // ---------------------------------------------------------
    // Апгрейд гостя: POST /v1/guest/upgrade { email }
    // ---------------------------------------------------------

    /// Привязывает e-mail к гостевому профилю. Идемпотентность:
    /// повторный вызов с тем же нормализованным e-mail →
    /// `Bound { already_upgraded: true }` без дублей. E-mail, занятый
    /// другим гостем → `EmailTaken` (409). Смена e-mail на свободный
    /// разрешена и атомарна (UPDATE с_guard `NOT EXISTS`).
    ///
    /// `email_normalized` и `email_display` — результат нормализации
    /// (trim + lowercase), которую вызывающий выполняет ДО вызова.
    pub async fn upgrade_guest_email(
        &self,
        guest_device_id: Uuid,
        email_normalized: &str,
        email_display: &str,
    ) -> Result<UpgradeOutcome, RequestError> {
        let mut tx = self.pool.begin().await?;

        // Текущий account гостя (FOR UPDATE — фиксируем строку устройства).
        let current: Option<(Option<Uuid>,)> =
            sqlx::query_as("SELECT account_id FROM guest_device WHERE id = $1 FOR UPDATE")
                .bind(guest_device_id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((current_account,)) = current else {
            // Устройства нет — вызывающий должен был отвергнуть (401).
            return Err(RequestError::Db(sqlx::Error::RowNotFound));
        };

        if let Some(acc) = current_account {
            // Уже привязан аккаунт — сравниваем по нормализованному e-mail.
            let (existing,): (String,) =
                sqlx::query_as("SELECT email_normalized FROM guest_account WHERE id = $1")
                    .bind(acc)
                    .fetch_one(&mut *tx)
                    .await?;
            if existing == email_normalized {
                tx.rollback().await?;
                return Ok(UpgradeOutcome::Bound {
                    email: email_normalized.to_string(),
                    already_upgraded: true,
                });
            }
            // Смена e-mail: атомарный UPDATE с_guard против занятия чужого.
            let updated = sqlx::query(
                "UPDATE guest_account \
                 SET email_normalized = $1, email_display = $2, verified = FALSE \
                 WHERE id = $3 \
                 AND NOT EXISTS ( \
                   SELECT 1 FROM guest_account \
                   WHERE email_normalized = $1 AND id <> $3 \
                 )",
            )
            .bind(email_normalized)
            .bind(email_display)
            .bind(acc)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() == 0 {
                // Новый e-mail занят другим аккаунтом.
                tx.rollback().await?;
                return Ok(UpgradeOutcome::EmailTaken);
            }
            tx.commit().await?;
            return Ok(UpgradeOutcome::Bound {
                email: email_normalized.to_string(),
                already_upgraded: false,
            });
        }

        // Нет аккаунта — создаём. ON CONFLICT ловит гонку и уже занятый e-mail.
        let new_id = Uuid::now_v7();
        let inserted = sqlx::query(
            "INSERT INTO guest_account (id, email_normalized, email_display, verified) \
             VALUES ($1, $2, $3, FALSE) \
             ON CONFLICT (email_normalized) DO NOTHING",
        )
        .bind(new_id)
        .bind(email_normalized)
        .bind(email_display)
        .execute(&mut *tx)
        .await?;
        if inserted.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(UpgradeOutcome::EmailTaken);
        }
        sqlx::query("UPDATE guest_device SET account_id = $1 WHERE id = $2")
            .bind(new_id)
            .bind(guest_device_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(UpgradeOutcome::Bound {
            email: email_normalized.to_string(),
            already_upgraded: false,
        })
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
    use crate::sessions::SessionRepo;
    use crate::IdentityRepo;

    #[test]
    fn status_strings_roundtrip() {
        assert_eq!(status_str(RequestStatus::Pending), "pending");
        assert_eq!(status_str(RequestStatus::Accepted), "accepted");
        assert_eq!(status_str(RequestStatus::Played), "played");
    }

    /// Подключение к общей тестовой Postgres (миграции применены).
    /// При недоступной БД тест пропускается.
    async fn test_pool() -> Option<PgPool> {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://dropit:dropit_local@localhost:5432/dropit".into());
        crate::connect(&url).await.ok()
    }

    /// Фикстуры: клуб, DJ, active-сессия, грант гостя, трек в кэше.
    /// Все идентификаторы — свежие UUIDv7, поэтому тест изолирован от
    /// данных других прогонов (UNIQUE-колонки тоже не конфликтуют).
    async fn fixtures(pool: &PgPool) -> (Uuid, Uuid, Uuid, Uuid, Uuid) {
        let sessions = SessionRepo::new(pool.clone());
        let identity = IdentityRepo::new(pool.clone());
        let tag = Uuid::now_v7();
        let club_id = Uuid::now_v7();
        let dj_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO club (id, name, slug, email, password_hash) \
             VALUES ($1, 'W5E', $2, $3, 'x')",
        )
        .bind(club_id)
        .bind(format!("w5e-{}", tag))
        .bind(format!("w5e-{}@club.test", tag))
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO dj (id, club_id, email, password_hash, display_name) \
             VALUES ($1, $2, $3, 'x', 'DJ W5E')",
        )
        .bind(dj_id)
        .bind(club_id)
        .bind(format!("w5e-{}@dj.test", tag))
        .execute(pool)
        .await
        .unwrap();
        let session = sessions
            .create_session(club_id, Some(dj_id), None, "Europe/Madrid")
            .await
            .unwrap();
        sessions
            .apply_transition(session.id, dropit_domain::SessionTransition::Open)
            .await
            .unwrap();
        let guest_id = Uuid::now_v7();
        identity
            .create_guest_device(guest_id, "ios", None)
            .await
            .unwrap();
        sessions
            .grant_session_guest(session.id, guest_id)
            .await
            .unwrap();
        let canonical = format!("itunes:w5e-{}", tag);
        let (track_id,): (Uuid,) = sqlx::query_as(
            "INSERT INTO track_cache (id, source, source_id, canonical_key, title, artist) \
             VALUES ($1, 'itunes', $2, $3, 'Vivir Mi Vida', 'Marc Anthony') RETURNING id",
        )
        .bind(Uuid::now_v7())
        .bind(format!("w5e-{}", tag))
        .bind(canonical)
        .fetch_one(pool)
        .await
        .unwrap();
        (session.id, guest_id, track_id, club_id, dj_id)
    }

    /// Два ПАРАЛЛЕЛЬНЫХ `create_request` с одним Idempotency-Key: ровно
    /// одна заявка, один outbox-эвент, оба вызова возвращают один
    /// request_id (один — создатель, второй — replay). Регрессия на
    /// дефект, когда идемпотентный ответ писался после коммита.
    #[tokio::test]
    async fn parallel_create_request_one_key_one_row() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("DATABASE_URL недоступен — W5-E repo-тест пропущен");
                return;
            }
        };
        let (session_id, guest_id, track_id, _club_id, _dj_id) = fixtures(&pool).await;
        let repo = RequestRepo::new(pool.clone());
        let key = format!("w5e-par-{}", Uuid::now_v7());

        // tokio::join! — два конкурентных вызова против одного ключа.
        let (a, b) = tokio::join!(
            repo.create_request(session_id, guest_id, track_id, None, &key),
            repo.create_request(session_id, guest_id, track_id, None, &key),
        );
        let a = a.expect("a no db err");
        let b = b.expect("b no db err");

        // Ровно один создатель (Ok(Ok)) и ровно один replay (Ok(Err)),
        // причём replay ссылается на тот же request_id.
        let (created_id, replay_id) = match (&a, &b) {
            (Ok(row), Err(replay)) | (Err(replay), Ok(row)) => {
                let replay_id = replay["request_id"].as_str().expect("replay request_id");
                (row.id, replay_id.to_string())
            }
            _ => panic!("expected one creator + one replay, got {a:?} / {b:?}"),
        };
        assert_eq!(
            replay_id,
            created_id.to_string(),
            "replay must reference the creator's request_id"
        );

        // В БД ровно одна заявка этого гостя.
        let (cnt,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM request WHERE guest_device_id = $1")
                .bind(guest_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(cnt, 1, "parallel create must not duplicate the request");

        // Один идемпотентный ответ на ключ.
        let (idem_cnt,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM idempotency_response WHERE key = $1")
                .bind(&key)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(idem_cnt, 1, "exactly one idempotency_response per key");

        // Ровно одно outbox-событие request.created на заявку.
        let (ob_cnt,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM outbox WHERE aggregate_type = 'request' \
             AND aggregate_id = $1 AND event_type = 'request.created'",
        )
        .bind(created_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(ob_cnt, 1, "exactly one request.created outbox event");

        // Последовательный повтор → replay того же id, без дубля заявки
        // и без дубля outbox.
        let c = repo
            .create_request(session_id, guest_id, track_id, None, &key)
            .await
            .expect("seq no db err");
        match c {
            Err(replay) => {
                assert_eq!(
                    replay["request_id"].as_str().unwrap(),
                    created_id.to_string(),
                    "sequential replay returns same request_id"
                );
            }
            other => panic!("sequential repeat must replay, got {other:?}"),
        }
        let (cnt2,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM request WHERE guest_device_id = $1")
                .bind(guest_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(cnt2, 1, "sequential replay must not duplicate request");
        let (ob_cnt2,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM outbox WHERE aggregate_type = 'request' \
             AND aggregate_id = $1 AND event_type = 'request.created'",
        )
        .bind(created_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(ob_cnt2, 1, "sequential replay must not duplicate outbox");
    }

    /// Чужой гость с тем же Idempotency-Key → IdempotencyConflict (409),
    /// а не перезапись ответа.
    #[tokio::test]
    async fn foreign_guest_same_key_conflicts() {
        let pool = match test_pool().await {
            Some(p) => p,
            None => {
                eprintln!("DATABASE_URL недоступен — W5-E conflict-тест пропущен");
                return;
            }
        };
        let (session_id, guest_a, track_id, _, _) = fixtures(&pool).await;
        let identity = IdentityRepo::new(pool.clone());
        let guest_b = Uuid::now_v7();
        identity
            .create_guest_device(guest_b, "android", None)
            .await
            .unwrap();
        // Гостю B нужен грант в той же (или любой active) сессии.
        SessionRepo::new(pool.clone())
            .grant_session_guest(session_id, guest_b)
            .await
            .unwrap();
        let repo = RequestRepo::new(pool.clone());
        let key = format!("w5e-foreign-{}", Uuid::now_v7());

        // Гость A создаёт заявку и фиксирует ключ.
        let a = repo
            .create_request(session_id, guest_a, track_id, None, &key)
            .await
            .unwrap()
            .expect("a created");
        // Гость B с тем же ключом → конфликт (чужой владелец).
        let b = repo
            .create_request(session_id, guest_b, track_id, None, &key)
            .await;
        assert!(
            matches!(b, Err(RequestError::IdempotencyConflict)),
            "foreign guest same key must conflict, got {b:?}"
        );
        // Заявка гостя A на месте; у B нет заявки.
        let (cnt_a,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM request WHERE guest_device_id = $1")
                .bind(guest_a)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(cnt_a, 1);
        let (cnt_b,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM request WHERE guest_device_id = $1")
                .bind(guest_b)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(cnt_b, 0, "conflicting guest must have no request");
        // Идемпотентный ответ остаётся за владельцем A.
        let (owner,): (Uuid,) =
            sqlx::query_as("SELECT guest_device_id FROM idempotency_response WHERE key = $1")
                .bind(&key)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(owner, guest_a);
        let _ = a;
    }
}
