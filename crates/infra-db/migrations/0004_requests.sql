-- DROP IT — миграция 0004: запросы и кэш каталога (F4, БЕЗ денег — F5).
-- Модель — Часть II технической карты; стейт-машина request — Часть III.
--
-- ПРАВИЛА:
--   * request.version — optimistic locking (гонки accept/played);
--   * счётчик «той же песни» — по canonical-ключу (source, source_id)
--     трека, НЕ по id строки track_cache;
--   * idempotency_response — replay-ответы дублей POST /requests
--     (TTL 24 ч, воркер чистит);
--   * outbox — transactional outbox (ADR-002): события запросов
--     публикуются воркером в NATS (с F5); пока служит источником
--     уведомлений WS через poll.

-- ============================================================
-- Кэш каталога: валидированные треки (ADR-006).
-- UNIQUE(source, source_id) — canonical-ключ; счётчики по нему.
-- ============================================================
CREATE TABLE track_cache (
    id                      UUID PRIMARY KEY,          -- UUIDv7
    source                  TEXT NOT NULL,             -- 'itunes' | 'deezer' | 'musicbrainz'
    source_id               TEXT NOT NULL,
    canonical_key           TEXT NOT NULL,             -- 'itunes:12345' — стабильный ключ
    isrc                    TEXT,
    title                   TEXT NOT NULL,
    artist                  TEXT NOT NULL,
    cover_url               TEXT,
    preview_url             TEXT,
    preview_url_checked_at  TIMESTAMPTZ,
    style_tags              JSONB,
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (source, source_id)
);

CREATE INDEX idx_track_cache_canonical ON track_cache (canonical_key);

-- ============================================================
-- Запросы гостя. price_minor — снимок pricing_policy на момент
-- создания (FOR SHARE — защита от гонки смены тарифа, в F5).
-- ============================================================
CREATE TABLE request (
    id              UUID PRIMARY KEY,               -- UUIDv7
    session_id      UUID NOT NULL REFERENCES session (id),
    guest_device_id UUID NOT NULL REFERENCES guest_device (id),
    track_cache_id  UUID NOT NULL REFERENCES track_cache (id),
    style           TEXT,                            -- стиль гостя к треку
    price_minor     BIGINT NOT NULL DEFAULT 0,       -- F4: без денег = 0; F5: снимок тарифа
    status          TEXT NOT NULL DEFAULT 'pending', -- pending/accepted/rejected/played/expired
    reject_reason   TEXT,
    played_at       TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    version         INTEGER NOT NULL DEFAULT 1       -- optimistic locking
);

CREATE INDEX idx_request_session_status ON request (session_id, status);
CREATE INDEX idx_request_guest ON request (guest_device_id, created_at DESC);
CREATE INDEX idx_request_track ON request (track_cache_id);

-- ============================================================
-- Replay-ответы идемпотентных POST /requests (дубль ключа).
-- ============================================================
CREATE TABLE idempotency_response (
    key             TEXT PRIMARY KEY,               -- Idempotency-Key клиента
    guest_device_id UUID NOT NULL REFERENCES guest_device (id),
    request_id      UUID NOT NULL REFERENCES request (id),
    response        JSONB NOT NULL,                 -- сохранённый ответ первого POST
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_idempotency_created ON idempotency_response (created_at);

-- ============================================================
-- Transactional outbox (ADR-002). События запросов — источник
-- уведомлений WS (ADR-005: WS только уведомляет).
-- ============================================================
CREATE TABLE outbox (
    id             UUID PRIMARY KEY,                -- UUIDv7
    aggregate_type TEXT NOT NULL,                   -- 'request' | 'session' | 'payment'
    aggregate_id   UUID NOT NULL,
    event_type     TEXT NOT NULL,                   -- 'request.created' | 'request.accepted' | ...
    payload        JSONB NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at   TIMESTAMPTZ,
    attempts       INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_outbox_unpublished ON outbox (created_at) WHERE published_at IS NULL;
