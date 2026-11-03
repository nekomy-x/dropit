//! Репозиторий Developer API v1 (ADR-010): выборки, ограниченные клубом
//! ключа, статистика за период и «синтетическое» устройство ключа для
//! заявок, созданных интеграцией.
//!
//! Инвариант: у каждого метода в аргументах первым идёт `club_id` ключа.
//! Запросы без `WHERE club_id = $1` здесь запрещены — именно это делает
//! «чужой id → 404» свойством репозитория, а не хендлера. Исключение —
//! справочник треков `track_cache` (глобальный каталог, не данные клуба) и
//! вычисление синтетического `guest_device` (идентичность интеграции).
//!
//! Скоупы и маршруты проверяет gateway (`developer_api.rs`); здесь только
//! SQL. Курсор пагинации (`created_at, id`) разбирает вызывающий слой,
//! репозиторий получает уже готовую пару и не интерпретирует её.

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

/// Ошибка репозитория Developer API.
#[derive(Debug, thiserror::Error)]
pub enum DeveloperError {
    #[error("database error")]
    Db(#[from] sqlx::Error),
}

/// Профиль клуба в ответе `GET /v1/developer/profile`.
///
/// Сознательно НЕ содержит `stripe_account_id`, `stripe_onboarding_status`,
/// `password_hash` и TOTP-полей: публичному API они не нужны.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ClubProfileRow {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub address: Option<String>,
    pub country: Option<String>,
    pub email: String,
    pub phone: Option<String>,
    pub logo_url: Option<String>,
    pub timezone: Option<String>,
    pub status: String,
}

/// Сессия (смена) в понимании Developer API.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DeveloperSessionRow {
    pub id: Uuid,
    pub status: String,
    pub timezone: String,
    pub dj_id: Option<Uuid>,
    pub pricing_policy_id: Option<Uuid>,
    pub opened_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Счётчики заявок сессии (для `GET /v1/developer/sessions/{id}`).
#[derive(Debug, Clone, Copy, Default)]
pub struct SessionCounts {
    pub total: i64,
    pub pending: i64,
    pub accepted: i64,
    pub rejected: i64,
    pub played: i64,
    /// Активная очередь: pending + accepted (та же семантика, что у DJ-консоли).
    pub queue: i64,
}

/// Заявка с данными трека (список и карточка Developer API).
///
/// Здесь НЕТ `guest_device_id`: посетитель — не данные API (ADR-010: гостевая
/// идентичность наружу не отдаётся), поэтому колонка не выбирается вовсе.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DeveloperRequestRow {
    pub id: Uuid,
    pub session_id: Uuid,
    pub status: String,
    pub style: Option<String>,
    pub price_minor: i64,
    pub reject_reason: Option<String>,
    pub played_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub title: String,
    pub artist: String,
    pub cover_url: Option<String>,
    pub preview_url: Option<String>,
    pub isrc: Option<String>,
}

/// Агрегаты статистики клуба за период (без разбивки по заявкам).
#[derive(Debug, Clone)]
pub struct OverviewStats {
    pub sessions: i64,
    pub requests_total: i64,
    pub pending: i64,
    pub accepted: i64,
    pub rejected: i64,
    pub played: i64,
    /// Возвращённые деньги (captured минус refunds), минорные единицы.
    pub captured_minor: i64,
    pub currency: String,
}

/// Строка топа треков за период.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TopTrackRow {
    pub title: String,
    pub artist: String,
    pub requests: i64,
}

/// Текущая незакрытая сессия клуба (для `active_session`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ActiveSessionRow {
    pub id: Uuid,
    pub status: String,
    pub opened_at: Option<DateTime<Utc>>,
}

/// Репозиторий публичного API.
#[derive(Clone)]
pub struct DeveloperRepo {
    pool: PgPool,
}

impl DeveloperRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    // ---------------------------------------------------------
    // Профиль
    // ---------------------------------------------------------

    /// Профиль клуба по id ключа. `None` — клуба нет (ключ-сирота: 404).
    ///
    /// Контактные поля клуба (email/phone/address) — данные самого клуба,
    /// а не гостя: их отдаём только под `profile:read`.
    pub async fn club_profile(
        &self,
        club_id: Uuid,
    ) -> Result<Option<ClubProfileRow>, DeveloperError> {
        let row = sqlx::query_as::<_, ClubProfileRow>(
            "SELECT id, name, slug, address, country, email, phone, logo_url, \
             timezone, status FROM club WHERE id = $1",
        )
        .bind(club_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    // ---------------------------------------------------------
    // Сессии
    // ---------------------------------------------------------

    /// Одна сессия клуба. Чужой/отсутствующий id → `None` (хендлер отдаёт 404).
    pub async fn find_session(
        &self,
        club_id: Uuid,
        session_id: Uuid,
    ) -> Result<Option<DeveloperSessionRow>, DeveloperError> {
        let row = sqlx::query_as::<_, DeveloperSessionRow>(
            "SELECT id, status, timezone, dj_id, pricing_policy_id, opened_at, \
             closed_at, created_at FROM session \
             WHERE club_id = $1 AND id = $2",
        )
        .bind(club_id)
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Страница сессий клуба: новые сверху, курсор — последняя отданная пара
    /// `(created_at, id)`. `limit` уже провалидирован и ограничен вызывающим.
    pub async fn list_sessions(
        &self,
        club_id: Uuid,
        status: Option<&str>,
        limit: i64,
        cursor: Option<(DateTime<Utc>, Uuid)>,
    ) -> Result<Vec<DeveloperSessionRow>, DeveloperError> {
        let (cursor_at, cursor_id) = match cursor {
            Some((at, id)) => (Some(at), Some(id)),
            None => (None, None),
        };
        let rows = sqlx::query_as::<_, DeveloperSessionRow>(
            "SELECT id, status, timezone, dj_id, pricing_policy_id, opened_at, \
             closed_at, created_at FROM session \
             WHERE club_id = $1 \
               AND ($2::text IS NULL OR status = $2::text) \
               AND ($3::timestamptz IS NULL OR (created_at, id) < ($3::timestamptz, $4::uuid)) \
             ORDER BY created_at DESC, id DESC LIMIT $5",
        )
        .bind(club_id)
        .bind(status)
        .bind(cursor_at)
        .bind(cursor_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Счётчики заявок сессии (только для сессии этого клуба).
    pub async fn session_counts(
        &self,
        club_id: Uuid,
        session_id: Uuid,
    ) -> Result<SessionCounts, DeveloperError> {
        let row: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
            "SELECT count(*), \
                    count(*) FILTER (WHERE r.status = 'pending'), \
                    count(*) FILTER (WHERE r.status = 'accepted'), \
                    count(*) FILTER (WHERE r.status = 'rejected'), \
                    count(*) FILTER (WHERE r.status = 'played'), \
                    count(*) FILTER (WHERE r.status IN ('pending', 'accepted')) \
             FROM request r JOIN session s ON s.id = r.session_id \
             WHERE s.club_id = $1 AND r.session_id = $2",
        )
        .bind(club_id)
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(SessionCounts {
            total: row.0,
            pending: row.1,
            accepted: row.2,
            rejected: row.3,
            played: row.4,
            queue: row.5,
        })
    }

    /// Принадлежит ли DJ этому клубу (и не удалён из состава).
    pub async fn dj_belongs_to_club(
        &self,
        club_id: Uuid,
        dj_id: Uuid,
    ) -> Result<bool, DeveloperError> {
        let row: (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM dj WHERE club_id = $1 AND id = $2 \
             AND removed_at IS NULL)",
        )
        .bind(club_id)
        .bind(dj_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// Принадлежит ли тарифная политика этому клубу.
    pub async fn pricing_policy_belongs_to_club(
        &self,
        club_id: Uuid,
        policy_id: Uuid,
    ) -> Result<bool, DeveloperError> {
        let row: (bool,) = sqlx::query_as(
            "SELECT EXISTS (SELECT 1 FROM pricing_policy \
             WHERE club_id = $1 AND id = $2)",
        )
        .bind(club_id)
        .bind(policy_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// Создаёт сессию клуба (`status='created'`).
    ///
    /// DJ и тариф уже проверены [`Self::dj_belongs_to_club`] /
    /// [`Self::pricing_policy_belongs_to_club`] — здесь только INSERT, чтобы
    /// не смешивать валидацию (404) с созданием (201).
    pub async fn create_session(
        &self,
        club_id: Uuid,
        dj_id: Option<Uuid>,
        pricing_policy_id: Option<Uuid>,
        timezone: &str,
    ) -> Result<DeveloperSessionRow, DeveloperError> {
        let row = sqlx::query_as::<_, DeveloperSessionRow>(
            "INSERT INTO session (id, club_id, dj_id, pricing_policy_id, timezone) \
             VALUES ($1, $2, $3, $4, $5) \
             RETURNING id, status, timezone, dj_id, pricing_policy_id, opened_at, \
             closed_at, created_at",
        )
        .bind(Uuid::now_v7())
        .bind(club_id)
        .bind(dj_id)
        .bind(pricing_policy_id)
        .bind(timezone)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    // ---------------------------------------------------------
    // Заявки
    // ---------------------------------------------------------

    /// Страница заявок клуба (все сессии). Фильтры `status`/`session_id`
    /// опциональны; курсор — последняя отданная пара `(created_at, id)`.
    pub async fn list_requests(
        &self,
        club_id: Uuid,
        status: Option<&str>,
        session_id: Option<Uuid>,
        limit: i64,
        cursor: Option<(DateTime<Utc>, Uuid)>,
    ) -> Result<Vec<DeveloperRequestRow>, DeveloperError> {
        let (cursor_at, cursor_id) = match cursor {
            Some((at, id)) => (Some(at), Some(id)),
            None => (None, None),
        };
        let rows = sqlx::query_as::<_, DeveloperRequestRow>(
            "SELECT r.id, r.session_id, r.status, r.style, r.price_minor, \
                    r.reject_reason, r.played_at, r.created_at, \
                    t.title, t.artist, t.cover_url, t.preview_url, t.isrc \
             FROM request r \
             JOIN session s ON s.id = r.session_id \
             JOIN track_cache t ON t.id = r.track_cache_id \
             WHERE s.club_id = $1 \
               AND ($2::text IS NULL OR r.status = $2::text) \
               AND ($3::uuid IS NULL OR r.session_id = $3::uuid) \
               AND ($4::timestamptz IS NULL OR (r.created_at, r.id) < ($4::timestamptz, $5::uuid)) \
             ORDER BY r.created_at DESC, r.id DESC LIMIT $6",
        )
        .bind(club_id)
        .bind(status)
        .bind(session_id)
        .bind(cursor_at)
        .bind(cursor_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Одна заявка клуба (сквозь сессию: `s.club_id = $1`). Чужая → `None`.
    pub async fn find_request(
        &self,
        club_id: Uuid,
        request_id: Uuid,
    ) -> Result<Option<DeveloperRequestRow>, DeveloperError> {
        let row = sqlx::query_as::<_, DeveloperRequestRow>(
            "SELECT r.id, r.session_id, r.status, r.style, r.price_minor, \
                    r.reject_reason, r.played_at, r.created_at, \
                    t.title, t.artist, t.cover_url, t.preview_url, t.isrc \
             FROM request r \
             JOIN session s ON s.id = r.session_id \
             JOIN track_cache t ON t.id = r.track_cache_id \
             WHERE s.club_id = $1 AND r.id = $2",
        )
        .bind(club_id)
        .bind(request_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Существует ли трек в кэше каталога (глобальный справочник, не клуб).
    pub async fn track_exists(&self, track_id: Uuid) -> Result<bool, DeveloperError> {
        let row: (bool,) =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM track_cache WHERE id = $1)")
                .bind(track_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(row.0)
    }

    /// Готовит «синтетического гостя» интеграции к сессии: само устройство
    /// (`platform='api'`) и живой `session_guest`-грант, без которого домен
    /// (RequestRepo::create_request) не примет заявку от имени ключа.
    ///
    /// Возвращает `false`, если сессия не принадлежит клубу — хендлер отвечает
    /// 404 и ничего не пишет. Идентичность устройства детерминирована от
    /// ключа ([`api_guest_device_id`]), поэтому повторные вызовы идемпотентны.
    pub async fn ensure_api_guest(
        &self,
        club_id: Uuid,
        session_id: Uuid,
        guest_device_id: Uuid,
    ) -> Result<bool, DeveloperError> {
        let mut tx = self.pool.begin().await?;
        let owned: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM session WHERE club_id = $1 AND id = $2 FOR SHARE")
                .bind(club_id)
                .bind(session_id)
                .fetch_optional(&mut *tx)
                .await?;
        if owned.is_none() {
            tx.rollback().await?;
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO guest_device (id, platform) VALUES ($1, 'api') \
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(guest_device_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO session_guest (id, session_id, guest_device_id) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (session_id, guest_device_id) \
             DO UPDATE SET revoked_at = NULL",
        )
        .bind(Uuid::now_v7())
        .bind(session_id)
        .bind(guest_device_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    // ---------------------------------------------------------
    // Статистика
    // ---------------------------------------------------------

    /// Агрегаты клуба за `[from, to)`: сессии, заявки по статусам, выручка.
    pub async fn overview(
        &self,
        club_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<OverviewStats, DeveloperError> {
        let row: (i64, i64, i64, i64, i64, i64, i64, String) = sqlx::query_as(
            "SELECT \
               (SELECT count(*) FROM session s \
                WHERE s.club_id = $1 AND s.created_at >= $2 AND s.created_at < $3), \
               (SELECT count(*) FROM request r JOIN session s ON s.id = r.session_id \
                WHERE s.club_id = $1 AND r.created_at >= $2 AND r.created_at < $3), \
               (SELECT count(*) FROM request r JOIN session s ON s.id = r.session_id \
                WHERE s.club_id = $1 AND r.created_at >= $2 AND r.created_at < $3 \
                  AND r.status = 'pending'), \
               (SELECT count(*) FROM request r JOIN session s ON s.id = r.session_id \
                WHERE s.club_id = $1 AND r.created_at >= $2 AND r.created_at < $3 \
                  AND r.status = 'accepted'), \
               (SELECT count(*) FROM request r JOIN session s ON s.id = r.session_id \
                WHERE s.club_id = $1 AND r.created_at >= $2 AND r.created_at < $3 \
                  AND r.status = 'rejected'), \
               (SELECT count(*) FROM request r JOIN session s ON s.id = r.session_id \
                WHERE s.club_id = $1 AND r.created_at >= $2 AND r.created_at < $3 \
                  AND r.status = 'played'), \
               (SELECT COALESCE(sum(p.amount_minor - p.refunded_minor), 0)::bigint \
                FROM payment p \
                JOIN request r ON r.id = p.request_id \
                JOIN session s ON s.id = r.session_id \
                WHERE s.club_id = $1 \
                  AND p.status IN ('captured', 'partially_refunded', 'refunded') \
                  AND p.captured_at >= $2 AND p.captured_at < $3), \
               COALESCE((SELECT currency FROM platform_settings WHERE id = 1), 'EUR')",
        )
        .bind(club_id)
        .bind(from)
        .bind(to)
        .fetch_one(&self.pool)
        .await?;
        Ok(OverviewStats {
            sessions: row.0,
            requests_total: row.1,
            pending: row.2,
            accepted: row.3,
            rejected: row.4,
            played: row.5,
            captured_minor: row.6,
            currency: row.7,
        })
    }

    /// Топ треков периода: считаем принятые и сыгранные заявки (шум
    /// отклонённых в «топ» не попадает).
    pub async fn top_tracks(
        &self,
        club_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<TopTrackRow>, DeveloperError> {
        let rows = sqlx::query_as::<_, TopTrackRow>(
            "SELECT t.title, t.artist, count(*)::bigint AS requests \
             FROM request r \
             JOIN session s ON s.id = r.session_id \
             JOIN track_cache t ON t.id = r.track_cache_id \
             WHERE s.club_id = $1 \
               AND r.created_at >= $2 AND r.created_at < $3 \
               AND r.status IN ('accepted', 'played') \
             GROUP BY t.title, t.artist \
             ORDER BY requests DESC, t.title ASC LIMIT $4",
        )
        .bind(club_id)
        .bind(from)
        .bind(to)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Текущая незакрытая сессия клуба (последняя по created_at).
    pub async fn active_session(
        &self,
        club_id: Uuid,
    ) -> Result<Option<ActiveSessionRow>, DeveloperError> {
        let row = sqlx::query_as::<_, ActiveSessionRow>(
            "SELECT id, status, opened_at FROM session \
             WHERE club_id = $1 AND status <> 'closed' \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(club_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }
}

/// Детерминированный `guest_device.id` интеграции по id API-ключа.
///
/// Домен требует у заявки гостевую идентичность; для API это устройство
/// «принадлежит» ключу. UUID детерминирован: `sha256(key_id)` → первые 16
/// байт → UUIDv4 (RFC 4122 variant). Поэтому повторные вызовы не плодят
/// устройства, а все заявки ключа группируются под одной внутренней
/// идентичностью. Наружу этот id не отдаётся.
pub fn api_guest_device_id(key_id: Uuid) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(b"dropit:developer-api:guest:");
    hasher.update(key_id.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    // Версия 4 и вариант RFC 4122: валидный UUID, но детерминированный.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Один ключ → всегда один и тот же синтетический гость; разные ключи →
    /// разные устройства. Владелец не должен «переехать» на чужую заявку.
    #[test]
    fn api_guest_device_is_deterministic_and_distinct() {
        let key = Uuid::now_v7();
        let other = Uuid::now_v7();
        assert_eq!(api_guest_device_id(key), api_guest_device_id(key));
        assert_ne!(api_guest_device_id(key), api_guest_device_id(other));
        let id = api_guest_device_id(key);
        assert_eq!(id.get_version_num(), 4, "UUIDv4-формат");
        assert_eq!(id.as_bytes()[8] & 0xc0, 0x80, "вариант RFC 4122 (10xx)");
    }
}
