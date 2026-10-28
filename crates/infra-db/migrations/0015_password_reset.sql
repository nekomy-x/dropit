-- DROP IT — миграция 0015: сброс пароля клуба / DJ / админа.
--
-- Номер 0015, а не 0013 из первоначального плана: 0013 (почта) и 0014
-- (подтверждение e-mail гостя) уже заняты соседними изменениями, а
-- sqlx-мигратор держит версию как PRIMARY KEY `_sqlx_migrations` — два
-- файла с одним номером ломают прогон миграций. Порядок применения =
-- порядок имён файлов.
--
-- Зачем таблица, а не память процесса:
--
--   * токен обязан пережить рестарт gateway: пользователь может открыть
--     письмо через 20 минут после запроса, а процесс к этому моменту уже
--     перезапущен (деплой, OOM, несколько узлов за балансировщиком);
--   * «одноразовость» и «срок годности» должны проверяться атомарно. Это
--     делает SQL (`UPDATE … WHERE used_at IS NULL AND expires_at > now()
--     RETURNING …`), а не код: две одновременные отправки формы с одним
--     токеном не могут обе выиграть — вторая увидит 0 строк;
--   * лимит «не больше 5 запросов на e-mail и на IP в час» считается по
--     этой же таблице, отдельного счётчика не заводим.
--
-- Хранится ТОЛЬКО хеш: `sha256(hex)`, как у refresh-токенов (0002). Сырой
-- токен живёт лишь в ответе 202 (Test Mode, `DROPIT_RESET_REVEAL_TOKEN`) и
-- в info-логе; утечка дампа БД не даёт возможности сбросить чужой пароль.
--
-- subject_id — мягкая ссылка (без FK): токен не должен блокировать
-- удаление DJ/клуба и не меняет порядок TRUNCATE в фикстурах e2e.
--
-- TTL — 30 минут (задаётся приложением при вставке), single use —
-- `used_at`. Просроченные/использованные строки не удаляются: это след
-- для аудита и основание для лимита.
--
-- Миграция аддитивная: только CREATE … IF NOT EXISTS. Существующие
-- таблицы, их колонки и порядок TRUNCATE не меняются (expand/contract).

CREATE TABLE IF NOT EXISTS password_reset_token (
    id           UUID PRIMARY KEY,                       -- UUIDv7: монотонно по времени
    subject_type TEXT NOT NULL,                          -- 'club' | 'dj' | 'admin'
    subject_id   UUID NOT NULL,                          -- без FK: сущность может быть удалена
    token_hash   TEXT NOT NULL,                          -- sha256(hex) сырого токена
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ NOT NULL,                   -- created_at + 30 минут
    used_at      TIMESTAMPTZ,                            -- NULL = ещё не погашен (single use)
    request_ip   INET,                                   -- IP запроса (NULL, если не определён)
    UNIQUE (token_hash)
);

-- Поиск «есть ли живой токен у субъекта» и счётчик лимита по e-mail:
-- find_password_subject → (subject_type, subject_id) → count за час.
CREATE INDEX IF NOT EXISTS idx_password_reset_subject
    ON password_reset_token (subject_type, subject_id, created_at DESC);

-- Счётчик лимита по IP: 5 запросов в час с одного адреса.
CREATE INDEX IF NOT EXISTS idx_password_reset_ip
    ON password_reset_token (request_ip, created_at DESC);