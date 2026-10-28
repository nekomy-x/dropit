-- DROP IT — миграция 0014: подтверждение e-mail гостя (ADR-008).
--
-- Колонка `guest_account.verified` заведена миграцией 0010 и до сих пор
-- всегда FALSE: «флоу верификации — за рамками пакета W2-B». Появление
-- почтового транспорта закрывает эту дырку: письмо с кодом уходит при
-- апгрейде гостя, код подтверждается эндпоинтом и только тогда
-- `verified = TRUE`.
--
-- ПРАВИЛА:
--   * код НЕ хранится в открытом виде: только SHA-256 от
--     `"{account_id}:{code}"` (hex). Утечка таблицы не даёт кода;
--   * один активный код на аккаунт (PRIMARY KEY по account_id):
--     повторная привязка e-mail перезаписывает код и обнуляет попытки;
--   * срок жизни — 15 минут; `expires_at` проверяется в SQL, не в коде;
--   * `attempts` ограничивает перебор: 5 неверных попыток гасят код
--     (строка удаляется), новый можно получить только новым апгрейдом;
--   * FK на `guest_account` НЕ добавляется — как и в 0010, ссылки в этом
--     проекте мягкие, чтобы не менять порядок TRUNCATE в фикстурах e2e.
--
-- Только CREATE TABLE/INDEX: существующие колонки и UNIQUE не меняются.

CREATE TABLE IF NOT EXISTS guest_email_verification (
    account_id  UUID PRIMARY KEY,                  -- guest_account(id), soft reference
    code_hash   TEXT        NOT NULL,              -- sha256("{account_id}:{code}") в hex
    expires_at  TIMESTAMPTZ NOT NULL,
    attempts    INTEGER     NOT NULL DEFAULT 0,    -- неверные попытки (порог — в коде)
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Уборка просроченных кодов (retention/крон): индекс по времени.
CREATE INDEX IF NOT EXISTS idx_guest_email_verification_expires
    ON guest_email_verification (expires_at);
