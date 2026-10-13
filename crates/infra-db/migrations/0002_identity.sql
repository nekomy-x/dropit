-- DROP IT — миграция 0002: идентичность и роли (F1).
-- Модель — Часть II технической карты; правила — Часть III/«Роли и идентичность».
--
-- ПРАВИЛА:
--   * пароли — Argon2id (хеш в приложении, в БД только hash);
--   * TOTP-секреты шифруются приложением (totp_secret_enc);
--   * guest_device — без регистрации, device-token = UUID v7;
--   * email — CITEXT (безрегистровое сравнение);

CREATE EXTENSION IF NOT EXISTS citext;
--   * refresh-токены — ротация: каждый refresh аннулирует предыдущий
--     (family), кража детектируется по reuse → ревок всей семьи.

-- ============================================================
-- Гость: устройство = идентичность. Аккаунт (email) — опц. апгрейд.
-- ============================================================
CREATE TABLE guest_device (
    id          UUID PRIMARY KEY,                    -- UUIDv7: сам device-token
    account_id  UUID,                                -- NULL пока гость без аккаунта
    platform    TEXT NOT NULL,                       -- 'ios' | 'android' | 'web'
    app_version TEXT,
    first_seen  TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_guest_device_account ON guest_device (account_id) WHERE account_id IS NOT NULL;

-- ============================================================
-- Диджей: приглашение клубом; club_id NULL = фрилансер (v2).
-- ============================================================
CREATE TABLE dj (
    id                       UUID PRIMARY KEY,
    club_id                  UUID,
    email                    CITEXT NOT NULL,
    password_hash            TEXT NOT NULL,
    totp_secret_enc          BYTEA,                  -- NULL = 2FA не настроена
    totp_confirmed_at        TIMESTAMPTZ,
    display_name             TEXT NOT NULL,
    stripe_account_id        TEXT,
    stripe_onboarding_status TEXT NOT NULL DEFAULT 'pending',
    status                   TEXT NOT NULL DEFAULT 'active',   -- active/suspended/blocked
    created_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (email)
);
CREATE INDEX idx_dj_club ON dj (club_id);

-- ============================================================
-- Клуб: регистрация + верификация админом.
-- ============================================================
CREATE TABLE club (
    id                       UUID PRIMARY KEY,
    name                     TEXT NOT NULL,
    slug                     TEXT NOT NULL,
    address                  TEXT,
    country                  TEXT,                    -- ISO-3166 alpha-2, Q13
    email                    CITEXT NOT NULL,
    phone                    TEXT,
    logo_url                 TEXT,
    password_hash            TEXT NOT NULL,
    totp_secret_enc          BYTEA,
    totp_confirmed_at        TIMESTAMPTZ,
    stripe_account_id        TEXT,
    stripe_onboarding_status TEXT NOT NULL DEFAULT 'pending',
    status                   TEXT NOT NULL DEFAULT 'pending_verification', -- pending_verification/active/suspended/blocked
    created_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (email),
    UNIQUE (slug)
);

-- ============================================================
-- Админ: создаётся вручную, 2FA обязательна.
-- ============================================================
CREATE TABLE admin_user (
    id                UUID PRIMARY KEY,
    email             CITEXT NOT NULL,
    password_hash     TEXT NOT NULL,
    totp_secret_enc   BYTEA NOT NULL,               -- 2FA обязательна с F1
    totp_confirmed_at TIMESTAMPTZ NOT NULL,
    role              TEXT NOT NULL DEFAULT 'superadmin',  -- superadmin/operator/readonly
    status            TEXT NOT NULL DEFAULT 'active',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (email)
);

-- ============================================================
-- Refresh-токены: ротация с семьями (detection reuse).
-- access JWT — 15 мин, stateless; refresh — 30 дней, stateful.
-- ============================================================
CREATE TABLE refresh_token (
    id          UUID PRIMARY KEY,
    family_id   UUID NOT NULL,                      -- одна семья = один логин
    subject_type TEXT NOT NULL,                     -- 'dj' | 'club' | 'admin'
    subject_id  UUID NOT NULL,
    token_hash  TEXT NOT NULL,                      -- sha256(token)
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL,
    revoked_at  TIMESTAMPTZ,
    replaced_by UUID,                               -- токен, пришедший на смену
    UNIQUE (token_hash)
);

CREATE INDEX idx_refresh_family ON refresh_token (family_id);
CREATE INDEX idx_refresh_subject ON refresh_token (subject_type, subject_id);

-- ============================================================
-- Логины: rate-limit 5/15 мин (таблица, не Redis — проще и надёжно
-- для F1; Redis-счётчики — F8 антифрод).
-- ============================================================
CREATE TABLE login_attempt (
    id          BIGSERIAL PRIMARY KEY,
    subject_type TEXT NOT NULL,
    email       CITEXT NOT NULL,
    ip          INET,
    ok          BOOLEAN NOT NULL,
    at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_login_attempt_lookup ON login_attempt (subject_type, email, at DESC);

-- Выдача device-token: 5/час/IP (выше — captcha; проверка в приложении,
-- счётчик здесь для аудита и простого лимита).
CREATE TABLE device_token_issue (
    id  BIGSERIAL PRIMARY KEY,
    ip  INET NOT NULL,
    at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_device_token_issue ON device_token_issue (ip, at DESC);
