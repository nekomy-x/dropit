# Аудит веб-поверхностей против бэкенда

Задача: проверить, что **весь** функционал веб-версии DROP IT
(`apps/web-club`, `apps/web-admin`, `apps/web-status`, `apps/web-dj`) реализован
на бэкенде, и закрыть найденные пробелы.

Дата: 2026-10-29.

## Отчёты

| Файл | Что покрывает |
|---|---|
| [`web-club-gaps.md`](./web-club-gaps.md) | панель клуба, Connect-лендинг, DJ-консоль, API-ключи, документация разработчика |
| [`web-admin-gaps.md`](./web-admin-gaps.md) | админ-панель: живые экраны + «Control center» на `localStorage` |
| [`web-status-gaps.md`](./web-status-gaps.md) | страница `/status/`, `apps/web-dj` (статический мок) |

Каждый отчёт содержит матрицу «экран → ожидаемый эндпоинт → статус →
доказательство (`файл:строка` с обеих сторон) → влияние (production/demo) →
severity», раздел расхождений полей и раздел «что нужно на бэкенде» с точной
формой эндпоинтов.

## Что было закрыто (2026-10-29)

Все новые маршруты описаны в `docs/openapi.yaml`; контрактный тест
(`cargo test -p dropit-gateway-api --test openapi_contract`) сверяет множества
путей/методов кода и спецификации в обе стороны.

### Онбординг клуба (был blocker: единственный `missing`)

| Метод | Путь | Что даёт |
|---|---|---|
| `POST` | `/v1/club/onboarding/apply` | публичная заявка с `/connect` и `/register` (валидация, лимит 5/час по IP, идемпотентность по `Idempotency-Key`) |
| `GET` | `/v1/club/onboarding/invite/{token}` | превью активации: название, город, e-mail из заявки |
| `POST` | `/v1/club/onboarding/invite/{token}/accept` | владелец задаёт пароль → `TokenPair`, клуб становится `active` |
| `GET` | `/v1/admin/onboarding-applications` | очередь заявок (keyset, фильтр по статусу) |
| `GET` | `/v1/admin/onboarding-applications/{id}` | полная заявка + состояние приглашения |
| `POST` | `/v1/admin/onboarding-applications/{id}/approve` | создаёт клуб без пароля + одноразовый invite (128 бит, sha256, TTL 7 дней) + `qr_master_token` |
| `POST` | `/v1/admin/onboarding-applications/{id}/reject` | отклонение с причиной |
| `POST` | `/v1/admin/onboarding-applications/{id}/invite` | перевыпуск ссылки активации |

Контракт целиком: [`docs/onboarding-contract.md`](../onboarding-contract.md).
Миграции: `0021_onboarding.sql`, `0022_club_invite.sql`.

### Админ-панель: каталог пользователей, роли, обзор, флаги, выгрузка

| Метод | Путь | Что даёт |
|---|---|---|
| `GET`/`POST` | `/v1/admin/users` | каталог principal-ролей (admin/club_staff/dj/guest), создание администратора с TOTP-enrollment |
| `GET`/`PATCH`/`DELETE` | `/v1/admin/users/{id}` | карточка, правка роли/статуса, мягкое удаление с отзывом refresh-токенов |
| `POST` | `/v1/admin/users/bulk` | массовые approve/suspend/block/role одной транзакцией |
| `GET` | `/v1/admin/roles` | матрица прав, выведенная из фактического кода (`enforced_by`) |
| `GET` | `/v1/admin/overview` | реальные агрегаты обзора + серии запросов/выручки (24/7/30 точек) |
| `GET`/`PATCH` | `/v1/admin/feature-flags`, `/v1/admin/feature-flags/{key}` | фича-флаги платформы (таблица `feature_flag`, аудит) |
| `GET` | `/v1/admin/export` | выгрузка users/clubs/djs/payments в CSV (RFC 4180) или JSON |

Плюс расширены существующие админские роуты: `GET`/`DELETE /v1/admin/clubs/{id}`,
`POST`/`PATCH`/`DELETE /v1/admin/djs/{id}` и `POST /v1/admin/djs/{id}/restore`,
`PATCH /v1/admin/guests/{id}` (бан), а в ответы добавлены поля, которые UI уже
читал: Stripe-статус клубов и DJ, `refunded_minor`/`failure_reason`/`captured_at`/
`currency` платежей, `diff` и пагинация журнала аудита.

### Деньги

| Метод | Путь | Что даёт |
|---|---|---|
| `POST` | `/v1/admin/payments/{id}/refund` | возврат у провайдера + одна транзакция БД: `refunded_minor`, пропорциональный реверс сплитов (Σ == сумма возврата), ledger `reversal` (Σ по txn == 0), outbox, аудит; идемпотентность по `Idempotency-Key` |

До этого кнопка «refund» в админ-консоли была фикцией: она меняла строку в
`localStorage`, а деньги не возвращались.

### DJ-консоль

| Метод | Путь | Что даёт |
|---|---|---|
| `GET` | `/v1/dj/history` | история сыгранных заявок смены (keyset-пагинация) — раньше «Played» и CSV всегда были нулевыми |
| `GET` | `/v1/dj/stats/session` | счётчики открытой смены (не «за всё время») |
| `GET` | `/v1/dj/invite/{token}` | превью приглашения DJ; письмо ведёт на рабочую ссылку |

### Каталог треков

`GET /v1/catalog/search` монтировался публично, но не проверял вызывающего:
любой мог расходовать квоту внешнего каталога. Теперь хендлер принимает JWT
роли `guest` либо выданный device-token (`Bearer <uuid из guest_device>`),
остальным — 403/401. Мобильный клиент уже шлёт device-token, поэтому его путь
не изменился.

### Страница статуса

Контракт `/v1/status/services` был полным, но деградация была нечестной:
строка `payments` всегда `ok` (даже при мёртвой БД), а Redis-проверка без
таймаута могла подвесить ответ. Теперь `payments` берётся из проверки
(`degraded` при недоступной БД, `unknown` для live-провайдера без проба),
все проверки параллельны, а `deploy.sh` и `verify-stand.mjs` проверяют саму
страницу и её эндпоинт.

## Что осознанно осталось локальным

Это не пробелы бэкенда, а решения с честной пометкой в UI:

- зоны/заполненность на доске клуба — блок `preview` с баннером
  (`docs/ui/club-redesign/01-contract.md` §4);
- телеметрия DJ-консоли (чаевые, now-playing) — «недоступно», без выдуманных
  нулей: эндпоинтов нет;
- дизайн QR-табличек (`localStorage`) — общий доступ не нужен, пока клуб один
  на браузер;
- `apps/web-dj` — статический дизайн-прототип, публикуется как `/dj-mock/` и
  намеренно не ходит в API.
