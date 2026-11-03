-- DROP IT — миграция 0023: индекс для replay возвратов.
--
-- Зачем: идемпотентность возврата платежа (`POST /v1/admin/payments/:id/refund`)
-- хранится в самой записи outbox `payment.refunded` — она пишется в одной
-- транзакции с изменением денег, поэтому «запись есть» ⇔ «возврат применён».
-- Replay ищет запись по `payload->>'idempotency_key'`, а таблица `outbox`
-- растёт вместе с потоком событий: без индекса поиск превращается в скан.
--
-- Индекс partial: интересуют только записи возвратов, остальные события
-- (request.created, session.opened, …) в нём не участвуют и не раздувают его.

CREATE INDEX IF NOT EXISTS idx_outbox_refund_idempotency
    ON outbox ((payload ->> 'idempotency_key'))
    WHERE event_type = 'payment.refunded';
