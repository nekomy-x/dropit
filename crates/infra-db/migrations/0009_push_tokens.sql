-- DROP IT — миграция 0009: push-токены гостей, стили DJ, кеш поиска каталога.
-- Пакет W2-D.
--
-- ПРАВИЛА (техкарта Часть VI «Каталог» и Часть II «Роли»):
--   * кеш поиска каталога живёт в track_cache (расширенные поля хита) +
--     индекс запросов catalog_query_cache (query_key,style -> track_ids);
--     канонический ключ песни song_key = canonical_key(artist,title);
--   * TTL кеша — cache_ttl(isrc_present): 24 ч с ISRC, 1 ч без;
--   * стили DJ — text[] в профиле (0..=8, нормализация trim+lowercase+дедуп);
--   * guest_push_token — идемпотентный upsert по UNIQUE(token): повторная
--     регистрация обновляет last_seen_at, перевод токена другому гостю
--     обновляет guest_id, повторная активация очищает revoked_at.

-- ============================================================
-- 1. track_cache: расширение для хранения полных хитов внешнего поиска.
--    Существующие колонки (source, source_id, canonical_key, isrc, title,
--    artist, cover_url, preview_url) НЕ меняются — только добавляются.
--    cover_url используется как artwork_url ответа каталога.
-- ============================================================
ALTER TABLE track_cache
    ADD COLUMN IF NOT EXISTS album        TEXT,
    ADD COLUMN IF NOT EXISTS duration_ms  BIGINT    NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS source_url   TEXT,
    ADD COLUMN IF NOT EXISTS explicit     BOOLEAN   NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS song_key     TEXT;

CREATE INDEX IF NOT EXISTS idx_track_cache_song_key
    ON track_cache (song_key) WHERE song_key IS NOT NULL;

-- ============================================================
-- 2. Индекс запросов поиска: (query_key, style) -> track_cache ids.
--    Свежесть определяется created_at + cache_ttl(has_isrc).
-- ============================================================
CREATE TABLE IF NOT EXISTS catalog_query_cache (
    query_key   TEXT        NOT NULL,
    style       TEXT,
    track_ids   UUID[]      NOT NULL,
    has_isrc    BOOLEAN     NOT NULL DEFAULT false,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (query_key, style)
);

-- ============================================================
-- 3. Стили DJ: массив тегов в профиле диджея.
-- ============================================================
ALTER TABLE dj ADD COLUMN IF NOT EXISTS styles TEXT[] NOT NULL DEFAULT '{}';

-- ============================================================
-- 4. Push-токены гостей.
-- ============================================================
CREATE TABLE IF NOT EXISTS guest_push_token (
    id           UUID PRIMARY KEY,
    guest_id     UUID NOT NULL,
    token        TEXT NOT NULL,
    platform     TEXT NOT NULL,              -- 'android' | 'ios'
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at   TIMESTAMPTZ,
    UNIQUE (token)
);

CREATE INDEX IF NOT EXISTS idx_guest_push_token_guest
    ON guest_push_token (guest_id) WHERE revoked_at IS NULL;
