-- DROP IT — миграция 0010: аккаунт гостя (опц. апгрейд e-mail).
--
-- Техкарта Часть IV: POST /v1/guest/upgrade { email } → привязка аккаунта.
-- guest_device.account_id заведён миграцией 0002 как UUID без FK
-- (плейсхолдер «аккаунт опц.»); здесь вводится таблица аккаунтов и
-- уникальность по нормализованному e-mail (trim + lowercase).
--
-- ПРАВИЛА:
--   * e-mail хранится в двух видах: email_normalized (UNIQUE, нижний регистр,
--     trim) — для уникальности и сравнения; email_display — исходный ввод;
--   * verified = FALSE (флоу верификации — за рамками пакета W2-B);
--   * FK guest_device.account_id → guest_account(id) НЕ добавляется:
--     колонка существовала без FK с F1 (soft-reference сохранена, чтобы
--     не ломать порядок TRUNCATE в фикстурах e2e);
--   * CITEXT не используется: нормализация делается в приложении, чтобы
--     правило «trim + lowercase» было единственным источником истины.

CREATE TABLE IF NOT EXISTS guest_account (
    id               UUID PRIMARY KEY,                 -- UUIDv7
    email_normalized TEXT NOT NULL UNIQUE,             -- trim + lowercase
    email_display    TEXT NOT NULL,                    -- исходный ввод (trim)
    verified         BOOLEAN NOT NULL DEFAULT FALSE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_guest_account_normalized ON guest_account (email_normalized);
