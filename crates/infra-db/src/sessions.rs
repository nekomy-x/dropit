//! Репозиторий сессий, QR-токенов и join-грантов (F2).
//!
//! Инварианты (техкарта Часть II):
//!   * у сессии максимум один активный QR-токен (partial unique index);
//!   * в БД только sha256-хеш QR-токена;
//!   * join-грант UNIQUE(session_id, guest_device_id): повторный
//!     join = re-grant (revoked_at → NULL), не дубль;
//!   * переход статусов только через стейт-машину domain::session
//!     (недопустимый переход → ошибка, не молчаливое изменение).

use chrono::{DateTime, Utc};
use dropit_domain::{SessionStatus, SessionTransition};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("database error")]
    Db(#[from] sqlx::Error),
    #[error("not found")]
    NotFound,
    #[error("invalid transition")]
    InvalidTransition,
    #[error("qr token invalid or inactive")]
    QrInvalid,
}

#[derive(Debug, sqlx::FromRow)]
pub struct SessionRow {
    pub id: Uuid,
    pub club_id: Uuid,
    pub dj_id: Option<Uuid>,
    pub status: String,
    pub pricing_policy_id: Option<Uuid>,
    pub opened_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub timezone: String,
    pub created_at: DateTime<Utc>,
}

impl SessionRow {
    /// Статус через типизированную стейт-машину.
    pub fn session_status(&self) -> Option<SessionStatus> {
        parse_session_status(&self.status)
    }
}

/// Разбирает строковый статус сессии (из БД) в типизированную
/// стейт-машину `SessionStatus`. Единственное место маппинга
/// строка→enum — используется и `SessionRow::session_status`, и
/// хендлерами, которым нужна доменная логика (например,
/// `accepts_requests` для `requests_open`).
pub fn parse_session_status(s: &str) -> Option<SessionStatus> {
    match s {
        "created" => Some(SessionStatus::Created),
        "active" => Some(SessionStatus::Active),
        "paused_acceptance" => Some(SessionStatus::PausedAcceptance),
        "closed" => Some(SessionStatus::Closed),
        _ => None,
    }
}

/// Информация о сессии для гостя (ответ /sessions/join).
#[derive(Debug, sqlx::FromRow)]
pub struct SessionInfo {
    pub session_id: Uuid,
    pub club_name: String,
    pub dj_display_name: Option<String>,
    pub status: String,
    pub timezone: String,
    pub price_minor: Option<i64>,
}

/// Текущая открытая сессия гостя для `GET /v1/sessions/current`.
///
/// Выбирается последняя открытая (`active`/`paused_acceptance`)
/// сессия клуба, где у гостя есть действующий join-грант
/// (`session_guest.revoked_at IS NULL`). `opened_at` гарантированно
/// не NULL — фильтр отсекает `created`/`closed`. `grant_expires_at` —
/// `expires_at` активного QR-токена сессии (плановое авто-закрытие);
/// NULL, если все QR деактивированы (тогда хендер считает следующий
/// 07:00 в зоне клуба). `club_city` берётся из `club.address`
/// (отдельной колонки city в схеме нет — миграции чужого пакета).
#[derive(Debug, sqlx::FromRow)]
pub struct CurrentGuestSession {
    pub id: Uuid,
    pub club_id: Uuid,
    pub club_name: String,
    pub club_city: String,
    pub status: String,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub timezone: String,
    pub dj_id: Option<Uuid>,
    pub dj_display_name: Option<String>,
    pub grant_expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct QrTokenRow {
    pub id: Uuid,
    pub session_id: Uuid,
    pub token_hash: String,
    pub active: bool,
    pub generated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct SessionGuestRow {
    pub id: Uuid,
    pub session_id: Uuid,
    pub guest_device_id: Uuid,
    pub joined_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Clone)]
pub struct SessionRepo {
    pool: PgPool,
}

impl SessionRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    // ---------------------------------------------------------
    // Сессии
    // ---------------------------------------------------------

    /// Создаёт сессию (status='created'). Таймзона — IANA-зона клуба.
    pub async fn create_session(
        &self,
        club_id: Uuid,
        dj_id: Option<Uuid>,
        pricing_policy_id: Option<Uuid>,
        timezone: &str,
    ) -> Result<SessionRow, SessionError> {
        let id = Uuid::now_v7();
        let row = sqlx::query_as::<_, SessionRow>(
            "INSERT INTO session (id, club_id, dj_id, pricing_policy_id, timezone) \
             VALUES ($1, $2, $3, $4, $5) \
             RETURNING id, club_id, dj_id, status, pricing_policy_id, opened_at, \
             closed_at, timezone, created_at",
        )
        .bind(id)
        .bind(club_id)
        .bind(dj_id)
        .bind(pricing_policy_id)
        .bind(timezone)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn find_session(&self, id: Uuid) -> Result<Option<SessionRow>, SessionError> {
        let row = sqlx::query_as::<_, SessionRow>(
            "SELECT id, club_id, dj_id, status, pricing_policy_id, opened_at, closed_at, \
             timezone, created_at FROM session WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Применяет переход стейт-машины атомарно:
    /// UPDATE только если текущий статус совпадает с ожидаемым —
    /// гонки двух CLUB/библиотека-запросов дают InvalidTransition.
    pub async fn apply_transition(
        &self,
        session_id: Uuid,
        transition: SessionTransition,
    ) -> Result<SessionRow, SessionError> {
        use SessionTransition as T;
        let row = self
            .find_session(session_id)
            .await?
            .ok_or(SessionError::NotFound)?;
        let current = row
            .session_status()
            .ok_or(SessionError::Db(sqlx::Error::Decode(
                "unknown session status".into(),
            )))?;
        let next = current
            .apply(transition)
            .map_err(|_| SessionError::InvalidTransition)?;

        let (opened, closed): (Option<DateTime<Utc>>, Option<DateTime<Utc>>) = match transition {
            T::Open => (Some(Utc::now()), None),
            T::Close => (row.opened_at, Some(Utc::now())),
            _ => (row.opened_at, row.closed_at),
        };
        let updated = sqlx::query_as::<_, SessionRow>(
            "UPDATE session SET status = $2, opened_at = $3, closed_at = $4 \
             WHERE id = $1 AND status = $5 \
             RETURNING id, club_id, dj_id, status, pricing_policy_id, opened_at, \
             closed_at, timezone, created_at",
        )
        .bind(session_id)
        .bind(status_str(next))
        .bind(opened)
        .bind(closed)
        .bind(row.status.clone())
        .fetch_optional(&self.pool)
        .await?;
        updated.ok_or(SessionError::InvalidTransition)
    }

    /// Сессии, открытые (status <> 'closed') — для авто-закрытия джобой.
    pub async fn open_sessions(&self) -> Result<Vec<SessionRow>, SessionError> {
        let rows = sqlx::query_as::<_, SessionRow>(
            "SELECT id, club_id, dj_id, status, pricing_policy_id, opened_at, closed_at, \
             timezone, created_at FROM session WHERE status <> 'closed'",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    // ---------------------------------------------------------
    // QR-токены
    // ---------------------------------------------------------

    /// Выдаёт новый QR-токен сессии (ротация: старые деактивируются).
    /// `expires_at` = плановое авто-закрытие (QR истекает вместе с сессией).
    pub async fn issue_qr_token(
        &self,
        session_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<QrTokenRow, SessionError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE qr_token SET active = FALSE WHERE session_id = $1 AND active")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;
        let row = sqlx::query_as::<_, QrTokenRow>(
            "INSERT INTO qr_token (id, session_id, token_hash, expires_at) \
             VALUES ($1, $2, $3, $4) \
             RETURNING id, session_id, token_hash, active, generated_at, expires_at",
        )
        .bind(Uuid::now_v7())
        .bind(session_id)
        .bind(token_hash)
        .bind(expires_at)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    /// Ищет АКТИВНЫЙ токен по хешу. Неактивный/неизвестный → QrInvalid.
    pub async fn find_active_qr(&self, token_hash: &str) -> Result<QrTokenRow, SessionError> {
        let row = sqlx::query_as::<_, QrTokenRow>(
            "SELECT id, session_id, token_hash, active, generated_at, expires_at \
             FROM qr_token WHERE token_hash = $1 AND active",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            Some(r) if r.expires_at > Utc::now() => Ok(r),
            Some(_) => Err(SessionError::QrInvalid), // истёк = сессию пора закрывать
            None => Err(SessionError::QrInvalid),
        }
    }

    /// Возвращает активный (не истёкший) QR-токен сессии, если он есть.
    ///
    /// Истёкшие строки `active=TRUE` отбрасываются: функционально токен
    /// уже не рабочий (плановое закрытие сессии наступило). Используется
    /// хендлером `qr/activate` для идемпотентности: если активный токен
    /// есть — повторный activate НЕ выпускает второй токен.
    pub async fn active_qr_for_session(
        &self,
        session_id: Uuid,
    ) -> Result<Option<QrTokenRow>, SessionError> {
        let row = sqlx::query_as::<_, QrTokenRow>(
            "SELECT id, session_id, token_hash, active, generated_at, expires_at \
             FROM qr_token WHERE session_id = $1 AND active AND expires_at > now() \
             ORDER BY generated_at DESC LIMIT 1",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Деактивирует все активные QR-токены сессии (обычно один —
    /// инвариант `idx_qr_token_one_active`). Идемпотентно: если активных
    /// токенов нет, запрос просто затрагивает 0 строк и возвращает успех.
    ///
    /// **Гранты гостей (`session_guest`) НЕ отзываются.** Гость, уже
    /// вошедший по QR до деактивации, сохраняет доступ до планового
    /// закрытия сессии (`GET /v1/sessions/current` остаётся 200). Новый
    /// вход по тому же QR-токену блокируется в `find_active_qr`
    /// (`active=FALSE` → `QrInvalid` → 404): это и есть семантика
    /// «выключить приём по QR, не выгонять уже вошедших».
    pub async fn deactivate_qr(&self, session_id: Uuid) -> Result<(), SessionError> {
        sqlx::query("UPDATE qr_token SET active = FALSE WHERE session_id = $1 AND active")
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ---------------------------------------------------------
    // Join-гранты
    // ---------------------------------------------------------

    /// Выдаёт/обновляет грант гостя сессии. Повторный join — re-grant.
    pub async fn grant_session_guest(
        &self,
        session_id: Uuid,
        guest_device_id: Uuid,
    ) -> Result<SessionGuestRow, SessionError> {
        let row = sqlx::query_as::<_, SessionGuestRow>(
            "INSERT INTO session_guest (id, session_id, guest_device_id) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (session_id, guest_device_id) \
             DO UPDATE SET revoked_at = NULL, joined_at = now() \
             RETURNING id, session_id, guest_device_id, joined_at, revoked_at",
        )
        .bind(Uuid::now_v7())
        .bind(session_id)
        .bind(guest_device_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Живой грант (не отозван). Основание для POST /requests (F4).
    pub async fn find_live_grant(
        &self,
        session_id: Uuid,
        guest_device_id: Uuid,
    ) -> Result<Option<SessionGuestRow>, SessionError> {
        let row = sqlx::query_as::<_, SessionGuestRow>(
            "SELECT id, session_id, guest_device_id, joined_at, revoked_at \
             FROM session_guest WHERE session_id = $1 AND guest_device_id = $2 \
             AND revoked_at IS NULL",
        )
        .bind(session_id)
        .bind(guest_device_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Живая (не closed) сессия гостя по его грантам — последняя.
    /// Основание для POST /requests: сервер НЕ верит клиентскому
    /// session_id.
    pub async fn find_active_session_of_guest(
        &self,
        guest_device_id: Uuid,
    ) -> Result<Option<SessionRow>, SessionError> {
        let row = sqlx::query_as::<_, SessionRow>(
            "SELECT s.id, s.club_id, s.dj_id, s.status, s.pricing_policy_id, s.opened_at, \
             s.closed_at, s.timezone, s.created_at \
             FROM session s \
             JOIN session_guest g ON g.session_id = s.id \
             WHERE g.guest_device_id = $1 AND g.revoked_at IS NULL AND s.status <> 'closed' \
             ORDER BY g.joined_at DESC LIMIT 1",
        )
        .bind(guest_device_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Текущая открытая сессия гостя для `GET /v1/sessions/current`.
    ///
    /// Возвращает последнюю открытую (`active`/`paused_acceptance`)
    /// сессию клуба, где у гостя действующий (не отозванный) грант.
    /// `created`/`closed` отсекаются: «текущий вечер» — это уже
    /// открытая DJ-смен и ещё не закрытая. При нескольких — самая
    /// свежая по `opened_at` (таубрейкер — `created_at`).
    ///
    /// Сигнатура: `&PgPool` (через `&self`) + `guest_device_id`.
    pub async fn current_session_for_guest(
        &self,
        guest_device_id: Uuid,
    ) -> Result<Option<CurrentGuestSession>, SessionError> {
        let row = sqlx::query_as::<_, CurrentGuestSession>(
            "SELECT s.id, s.club_id, c.name AS club_name, \
             COALESCE(c.address, '') AS club_city, \
             s.status, s.opened_at, s.closed_at, s.timezone, s.dj_id, \
             d.display_name AS dj_display_name, \
             (SELECT qt.expires_at FROM qr_token qt \
              WHERE qt.session_id = s.id AND qt.active \
              ORDER BY qt.generated_at DESC LIMIT 1) AS grant_expires_at \
             FROM session s \
             JOIN session_guest g ON g.session_id = s.id \
             JOIN club c ON c.id = s.club_id \
             LEFT JOIN dj d ON d.id = s.dj_id \
             WHERE g.guest_device_id = $1 AND g.revoked_at IS NULL \
             AND s.status IN ('active', 'paused_acceptance') \
             AND s.opened_at IS NOT NULL \
             ORDER BY s.opened_at DESC, s.created_at DESC \
             LIMIT 1",
        )
        .bind(guest_device_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Текущая сессия DJ (последняя не closed).
    pub async fn find_current_dj_session(
        &self,
        dj_id: Uuid,
    ) -> Result<Option<SessionRow>, SessionError> {
        let row = sqlx::query_as::<_, SessionRow>(
            "SELECT id, club_id, dj_id, status, pricing_policy_id, opened_at, closed_at, \
             timezone, created_at FROM session \
             WHERE dj_id = $1 AND status <> 'closed' \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(dj_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Информация о сессии для гостя: клуб, DJ, статус, цена.
    /// Цена — из действующей tariff-политики клуба.
    pub async fn session_info_for_guest(
        &self,
        session_id: Uuid,
    ) -> Result<Option<SessionInfo>, SessionError> {
        let row = sqlx::query_as::<_, SessionInfo>(
            "SELECT s.id AS session_id, c.name AS club_name, d.display_name AS dj_display_name, \
             s.status, s.timezone, \
             (SELECT pp.price_minor FROM pricing_policy pp \
              WHERE pp.club_id = s.club_id AND pp.valid_to IS NULL \
              ORDER BY pp.valid_from DESC LIMIT 1) AS price_minor \
             FROM session s \
             JOIN club c ON c.id = s.club_id \
             LEFT JOIN dj d ON d.id = s.dj_id \
             WHERE s.id = $1",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }
}

fn status_str(s: SessionStatus) -> &'static str {
    match s {
        SessionStatus::Created => "created",
        SessionStatus::Active => "active",
        SessionStatus::PausedAcceptance => "paused_acceptance",
        SessionStatus::Closed => "closed",
    }
}

#[cfg(test)]
mod tests {
    // Интеграционные тесты репозитория живут в gateway e2e (нужна живая БД);
    // здесь только чистые проверки без БД.
    use super::*;

    #[test]
    fn status_strings_roundtrip() {
        for (s, str_) in [
            (SessionStatus::Created, "created"),
            (SessionStatus::Active, "active"),
            (SessionStatus::PausedAcceptance, "paused_acceptance"),
            (SessionStatus::Closed, "closed"),
        ] {
            assert_eq!(status_str(s), str_);
        }
    }
}
