-- DROP IT — миграция 0008: панельные хвосты (пакет W2-C).
--
-- Дополняет схему под недостающие эндпоинты панелей (web-club, web-admin):
--   * dj: одноразовые invite-токены (sha256-хеш, TTL 7 дней) —
--     переиспользуется подход хеширования токенов из auth/identity
--     (token_hash TEXT, sha256-hex от plain-токена), отдельный столбец,
--     НЕ пароль; DJ создаётся в статусе 'invited', токен отдаётся один раз;
--   * club: city/timezone (профиль + дефолтная IANA-зона для сессий),
--     qr_master_token_hash — постоянный токен входа клуба (хеш, сам токен
--     отдаётся один раз при создании клуба админом);
--   * guest_device: display_name/email/banned — для /v1/admin/guests
--     (read + block/suspend). email/display_name NULLABLE — заполняются
--     при апгрейде гостя до аккаунта; banned — глобальный бан устройства.
--
-- Только ADD COLUMN/INDEX — существующие колонки и ограничения не трогаются,
-- публичный контракт и порядок TRUNCATE в фикстурах e2e сохраняются.

-- ============================================================
-- DJ: invite-токены (одноразовые, TTL 7 дней, хеш sha256-hex).
-- ============================================================
ALTER TABLE dj ADD COLUMN IF NOT EXISTS invite_token_hash   TEXT;
ALTER TABLE dj ADD COLUMN IF NOT EXISTS invite_expires_at    TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_dj_invite_token
    ON dj (invite_token_hash) WHERE invite_token_hash IS NOT NULL;

-- ============================================================
-- Клуб: city/timezone профиля + постоянный master-токен входа.
-- ============================================================
ALTER TABLE club ADD COLUMN IF NOT EXISTS city                TEXT;
ALTER TABLE club ADD COLUMN IF NOT EXISTS timezone            TEXT;
ALTER TABLE club ADD COLUMN IF NOT EXISTS qr_master_token_hash TEXT;

-- ============================================================
-- Гость: профиль (display_name/email) и глобальный бан для админ-панели.
-- ============================================================
ALTER TABLE guest_device ADD COLUMN IF NOT EXISTS display_name TEXT;
ALTER TABLE guest_device ADD COLUMN IF NOT EXISTS email        CITEXT;
ALTER TABLE guest_device ADD COLUMN IF NOT EXISTS banned       BOOLEAN NOT NULL DEFAULT FALSE;
