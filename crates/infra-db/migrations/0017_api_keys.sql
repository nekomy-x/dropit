-- DROP IT — миграция 0017: API-ключи публичного Developer API v1 (ADR-010).
--
-- ПРАВИЛА:
--   * в БД НЕТ plaintext-ключа: только prefix (публичный id) и
--     sha256(plaintext) (secret_hash). Утечка БД не даёт рабочих ключей;
--   * scopes — явный список разрешений, никакого wildcard; проверка
--     каждого маршрута на стороне gateway (deny by default);
--   * ключ принадлежит ровно одному клубу: все запросы Developer API
--     автоматически ограничены club_id ключа, чужие id → 404;
--   * api_key_usage — журнал вызовов (30 дней), нужен для лимита,
--     панели «последние вызовы» и разбора инцидентов; сырой ключ и
--     секрет сюда не попадают никогда;
--   * developer_idempotency — replay ответов POST с Idempotency-Key,
--     отдельно от гостевого idempotency_response (разные владельцы).

CREATE TABLE api_key (
    id                 UUID PRIMARY KEY,                    -- UUIDv7
    club_id            UUID NOT NULL REFERENCES club (id) ON DELETE CASCADE,
    name               TEXT NOT NULL,
    prefix             TEXT NOT NULL,                       -- 10 hex, публичный
    secret_hash        TEXT NOT NULL,                       -- sha256(plaintext) hex
    environment        TEXT NOT NULL DEFAULT 'live',        -- 'live' | 'test'
    scopes             TEXT[] NOT NULL,                     -- ['docs:read', ...]
    status             TEXT NOT NULL DEFAULT 'active',      -- 'active' | 'revoked'
    rate_limit_per_min INTEGER NOT NULL DEFAULT 120,
    allowed_cidrs      TEXT[] NOT NULL DEFAULT '{}',        -- exact IP / CIDR
    expires_at         TIMESTAMPTZ,
    created_by         UUID,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at       TIMESTAMPTZ,
    revoked_at         TIMESTAMPTZ,
    rotated_from       UUID REFERENCES api_key (id),
    CONSTRAINT api_key_prefix_unique UNIQUE (prefix),
    CONSTRAINT api_key_name_len CHECK (char_length(name) BETWEEN 1 AND 64),
    CONSTRAINT api_key_scopes_nonempty CHECK (cardinality(scopes) > 0),
    CONSTRAINT api_key_environment_valid CHECK (environment IN ('live', 'test')),
    CONSTRAINT api_key_status_valid CHECK (status IN ('active', 'revoked')),
    CONSTRAINT api_key_rate_limit_range CHECK (rate_limit_per_min BETWEEN 30 AND 600)
);

CREATE INDEX idx_api_key_club ON api_key (club_id, created_at DESC);
CREATE INDEX idx_api_key_active ON api_key (club_id) WHERE status = 'active';

-- Журнал вызовов Developer API: источник лимита и ленты «последние вызовы».
CREATE TABLE api_key_usage (
    id         BIGSERIAL PRIMARY KEY,
    api_key_id UUID NOT NULL REFERENCES api_key (id) ON DELETE CASCADE,
    club_id    UUID NOT NULL,
    method     TEXT NOT NULL,
    path       TEXT NOT NULL,
    status     SMALLINT NOT NULL,
    at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_api_key_usage_key ON api_key_usage (api_key_id, at DESC);
CREATE INDEX idx_api_key_usage_club ON api_key_usage (club_id, at DESC);

-- Replay ответов POST Developer API по Idempotency-Key (в пределах ключа).
CREATE TABLE developer_idempotency (
    api_key_id UUID NOT NULL REFERENCES api_key (id) ON DELETE CASCADE,
    idem_key   TEXT NOT NULL,
    response   JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (api_key_id, idem_key)
);

CREATE INDEX idx_developer_idempotency_created ON developer_idempotency (created_at);