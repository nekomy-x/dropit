-- DROP IT — миграция 0003: сессии вечера, QR, гранты гостей, тарифы (F2).
-- Модель — Часть II технической карты; стейт-машина session — Часть III.
--
-- ПРАВИЛА:
--   * QR-токен: 128 бит энтропии, в БД только sha256-хеш;
--     ротация на сессию (активен один токен, старые деактивируются);
--   * expires_at QR = плановое авто-закрытие сессии (не фикс. +24ч),
--     часовой пояс — IANA-зона клуба (DST-безопасность);
--   * session_guest — ГРАНТ входа: выдаётся в POST /sessions/join,
--     ОБЯЗАТЕЛЕН в POST /requests (F4). Клиентский session_id на веру
--     не принимается — защита от чужих сессий;
--   * pricing_policy: изменение = новая строка (valid_to у старой),
--     чтение FOR SHARE при создании запроса (защита от гонки смены тарифа).

-- ============================================================
-- Тарифы клуба: цена = заведение, комиссия = платформа.
-- Снимок правил фиксируется в каждом платеже (F5).
-- ============================================================
CREATE TABLE pricing_policy (
    id          UUID PRIMARY KEY,                    -- UUIDv7
    club_id     UUID NOT NULL REFERENCES club (id),
    price_minor BIGINT NOT NULL,                     -- цена запроса, минорные единицы (ADR-003)
    dj_share_bp INTEGER,                             -- NULL = дефолт платформы (platform_settings)
    valid_from  TIMESTAMPTZ NOT NULL DEFAULT now(),
    valid_to    TIMESTAMPTZ,                         -- NULL = действует; закрытие при создании новой
    created_by  UUID NOT NULL,                       -- admin_user.id или club.id
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_pricing_policy_club ON pricing_policy (club_id, valid_from DESC);

-- ============================================================
-- Сессия вечера: одна сессия = один клуб + один DJ (Q8: MVP).
-- Статусы: created → active → paused_acceptance ⇄ active → closed.
-- ============================================================
CREATE TABLE session (
    id                UUID PRIMARY KEY,              -- UUIDv7
    club_id           UUID NOT NULL REFERENCES club (id),
    dj_id             UUID REFERENCES dj (id),       -- NULL до назначения DJ
    status            TEXT NOT NULL DEFAULT 'created',
    pricing_policy_id UUID REFERENCES pricing_policy (id),
    opened_at         TIMESTAMPTZ,                   -- при active
    closed_at         TIMESTAMPTZ,                   -- при closed (вручную или авто-джоба 07:00)
    timezone          TEXT NOT NULL,                 -- IANA-зона (DST-безопасное авто-закрытие)
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_session_club ON session (club_id, created_at DESC);
CREATE INDEX idx_session_dj ON session (dj_id) WHERE dj_id IS NOT NULL;
CREATE INDEX idx_session_status ON session (status) WHERE status <> 'closed';

-- ============================================================
-- QR-токены сессии: 128 бит, хеш sha256, ротация.
-- active = TRUE только у текущего токена сессии.
-- ============================================================
CREATE TABLE qr_token (
    id           UUID PRIMARY KEY,                   -- UUIDv7
    session_id   UUID NOT NULL REFERENCES session (id),
    token_hash   TEXT NOT NULL,                      -- sha256 hex от plain-токена
    active       BOOLEAN NOT NULL DEFAULT TRUE,
    generated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ NOT NULL,               -- плановое авто-закрытие сессии
    UNIQUE (token_hash)
);

CREATE INDEX idx_qr_token_session ON qr_token (session_id, active);

-- Ротация: у сессии максимум один активный токен.
CREATE UNIQUE INDEX idx_qr_token_one_active ON qr_token (session_id) WHERE active;

-- ============================================================
-- Грант входа гостя: выдаётся при скане QR, обязателен для
-- POST /requests. UNIQUE(session_id, guest_device_id) — повторный
-- join той же сессии = re-grant, не дубль.
-- ============================================================
CREATE TABLE session_guest (
    id              UUID PRIMARY KEY,                -- UUIDv7
    session_id      UUID NOT NULL REFERENCES session (id),
    guest_device_id UUID NOT NULL REFERENCES guest_device (id),
    joined_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at      TIMESTAMPTZ,                     -- ручной ревок (антифрод, F8)
    UNIQUE (session_id, guest_device_id)
);

CREATE INDEX idx_session_guest_device ON session_guest (guest_device_id);
