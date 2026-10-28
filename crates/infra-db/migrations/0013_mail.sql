-- DROP IT — миграция 0013: транзакционная почта (Resend).
--
-- Зачем отдельные таблицы, а не строка в outbox:
--
--   * Письмо живёт дольше события. Outbox — «опубликовал и забыл»; письму
--     нужна история: когда поставили в очередь, когда провайдер принял,
--     когда доставлено, чем отскочило. Админка `/mail` показывает именно
--     эту историю, а не поток событий.
--   * У письма своя машина состояний
--     (queued → sending → sent → delivered / delivery_delayed / bounced /
--     complained / failed / suppressed), и переходы обязаны быть
--     монотонными: поздний `sent` не имеет права откатить `delivered`.
--     Гарантия живёт в SQL (guarded UPDATE … WHERE status IN (…)), а не в
--     коде воркера: два вебхука доставки приходят параллельно и
--     переупорядочиваются сетью.
--   * Повторная отправка — это НЕ «ещё одно событие», а новая строка с
--     новым idempotency_key: старый ключ провайдер уже видел, и он вернул
--     бы прежний ответ вместо нового письма.
--
-- idempotency_key UNIQUE — единственная защита от двойной отправки. Он же
-- уходит в Resend заголовком `Idempotency-Key`, поэтому повтор бизнес-
-- операции (дважды нажали «пригласить», ретрай после таймаута) не даёт
-- второе письмо. UNIQUE держит это на уровне БД, а не в голове воркера.
--
-- Ссылки на бизнес-сущности — мягкие (aggregate_type/aggregate_id без FK),
-- как в outbox/audit_log: письмо не блокирует удаление DJ/клуба/гостя и не
-- меняет порядок TRUNCATE в фикстурах e2e. По той же причине в
-- `test_repo()` (gateway-api/src/e2e.rs) эти таблицы не перечислены —
-- каждый тест почты чистит mail_message/mail_event/mail_suppression сам.
--
-- mail_suppression — «не отправлять этому адресу»: жёсткий bounce и
-- жалоба на спам портят репутацию домена отправителя, поэтому адрес
-- подавляется навсегда, пока оператор не снимет запрет руками
-- (reason='manual'). Подавление проверяет воркер перед отправкой; в БД
-- это просто справочник, а не триггер: письма, уже стоящие в очереди,
-- никуда не деваются.
--
-- Миграция аддитивная: только CREATE … IF NOT EXISTS. Существующие
-- таблицы, их колонки и порядок TRUNCATE не меняются (expand/contract).
-- CITEXT для адресов — чтобы `Ivan@Club.ru` и `ivan@club.ru` были одним
-- подавлением; расширение создано миграцией 0002.

CREATE TABLE IF NOT EXISTS mail_message (
    id               UUID PRIMARY KEY,                          -- UUIDv7, монотонно по времени
    idempotency_key  TEXT NOT NULL UNIQUE,                      -- он же Idempotency-Key у провайдера
    template         TEXT NOT NULL,                             -- 'dj_invite' | 'guest_verify' | 'payout_paid' | 'mail_test'
    to_email         CITEXT NOT NULL,
    to_name          TEXT,                                      -- отображаемое имя получателя, опц.
    from_email       TEXT NOT NULL,                             -- 'DROP IT <no-reply@…>'
    reply_to         TEXT,                                      -- опц. адрес ответа
    subject          TEXT NOT NULL,                             -- отрендерено при постановке в очередь
    html             TEXT NOT NULL,                             -- отрендерено при постановке в очередь
    text             TEXT NOT NULL,                             -- текстовая версия обязательна
    vars             JSONB NOT NULL DEFAULT '{}'::jsonb,         -- переменные шаблона (для resend/разбора)
    status           TEXT NOT NULL DEFAULT 'queued',
    provider         TEXT NOT NULL DEFAULT 'resend',
    provider_id      TEXT,                                      -- id письма у провайдера; ключ сшивки с вебхуками
    attempts         INTEGER NOT NULL DEFAULT 0,
    max_attempts     INTEGER NOT NULL DEFAULT 5,
    next_attempt_at  TIMESTAMPTZ NOT NULL DEFAULT now(),         -- когда строку можно забирать
    last_error       TEXT,                                      -- последняя причина сбоя/подавления
    aggregate_type   TEXT,                                      -- 'dj' | 'guest_account' | 'payout' (мягкая ссылка)
    aggregate_id     UUID,                                      -- без FK: сущность может быть удалена
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    sent_at          TIMESTAMPTZ,
    delivered_at     TIMESTAMPTZ,
    CONSTRAINT mail_message_status_check CHECK (status IN (
        'queued', 'sending', 'sent', 'delivered', 'delivery_delayed',
        'bounced', 'complained', 'failed', 'suppressed'
    ))
);

-- Очередь диспетчера: «что можно забрать прямо сейчас». Частичный индекс —
-- ровно под предикат claim_batch; терминальные строки в него не попадают,
-- поэтому индекс не растёт вместе с историей.
CREATE INDEX IF NOT EXISTS idx_mail_message_pending
    ON mail_message (next_attempt_at)
    WHERE status IN ('queued', 'failed');

-- Список админки и keyset-пагинация по (created_at DESC, id DESC).
CREATE INDEX IF NOT EXISTS idx_mail_message_created
    ON mail_message (created_at DESC, id DESC);

-- Поиск письма по provider_id при разборе вебхука доставки.
CREATE INDEX IF NOT EXISTS idx_mail_message_provider
    ON mail_message (provider_id)
    WHERE provider_id IS NOT NULL;

-- «Что отправляли этому DJ/гостю/выплате» — карточка в админке.
CREATE INDEX IF NOT EXISTS idx_mail_message_aggregate
    ON mail_message (aggregate_type, aggregate_id)
    WHERE aggregate_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS mail_event (
    id          UUID PRIMARY KEY,                               -- UUIDv7
    svix_id     TEXT NOT NULL UNIQUE,                           -- дедуп повторов вебхука (Svix доставляет at-least-once)
    event_type  TEXT NOT NULL,                                  -- 'email.delivered' и т.п. как прислал провайдер
    provider_id TEXT,                                           -- data.email_id; NULL, если событие без письма
    payload     JSONB NOT NULL,                                 -- сырое тело: провайдер добавляет поля без предупреждения
    received_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_mail_event_provider
    ON mail_event (provider_id, received_at DESC);

CREATE TABLE IF NOT EXISTS mail_suppression (
    email      CITEXT PRIMARY KEY,                              -- регистронезависимо: один адрес — одна запись
    reason     TEXT NOT NULL,                                   -- 'bounce' | 'complaint' | 'manual'
    detail     TEXT,                                            -- текст ошибки провайдера/кто снял
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
