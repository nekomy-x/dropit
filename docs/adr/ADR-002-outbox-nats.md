# ADR-002: Transactional outbox + NATS JetStream

**Статус:** реализовано (публикатор `OutboxRelay` + воркер `dropit-workers`)
**Дата:** 2026-10-12
**Реализовано:** 2026-10-19

## Решение

Изменение состояния денег и публикация события — **одна транзакция БД** через таблицу `outbox` (id, aggregate_type, aggregate_id, event_type, payload, created_at, published_at, attempts). Реле-воркер читает outbox и публикует в NATS JetStream (durable-консьюмеры, manual ack, max-deliver → DLQ).

Обработчики идемпотентны по `(aggregate_id, event_type)`; повторная доставка — no-op, не ошибка.

## Следствия

- Событие физически не может потеряться при падении процесса между commit и publish.
- До F5 NATS не поднимается: outbox вводится миграцией вместе с платежами (не создаём инфраструктуру «на будущее»).
- Локальный Compose включает NATS только с F5.

## Реализация

Таблица `outbox` живёт в `infra-db` (миграция `0004_requests.sql`); транспорт и
реле — в `crates/infra-nats`, запуск — в воркере `crates/workers`.

- **Хранилище**: `OutboxStore` (трейт) + `PgOutboxStore` (Postgres, `SKIP LOCKED`
  батч-захват). Поля строки: `id, aggregate_id, event_type, payload, attempts`.
- **Шина**: `MessageBus` (трейт); `NatsBus` (async-nats, JetStream-стрим
  `DROPIT`, subject `dropit.outbox.<event_type>`) и `NoopBus` (без `NATS_URL`).
- **Реле**: `OutboxRelay::run_once` — цикл `claim → publish → mark`:
  успешно опубликованные помечаются пакетно (`mark_published`), упавшие —
  `mark_failed` (`attempts++`, причина сохраняется), строки с
  `attempts >= max_attempts` — `skipped` (poisoned). Инвариант отчёта:
  `claimed == published + failed + skipped`. Частичный сбой одной строки
  изолируется — остаток батча обрабатывается.
- **Воркер**: `dropit-workers run` крутит реле по `OUTBOX_INTERVAL_SECS`
  (дефолт 2 с); `dropit-workers --once outbox` — один прогон (CI/крон).
- **Доставка at-least-once**: если `mark_published` упадёт после успешной
  публикации, строка переиздаётся следующим циклом → потребители обязаны быть
  идемпотентными по `(aggregate_id, event_type)` (см. «Решение»).
- **Деградация**: без `NATS_URL` — `NoopBus`, реле пропускается (статус
  `skipped`, код 0), события копятся в `outbox` и публикуются при появлении
  NATS. Без `DATABASE_URL` — БД-задачи воркера пропускаются.

Покрытие: `crates/infra-nats/tests/outbox_relay.rs` (батч, дубль, ошибка
публикации, исчерпание попыток) + unit-тесты `subject_for`/`OutboxRow`.

## Альтернативы

- Kafka: избыточна для MVP (объём/операционная стоимость).
- Прямая публикация из запроса (без outbox): теряются события при падении между commit и publish — недопустимо для денег.
