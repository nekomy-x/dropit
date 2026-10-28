//! Репозитории пакета W2-D: кеш поиска каталога, стили DJ, push-токены гостей.
//!
//! Три независимых репозитория, объединённых в один модуль чтобы не плодить
//! новых `pub mod`-строк в `lib.rs` (правило владения пакета — одна строка
//! `pub mod catalog;`).
//!
//! * [`CatalogRepo`] — кеш поиска треков: индекс запросов
//!   `catalog_query_cache` (query_key, style → track_ids) + полные хиты в
//!   `track_cache`. Свежесть по `cache_ttl(has_isrc)` (24 ч / 1 ч).
//! * [`DjProfileRepo`] — массив `styles text[]` в профиле DJ.
//! * [`PushTokenRepo`] — идемпотентный upsert push-токенов гостей.
//!
//! Кэш намеренно НЕ зависит от крейта `dropit-infra-catalog`: канонический
//! ключ песни (`song_key`) и признак ISRC передаются вызывающим кодом
//! (gateway), что сохраняет слой инфраструктуры БД независимым от
//! транспортного слоя каталога.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

// ============================================================
// Кеш поиска каталога
// ============================================================

/// Одна запись кеша каталога в `track_cache` (расширенные поля хита).
///
/// `cover_url` хранит URL обложки и отображается в ответе как `artwork_url`.
/// `song_key` — канонический ключ песни `canonical_key(artist, title)`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CachedTrackRow {
    /// Идентификатор строки (UUIDv7).
    pub id: Uuid,
    /// Источник (`itunes` / `deezer`).
    pub source: String,
    /// Идентификатор трека в источнике.
    pub source_id: String,
    /// ISRC, если источник сообщил.
    pub isrc: Option<String>,
    /// Название трека.
    pub title: String,
    /// Исполнитель.
    pub artist: String,
    /// URL обложки (в ответе — `artwork_url`).
    pub cover_url: Option<String>,
    /// URL 30-сек превью.
    pub preview_url: Option<String>,
    /// Альбом.
    pub album: Option<String>,
    /// Длительность в миллисекундах.
    pub duration_ms: i64,
    /// Каноническая страница трека в источнике.
    pub source_url: Option<String>,
    /// Признак explicit-контента.
    pub explicit: bool,
    /// Канонический ключ песни `canonical_key(artist, title)`.
    pub song_key: Option<String>,
    /// Время последнего обновления строки (для TTL).
    pub updated_at: DateTime<Utc>,
}

/// Параметр upsert-а одного хита внешнего каталога в `track_cache`.
///
/// Серверный (gateway) код строит эту структуру из `CatalogHit`, вычисляя
/// `canonical_key` и `song_key` через `dropit_infra_catalog`.
#[derive(Debug, Clone)]
pub struct CatalogUpsert<'a> {
    pub source: &'a str,
    pub source_id: &'a str,
    /// Канонический ключ `source:source_id` (существующая семантика колонки).
    pub canonical_key: &'a str,
    pub isrc: Option<&'a str>,
    pub title: &'a str,
    pub artist: &'a str,
    /// URL обложки (`cover_url` в БД, `artwork_url` в ответе).
    pub artwork_url: Option<&'a str>,
    pub preview_url: Option<&'a str>,
    pub album: Option<&'a str>,
    pub duration_ms: i64,
    pub source_url: Option<&'a str>,
    pub explicit: bool,
    /// Канонический ключ песни `canonical_key(artist, title)`.
    pub song_key: &'a str,
}

/// Запись индекса запросов поиска `catalog_query_cache`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct QueryCacheRow {
    pub track_ids: Vec<Uuid>,
    pub has_isrc: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct CatalogRepo {
    pool: PgPool,
}

impl CatalogRepo {
    /// Создаёт репозиторий на заданном пуле.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Пул для построения других репозиториев.
    pub fn pool(&self) -> PgPool {
        self.pool.clone()
    }

    /// Ищет свежую запись индекса запросов.
    ///
    /// `style` — канонический ключ стиля; пустая строка означает «без стиля»
    /// (колонка `style` входит в составной PRIMARY KEY и потому NOT NULL).
    /// Возвращает `(track_ids, has_isrc, created_at)`, если строка есть
    /// (свежесть проверяет вызывающий через `cache_ttl(has_isrc)`).
    pub async fn find_query_cache(
        &self,
        query_key: &str,
        style: &str,
    ) -> Result<Option<QueryCacheRow>, sqlx::Error> {
        let row = sqlx::query_as::<_, QueryCacheRow>(
            "SELECT track_ids, has_isrc, created_at \
             FROM catalog_query_cache \
             WHERE query_key = $1 AND style = $2",
        )
        .bind(query_key)
        .bind(style)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Загружает треки кеша по идентификаторам (в порядке `track_ids`).
    ///
    /// Возвращает строки в порядке передачи `ids`; отсутствующие id
    /// пропускаются (могут быть удалены retention-процессом).
    pub async fn load_tracks_by_ids(
        &self,
        ids: &[Uuid],
    ) -> Result<Vec<CachedTrackRow>, sqlx::Error> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_as::<_, CachedTrackRow>(
            "SELECT id, source, source_id, isrc, title, artist, cover_url, preview_url, \
                    album, duration_ms, source_url, explicit, song_key, updated_at \
             FROM track_cache WHERE id = ANY($1)",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;
        // Восстанавливаем порядок запроса (ANY не гарантирует порядок).
        let mut by_id: std::collections::HashMap<Uuid, CachedTrackRow> =
            rows.into_iter().map(|r| (r.id, r)).collect();
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(r) = by_id.remove(id) {
                out.push(r);
            }
        }
        Ok(out)
    }

    /// Upsert-ит хиты внешнего каталога в `track_cache` (по `source,source_id`),
    /// обновляя `song_key`, media-поля и `updated_at`. Возвращает id строк.
    ///
    /// `canonical_key` (колонка, `source:source_id`) сохраняет существующую
    /// семантику счётчика «той же песни» в `requests`; `song_key` —
    /// канонический ключ песни для дедупликации кеша каталога.
    pub async fn upsert_hits<'a>(
        &self,
        hits: &'a [CatalogUpsert<'a>],
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        let mut ids = Vec::with_capacity(hits.len());
        for h in hits {
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO track_cache \
                    (id, source, source_id, canonical_key, isrc, title, artist, \
                     cover_url, preview_url, album, duration_ms, source_url, explicit, song_key) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) \
                 ON CONFLICT (source, source_id) DO UPDATE SET \
                    canonical_key = EXCLUDED.canonical_key, \
                    isrc = EXCLUDED.isrc, \
                    title = EXCLUDED.title, \
                    artist = EXCLUDED.artist, \
                    cover_url = EXCLUDED.cover_url, \
                    preview_url = EXCLUDED.preview_url, \
                    album = EXCLUDED.album, \
                    duration_ms = EXCLUDED.duration_ms, \
                    source_url = EXCLUDED.source_url, \
                    explicit = EXCLUDED.explicit, \
                    song_key = EXCLUDED.song_key, \
                    updated_at = now() \
                 RETURNING id",
            )
            .bind(Uuid::now_v7())
            .bind(h.source)
            .bind(h.source_id)
            .bind(h.canonical_key)
            .bind(h.isrc)
            .bind(h.title)
            .bind(h.artist)
            .bind(h.artwork_url)
            .bind(h.preview_url)
            .bind(h.album)
            .bind(h.duration_ms)
            .bind(h.source_url)
            .bind(h.explicit)
            .bind(h.song_key)
            .fetch_one(&self.pool)
            .await?;
            ids.push(id);
        }
        Ok(ids)
    }

    /// Сохраняет (upsert) индекс запроса: `(query_key, style) → track_ids`.
    ///
    /// `style` — пустая строка для «без стиля» (колонка NOT NULL как часть
    /// составного PRIMARY KEY). `has_isrc` определяет TTL записи
    /// (`cache_ttl(has_isrc)`).
    pub async fn save_query_cache(
        &self,
        query_key: &str,
        style: &str,
        track_ids: &[Uuid],
        has_isrc: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO catalog_query_cache (query_key, style, track_ids, has_isrc) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (query_key, style) DO UPDATE SET \
                track_ids = EXCLUDED.track_ids, \
                has_isrc = EXCLUDED.has_isrc, \
                created_at = now()",
        )
        .bind(query_key)
        .bind(style)
        .bind(track_ids)
        .bind(has_isrc)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Признак того, что запись индекса запросов свежая по TTL.
    ///
    /// `ttl` — политика `cache_ttl(has_isrc)` из `dropit-infra-catalog`
    /// (`std::time::Duration`); конвертируется в `chrono::Duration` для
    /// сравнения с `created_at`.
    pub fn is_fresh(row: &QueryCacheRow, ttl: std::time::Duration) -> bool {
        let ttl = chrono::Duration::from_std(ttl).unwrap_or(chrono::Duration::zero());
        row.created_at + ttl >= Utc::now()
    }
}

// ============================================================
// Стили DJ
// ============================================================

#[derive(Clone)]
pub struct DjProfileRepo {
    pool: PgPool,
}

impl DjProfileRepo {
    /// Создаёт репозиторий на заданном пуле.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Заменяет массив `styles` у профиля DJ.
    ///
    /// Возвращает `true`, если строка DJ найдена и обновлена; `false`, если
    /// DJ с таким id нет (вызывающий отвечает 404, чтобы не раскрывать
    /// существование).
    pub async fn set_styles(&self, dj_id: Uuid, styles: &[String]) -> Result<bool, sqlx::Error> {
        let row: Option<(Uuid,)> =
            sqlx::query_as("UPDATE dj SET styles = $2 WHERE id = $1 RETURNING id")
                .bind(dj_id)
                .bind(styles)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.is_some())
    }

    /// Возвращает текущий массив `styles` DJ или `None`, если DJ не найден.
    pub async fn get_styles(&self, dj_id: Uuid) -> Result<Option<Vec<String>>, sqlx::Error> {
        let row: Option<(Option<Vec<String>>,)> =
            sqlx::query_as("SELECT styles FROM dj WHERE id = $1")
                .bind(dj_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(s,)| s.unwrap_or_default()))
    }
}

// ============================================================
// Push-токены гостей
// ============================================================

/// Поддерживаемые платформы push-токена (lowercase).
pub const PUSH_PLATFORMS: &[&str] = &["android", "ios"];

#[derive(Clone)]
pub struct PushTokenRepo {
    pool: PgPool,
}

impl PushTokenRepo {
    /// Создаёт репозиторий на заданном пуле.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Идемпотентная регистрация push-токена гостя.
    ///
    /// Повторная регистрация того же токена обновляет `last_seen_at`; перевод
    /// токена другому гостю обновляет `guest_id`; повторная активация
    /// отозванного токена очищает `revoked_at`.
    pub async fn upsert_token(
        &self,
        guest_id: Uuid,
        token: &str,
        platform: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO guest_push_token (id, guest_id, token, platform) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (token) DO UPDATE SET \
                guest_id = EXCLUDED.guest_id, \
                platform = EXCLUDED.platform, \
                last_seen_at = now(), \
                revoked_at = NULL",
        )
        .bind(Uuid::now_v7())
        .bind(guest_id)
        .bind(token)
        .bind(platform)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
