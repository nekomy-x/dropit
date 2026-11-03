-- DROP IT — миграция 0021: заявки на публичный онбординг клуба.
--
-- Зачем: Connect-лендинг (`apps/web-club`, `TODO(club-onboarding-endpoint)`)
-- отправляет заявку на `POST /v1/club/onboarding/apply`, но публичного роута
-- не было и форма деградировала в 501. Заявка складывается сюда, разбирается
-- админом: approve создаёт клуб + master-токен, reject фиксирует причину.
--
-- ПРАВИЛА, которые держит схема:
--   * в строке НЕТ паролей и иных секретов заявки. Пароль нового клуба
--     задаётся отдельно при approve и попадает только в `club.password_hash`
--     (Argon2id), как у любого другого клуба;
--   * `club` / `venue` / `qr` — исходное тело заявки как JSONB: при разборе
--     ничего не теряется (district, website, genres, zones и т.п.);
--   * рядом с payload лежат нормализованные колонки для админского списка и
--     антифрода: `club_name`, `city`, `contact`, `email`, `price_minor`;
--     `email` заполняется ТОЛЬКО если `contact` — валидный e-mail;
--   * `idempotency_key` UNIQUE: повтор POST с тем же `Idempotency-Key`
--     возвращает ту же заявку (200), а не создаёт вторую. NULL допустим и
--     не участвует в UNIQUE (несколько заявок без ключа);
--   * `source_ip INET` — лимит 5 заявок/час на IP (TRUST_PROXY учитывается
--     в `routes::client_ip`); `0.0.0.0` — тот же сентинел
--     «неидентифицируемый клиент», что у `device_token_issue.ip`;
--   * `status` — ТОЛЬКО три значения: pending_moderation | approved |
--     rejected. CHECK не даёт кривому статусу протечь мимо админ-фильтра;
--   * `reject_reason` — 1..500 символов, только у rejected-заявки;
--   * `reviewed_by` — id админа из `admin_user`, `reviewed_at` — время
--     решения (NULL, пока заявка не разобрана).
--
-- DDL идемпотентен (IF NOT EXISTS): стенд применяет миграции как
-- `psql -f`, а воркер — через sqlx Migrator; повторный прогон не должен
-- падать на уже созданных объектах.

CREATE TABLE IF NOT EXISTS club_application (
    id              UUID PRIMARY KEY,                       -- UUIDv7
    club            JSONB NOT NULL,                          -- исходный клубный блок
    venue           JSONB NOT NULL,                          -- исходный блок площадки
    qr              JSONB NOT NULL,                          -- исходный QR-блок
    club_name       TEXT NOT NULL,                           -- нормализованное имя
    city            TEXT NOT NULL DEFAULT '',                -- venue.city, иначе club.city
    contact         TEXT NOT NULL,                           -- e-mail или телефон
    email           CITEXT,                                  -- если contact — e-mail
    price_minor     BIGINT NOT NULL,                         -- цена запроса, минорные единицы
    status          TEXT NOT NULL DEFAULT 'pending_moderation',
    reject_reason   TEXT,
    reviewed_by     UUID,
    reviewed_at     TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    idempotency_key TEXT,
    source_ip       INET NOT NULL,
    CONSTRAINT club_application_status_valid
        CHECK (status IN ('pending_moderation', 'approved', 'rejected')),
    CONSTRAINT club_application_idempotency_key_unique UNIQUE (idempotency_key),
    CONSTRAINT club_application_club_name_len
        CHECK (char_length(club_name) BETWEEN 1 AND 120),
    CONSTRAINT club_application_contact_len
        CHECK (char_length(contact) BETWEEN 1 AND 120),
    CONSTRAINT club_application_price_range
        CHECK (price_minor BETWEEN 100 AND 100000),
    CONSTRAINT club_application_reject_reason_len
        CHECK (reject_reason IS NULL OR char_length(reject_reason) BETWEEN 1 AND 500),
    CONSTRAINT club_application_reviewed_consistency
        CHECK ((status = 'pending_moderation') = (reviewed_at IS NULL))
);

-- Админский список: keyset по (created_at DESC, id DESC) с фильтром статуса.
CREATE INDEX IF NOT EXISTS idx_club_application_status_created
    ON club_application (status, created_at DESC, id DESC);

-- Антифрод-лимит 5 заявок/час на IP: count по окну времени.
CREATE INDEX IF NOT EXISTS idx_club_application_ip_created
    ON club_application (source_ip, created_at DESC);

-- Лимит 3 pending на один contact (регистр не важен: contact может быть
-- e-mail/телефоном, сравнение — lower(contact)).
CREATE INDEX IF NOT EXISTS idx_club_application_contact_pending
    ON club_application (lower(contact)) WHERE status = 'pending_moderation';
