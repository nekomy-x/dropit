# DROP IT — план полной доводки бэкенда (fleet plan)

Статус: рабочий документ. Составлен по факту кода на `aee5d71` (2026-10-15).

## 0. Цель и границы

**Цель:** бэкенд = контракт техкарты + ADR-001…006 выполнен целиком, без
«in-memory вместо Redis», без «mock вместо Stripe», без пропавших эндпоинтов,
с событийной шиной и воркерами.

**В границах:** `crates/*`, `deploy/*`, `docs/openapi.yaml`, ADR, миграции,
тесты, `IMPLEMENTATION_STATUS.md`.

**Вне границ:** Flutter-приложения, веб-панели, дизайн, лендинг. Панели и
гостевые экраны только **потребляют** контракт — их правка отдельным этапом.

**Жёсткие инварианты (нарушать нельзя):**
1. Деньги — целые минорные единицы, `compute_split` без изменений (ADR-003).
2. Идемпотентность вебхуков по `(aggregate_id, event_type)`, не по `event_id`.
3. Ошибки — только конверт `ApiError` → `{code, message, details}`.
4. RBAC — middleware `role_route`, ни один новый роут не выпадает из гейта.
5. Тесты БД-зависимые — `--test-threads=1`, фикстуры чистятся в `test_repo()`.
6. Живых сетевых вызовов в тестах нет: Stripe/NATS/FCM — за трейтами,
   по умолчанию noop/mock.

## 1. Базовая линия (что уже есть)

| Что | Факт |
|---|---|
| Роутов в бинарнике | 53 (46 путей), OpenAPI — 46 путей: **дрейфа нет** |
| Миграции | 0001–0007 |
| Тесты | `cargo test --workspace` — 80 passed |
| Домен | payment/payout/request/session/qr/catalog/fraud — есть |
| Пробелы | см. §2 |

## 2. Реальные пробелы (по техкарте, проверено по коду)

### 2.1 Отсутствующие эндпоинты
| Техкарта | Статус |
|---|---|
| `GET /v1/sessions/current` | нет |
| `GET /v1/my/requests/:id` (позиция + same-song count) | нет |
| `GET /v1/catalog/search?q=&style=` | **нет** (поиск в гостевом приложении — демо) |
| `POST /v1/guest/upgrade` (bind email) | нет |
| `PATCH /v1/dj/styles` | нет |
| `POST /v1/club/djs/invite` | нет |
| `GET /v1/club/stripe/onboarding-status` | нет |
| `POST /v1/club/stripe/onboarding-link` | нет |
| `POST /v1/admin/clubs` (создание клуба) | нет |
| `GET /v1/admin/guests` | нет |
| `GET /v1/admin/reconciliation-runs` | нет |
| `POST /v1/guest/push-token` | нет |

### 2.2 Заглушки под видом готового
* WS-тикеты — `Mutex<HashMap>` в процессе (`requests_routes.rs:575`), не Redis.
* WS-хаб — `tokio::broadcast` в одном узле, мультиузла нет.
* `outbox_event` — таблица есть, **публикатора нет** (NATS не подключён, ADR-002 не реализован).
* Платежи/выплаты — `MockStripe`: `tr_mock_…`, вебхук-подпись не проверяется по-настоящему.
* Каталог — только парсер `Source`, внешних источников нет (B5 открыт).
* Push — нет вообще (FCM/APNs из техкарты).
* Воркеры — нет: payout/reconcile/retention/auto-close живут хендлерами «на кнопку».
* Redis — есть в compose, **не используется** в коде.
* Бинарник — один (`gateway-api`) вместо 5 по техкарте.

## 3. Волны флота

Правила: 4–6 агентов на волну, у каждого — **эксклюзивные файлы**, у волны —
общий гейт (см. §5). Между волнами барьер.

### Волна 1 — инфраструктурные крейты (5 агентов, конфликтов нет)
Каждый агент владеет **одним новым крейтом целиком**.

| Пакет | Крейт | Содержание | Приёмка |
|---|---|---|---|
| W1-A | `crates/infra-redis` | пул соединений; `TicketStore` (одноразовые WS-тикеты, GETDEL+TTL) + `RedisTicketStore`/`InMemoryTicketStore`; `EventBus` pub/sub + `RedisEventBus`/`InMemoryEventBus` | unit-тесты на in-memory; интеграционный тест под env `REDIS_URL` (локально Redis есть) |
| W1-B | `crates/infra-nats` | `OutboxStore`+`MessageBus` трейты; `OutboxRelay::run_once` (SKIP LOCKED → publish → mark, батчи, ретраи, backoff); `NatsBus` (async-nats), `NoopBus` | тесты релиза на фейковом сторе: батч, дубль, ошибка публикации |
| W1-C | `crates/infra-stripe` | REST-клиент: PaymentIntent create/capture/refund, Connect account+account_link, transfer, idempotency-key; **проверка подписи** `t=,v1=` (HMAC-SHA256, tolerance 300s); трейт `StripeApi`, `MockStripe`, `LiveStripe` | тесты: подпись (валид/просрочена/битая), маппинг ошибок 402/409/429, сериализация запросов |
| W1-D | `crates/infra-catalog` | iTunes Search + Deezer клиенты, `canonical_key`, ISRC-валидация, TTL-кеш-политика, маппинг стилей; трейт `CatalogSearch`, `MockCatalog` | тесты на записанных JSON-фикстурах (без сети) |
| W1-E | `crates/infra-push` | `PushSender` трейт, FCM v1 (OAuth2 JWT), APNs (ES256 JWT), `NoopPush`, дедуп/ретраи, отчёт по токенам | тесты на формирование запросов/подписи, без сети |

Гейт волны: `cargo build --workspace`, `cargo test -p infra-redis -p infra-nats
-p infra-stripe -p infra-catalog -p infra-push`, `clippy` чисто.

### Волна 2 — эндпоинты и хранилища (4 агента, файлы размечены)

| Пакет | Эксклюзивные файлы | Работа |
|---|---|---|
| W2-A | `sessions_routes.rs`, `infra-db/src/sessions.rs` | `GET /v1/sessions/current` |
| W2-B | `requests_routes.rs`, `infra-db/src/requests.rs` | `GET /v1/my/requests/:id` (позиция, same-song), `POST /v1/guest/upgrade` |
| W2-C | `panel_routes.rs` | invite DJ, stripe onboarding status/link, create club, admin guests, reconciliation-runs; миграция `0008` |
| W2-D | `routes.rs` | `PATCH /v1/dj/styles`, `POST /v1/guest/push-token`, `catalog_routes.rs` + `GET /v1/catalog/search`, регистрация роутеров, миграция `0009` |

Требование к W2: каждый новый роут — e2e-тест в своём `e2e_*.rs` (создаёт
владелец соответствующего модуля роутов).

### Волна 3 — интеграция (3 агента)

| Пакет | Файлы | Работа |
|---|---|---|
| W3-A | `ws.rs`, `routes.rs` | Redis-тикеты вместо HashMap, мультиузловой relay через `EventBus`, деградация в in-memory без Redis |
| W3-B | `payments_routes.rs`, `payouts_routes.rs`, `payments.rs` | `LiveStripe` по env, реальная проверка подписи вебхуков, onboarding-link, transfer при payout; mock остаётся дефолтом |
| W3-C | новый `crates/workers` | бинарник: outbox relay, payout cron, reconcile cron, retention/GDPR purge, auto-close сессий, push-диспетчер; `--once` для CI |

### Волна 4 — контракт, документация, верификация (2 агента)

| Пакет | Файлы | Работа |
|---|---|---|
| W4-A | `docs/openapi.yaml`, `docs/adr/*`, `IMPLEMENTATION_STATUS.md` | контракт на 100%: все новые пути/схемы; ADR-002/004/006 → реализовано; статус без противоречий |
| W4-B | весь `crates/*` (только фиксы) | `fmt`, `clippy -D warnings`, полный прогон тестов, contract-тест «каждый путь OpenAPI есть в роутере», отчёт |

## 4. Порядок и зависимости

```
W1 (5 крейтов) ──► ApiEnv расширяется (делает оркестратор) ──►
W2 (4 эндпоинта) ──► W3 (интеграция: ws/stripe/workers) ──► W4 (контракт+верификация)
```

## 5. Гейты приёмки (для каждой волны)

1. `cargo build --workspace` — без ошибок.
2. `cargo test -p <крейты волны>` — зелено; БД-зависимые — по одному прогону.
3. `cargo clippy --workspace --all-targets -- -D warnings` — чисто.
4. Ни один существующий тест не удалён и не помечен `ignored`.
5. Публичный контракт не ломается (поля только добавляются).

## 6. Риски и как снимаем

| Риск | Снятие |
|---|---|
| Параллельные правки одного файла | жёсткая разметка владения (§3), новые модули вместо правки чужих |
| Гонка e2e на общей БД | агенты волн 1–3 гоняют только unit-тесты своих крейтов; полный прогон — один агент W4-B |
| Живой Stripe/NATS недоступен | трейты + `Mock*`/`Noop*`, live-путь включается env-переменной |
| Внешние API каталога | фикстуры, live — за env-флагом |
| Разбухание диффа | один пакет = один крейт/модуль; запрет «попутных рефакторингов» |

---

## 7. Статус выполнения (по факту, 2026-10-19)

План доведён до конца за семь волн. Все пакеты W1–W7 закрыты.

| Волна | Пакеты | Что дали | Статус |
|---|---|---|---|
| **W1** | W1-A…W1-E | 5 инфра-крейтов: `infra-redis`, `infra-nats`, `infra-stripe`, `infra-catalog`, `infra-push` (трейты + live/mock/noop-адаптеры, unit + integration-тесты) | ✅ закрыто |
| **W2** | W2-A…W2-D | эндпоинты: `sessions/current`, `my/requests/{id}`, `guest/upgrade`, `dj/styles`, `club/djs/invite`, `club/stripe/*`, `admin/clubs`, `admin/guests`, `admin/reconciliation-runs`, `guest/push-token`, `catalog/search`; миграции `0008`–`0009` | ✅ закрыто |
| **W3** | W3-A…W3-C | Redis-тикеты + мультиузловой WS-relay (`EventBus`); `LiveStripe` по env + проверка подписи вебхуков; бинарник `dropit-workers` (outbox, payouts, reconcile, retention, close-sessions, push; `--once`) | ✅ закрыто |
| **W4** | W4-A, W4-B | OpenAPI 0.7.0 — 65 путей, дрейфа нет (контракт-тест зелёный); ADR-002/004/006 → реализовано; `clippy --workspace --all-targets` — 0 warning; `cargo build --workspace` — зелёный | ✅ закрыто |
| **W6** | W6-A…W6-C | OpenAPI догнан до кода (65 = 65), убрано мёртвое поле `ApiEnv.push`, документация приведена к факту; повторное adversarial-ревью нашло 3 недозакрытые гонки | ✅ закрыто |
| **W7** | W7-A…W7-C | закрыты гонки: параллельный confirm отменял подтверждённый платёж; `ON CONFLICT` отдавал чужой платёж другому request; двойная выплата без блокировки сплитов; IP-антифрод переведён на безопасный дефолт `TRUST_PROXY` | ✅ закрыто |
| **W5** | hardening | 8 критических дефектов закрыты: IDOR в confirm, подпись transfer-вебхука, транзакционная выплата, QR-заглушка → 128-бит, брутфорс 2FA → rate-limit, хардкод IP → `TRUST_PROXY`+`client_ip`, дубли заявок → Idempotency-Key replay, двойной платёж → идемпотентный confirm; эндпоинт `auth/club/master`; миграция `0010` | ✅ закрыто |

**Итог по метрикам:** 10 крейтов, 2 бинарника (`dropit-api`, `dropit-workers`) + `seed_demo`, 10 миграций, 65 HTTP-путей (бинарник = OpenAPI), **415 тестов / 0 failed**, clippy 0 warning.

**Что осталось (по факту, без обещаний):**
- Stripe **live** — `LiveStripe` написан, подпись проверяется, но схема Connect (destination charges vs separate transfers) не утверждена → реальные деньги не проводятся (ADR-004/Q15).
- Каталог **live** — транспорт iTunes/Deezer готов, права на метаданные/превью/обложки в коммерческом продукте не проверены (B5).
- NATS **live** — `NatsBus` написан, но проверен только на `NoopBus`; живой JetStream не гонялся.
- Push **live** (FCM/APNs) — транспорт готов, живые сервисные ключи не проверены.
- Нагрузочные тесты (k6), soak, пилот — не проводились.
- Веб-панели и мобильные приложения — вне этой работы (потребляют контракт).
