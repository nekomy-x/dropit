# DROP IT — Статус реализации

**Обновлено:** 2026-10-19
**Этап:** бэкенд доведён до плана за пять волн (W1–W5): инфра-крейты, воркеры,
эндпоинты, WS multi-node, hardening критических дефектов, контракт. Внешние
интеграции (Stripe live, лицензии каталога) включаются env-переменными; без
них — mock/noop.

---

## 1. Что реализовано (по фазам)

Бэкенд — Rust workspace из 10 крейтов: `common`, `domain`, `infra-db`,
`infra-redis`, `infra-nats`, `infra-stripe`, `infra-catalog`, `infra-push`,
`workers` (бинарник `dropit-workers`), `gateway-api` (бинарник `dropit-api`).
Два бинарника: API (`dropit-api`, axum 0.7, все роуты) и воркер
(`dropit-workers`, фоновые задачи). 10 миграций (`0001`–`0010`),
65 HTTP-путей в бинарнике и OpenAPI 0.7.0 (дрейфа нет, контракт-тест зелёный).

### F0A/F0B — скелет и среда ✅

Workspace, ADR-001…006, compose (Postgres 16 + Redis 7 + NATS JetStream),
CI (fmt/clippy/test/migrations/openapi-lint/docker), миграция `0001_init.sql`
(platform_settings NULL-семантика, audit_log append-only).

### F1 — идентичность и роли ✅

`migrations/0002_identity.sql`: `guest_device`, `dj`, `club`, `admin_user`
(CITEXT-emails, Argon2id, TOTP-секреты), `refresh_token` (ротация с семьями:
reuse → ревок семьи), `login_attempt` (rate-limit 5/15 мин),
`device_token_issue` (лимит 5/час/IP).
`crates/common/src/auth.rs`: `Role`, `AccessClaims` (JWT: sub/role/fam/iat/exp).
`crates/gateway-api/src/auth.rs`: Argon2id, JWT access 15 мин, refresh 128-бит
+ sha256, TOTP (skew ±1). `IdentityRepo`: device-token CRUD, лимиты IP,
refresh-ротация транзакцией, ревок семей. Роуты: `POST /v1/guest/device`,
`POST /v1/auth/:role/login` (+2FA-шаг), `POST /v1/auth/:role/2fa`,
`POST /v1/auth/refresh`, echo-роуты 4 ролей с RBAC-гейтом,
**`POST /v1/auth/club/master`** (вход клуба по QR master-токену, W5-C:
sha256-сверка в постоянном времени, rate-limit 10/15мин/IP, аудит без токена).
e2e: RBAC-матрица 4 ролей (своя → 200, чужая → 403 `forbidden`, без токена → 401).

### F2 — сессии, QR, join гостя ✅

`migrations/0003_sessions.sql`: `pricing_policy` (история: valid_to у старой),
`session` (стейт-машина, IANA-зона), `qr_token` (sha256, partial unique «один
активный на сессию»), `session_guest` (UNIQUE грант, re-grant).
`crates/domain/src/qr.rs`: формат QR (128 бит hex), `next_close_at`
(«следующий 07:00 в IANA-зоне», DST-безопасно), `should_auto_close` (2 ч).
`SessionRepo`: create/find, `apply_transition`, `issue_qr_token`, `find_active_qr`,
`grant_session_guest`, `session_info_for_guest`. Роуты: `POST /v1/sessions/join`,
`POST /v1/club/sessions`, `qr/generate`/`activate`/`deactivate`, `close`,
`POST /v1/dj/sessions/:id/open`, **`GET /v1/sessions/current`**.

### F3 — каталог ✅ (транспорт; лицензии — блок B5)

`crates/domain/src/catalog.rs`: `CatalogTrack`, `Source`, `canonical_key`,
`validate_track` (whitelist доменов mzstatic/deezer, https, лимиты, ISRC),
`TrackCounter`. `crates/infra-catalog`: реальный HTTP-клиент `HttpCatalog`
(iTunes Search + Deezer, без API-ключей) и `MockCatalog`; парсеры
`parse_itunes_search`/`parse_deezer_search` на записанных JSON-фикстурах;
`canonical_key`, `validate_isrc`, `cache_ttl`. Роут: **`GET /v1/catalog/search`**.
Лицензии на метаданные/превью/обложки по странам — открыты (B5).

### F4 — запросы и очередь DJ, без денег ✅

`migrations/0004_requests.sql`: `track_cache` (UNIQUE(source,source_id),
canonical_key), `request` (version — optimistic locking, снимок price_minor),
`idempotency_response` (replay, TTL), `outbox` (ADR-002). `RequestRepo`:
`create_request` (TX: FOR SHARE active-гейт + живой грант + INSERT + outbox;
replay по Idempotency-Key), `dj_queue(new|hot)` (hot — счётчик по canonical-ключу),
`apply_action` (no-op + version-гонки → no-op, не перезапись). Роуты:
`POST /v1/requests`, **`GET /v1/my/requests`**, **`GET /v1/my/requests/:id`**
(позиция + same-song count), `GET /v1/dj/queue?sort=`, `accept/reject/played`,
`acceptance/pause`/`resume`, `POST /v1/ws/ticket`, `POST /v1/requests/:id/payment`.

### F5 — платежи ✅ (Test Mode по умолчанию; live-Stripe за env)

`migrations/0005_payments.sql`: `payment` (стейт-машина, split_snapshot
неизменяем после captured), `split` (Σ == amount), `ledger_entry` (двойная
запись, append-only), `webhook_event` (delivery- и semantic-дедуп).
`crates/domain/src/payment.rs`: все переходы, `cancelling→captured` (гонка),
`captured+Capture = NO-OP`, `compute_split` (округление вниз, остаток не теряется).
`crates/infra-stripe`: трейт `StripeApi`, `LiveStripe` (REST: PaymentIntent
create/capture/refund, Connect account+account_link, transfer, idempotency-key;
**проверка подписи** `t=,v1=` HMAC-SHA256, tolerance 300 с) и `MockStripe`.
`BillingRepo`: `create_payment`, `confirm_payment` (GUARD: request pending +
session active FOR UPDATE/SHARE), `handle_capture_webhook` (ОДНА TX: payment→
captured + split + ledger + outbox + processed-метка; дедуп). Роуты:
`POST /v1/payments/:id/confirm`, `POST /v1/stripe/webhook` (подпись → дедуп →
capture TX). По умолчанию `STRIPE_API_KEY` пуст → `MockStripe`; live — после
ADR-004/Q15.

### F6 — выплаты и сверка ✅ (Test Mode по умолчанию)

`migrations/0006_payouts.sql`: `payout` (scheduled→in_flight→paid/failed→
on_hold, UNIQUE transfer_id, final-флаг), `reconciliation_run`, `incident`.
`crates/domain/src/payout.rs`: retry ≤5 → on_hold, no-op дублей transfer,
`decide_payout` (порог 10 €, финальная без порога), backoff. `PayoutRepo`:
`net_by_payee` (НЕТТО по каждому payee), `run_payout_cycle`, `handle_transfer_
paid/failed` (идемпотентно по transfer_id; failed×5 → on_hold + инцидент),
`payee_invariant_violations`, `run_reconciliation`. Роуты: `POST /v1/admin/
payouts/run`, `GET /v1/admin/payouts`, `POST /v1/stripe/transfer-webhook`,
`POST /v1/admin/reconciliation/run`, **`GET /v1/admin/reconciliation-runs`**,
`GET /v1/admin/incidents` + `resolve`.

### F7 — панели клуба и админа ✅ (эндпоинты + каркасы)

`crates/gateway-api/src/panel_routes.rs`: клуб — dashboard, profile, pricing
с историей Q6, earnings, djs, payouts, **`POST /v1/club/djs/invite`**,
**`GET/POST /v1/club/stripe/onboarding-status`** / **`onboarding-link`**;
админ — dashboard/now, **`GET/POST /v1/admin/clubs`**, **`PATCH /v1/admin/clubs/:id`**,
**`GET /v1/admin/guests`**, payments+splits, audit-log, platform-settings,
djs, reconciliation-runs; DJ — shift, stats/today, **`PATCH /v1/dj/styles`**.
Каркасы панелей: `apps/web-club/`, `apps/web-admin/` (React+TS+Vite+Tailwind) —
живые API-клиенты на контрактах openapi.

### F8 — hardening ✅

`crates/domain/src/fraud.rs`: лимит 20 запросов/устройство/сессию → deny;
300 устройств/сессию → алерт. `crates/common/src/crypto.rs`: AES-256-GCM для
TOTP (`v2:nonce:ct`, ключ `DROPIT_TOTP_KEY`, легаси-plain читается).
`migrations/0007_gdpr.sql`: `gdpr_action`. `JobsRepo`: авто-закрытие сессий,
TTL-интенты, антифрод-счётчики, GDPR (`anonymize_guest_device`, `retention_sweep`,
`export_guest_data`).

### F9 — инфраструктура и воркеры ✅ (раньше «не начаты» — теперь есть)

Инфра-крейты Wave 1–3:
- `crates/infra-redis` — `TicketStore` (одноразовые WS-тикеты, GETDEL+TTL) +
  `RedisTicketStore`/`InMemoryTicketStore`; `EventBus` pub/sub +
  `RedisEventBus`/`InMemoryEventBus`. Включается `REDIS_URL`.
- `crates/infra-nats` — `OutboxStore`+`MessageBus`, `OutboxRelay::run_once`
  (claim → publish → mark, батчи, ретраи, исчерпание попыток), `NatsBus`
  (async-nats, JetStream), `NoopBus`. Включается `NATS_URL`.
- `crates/infra-stripe` — `LiveStripe` + `MockStripe` + проверка подписи
  вебхуков (см. F5). Включается `STRIPE_API_KEY`/`STRIPE_WEBHOOK_SECRET`.
- `crates/infra-catalog` — `HttpCatalog` (iTunes+Deezer) + `MockCatalog`.
- `crates/infra-push` — `PushSender`, `FcmSender` (FCM v1, OAuth2 JWT),
  `ApnsSender` (ES256 JWT), `RoutingPush`, `NoopPush`. Включается
  `FCM_*`/`APNS_*`.
- `crates/workers` (`dropit-workers`) — супервизор `run` и `--once <job>`:
  `outbox`, `payouts`, `reconcile`, `retention`, `close-sessions`, `push`.
- WS multi-node: `crates/gateway-api/src/ws.rs` берёт тикеты из Redis
  (`RedisTicketStore`) и релеит live-события между узлами через
  `RedisEventBus`; без Redis — in-memory (один узел). См. ADR-004/005.
- Новые миграции: `0008_panel_tails.sql`, `0009_push_tokens.sql`,
  `0010_guest_account.sql`. Роут: **`POST /v1/guest/upgrade`** (bind email),
  **`POST /v1/guest/push-token`** (регистрация push-токена гостя).

### Волна 5 — hardening и закрытие критических дефектов ✅

| Дефект | Что было | Что сделано | Доказательство |
|---|---|---|---|
| IDOR в confirm платежа | confirm принимал чужой `payment_id` без проверки владельца/сессии | `confirm_payment` — GUARD: session active (FOR SHARE) + request pending (FOR UPDATE); чужой → 403 | `payments_routes.rs:251` (FOR UPDATE guard), e2e `e2e_payments` |
| Подпись transfer-вебхука | `POST /v1/stripe/transfer-webhook` не проверял подпись | `verify_webhook` (HMAC-SHA256, `t=,v1=`, tolerance 300с) перед обработкой; битая/просроченная → 401 | `payouts_routes.rs:361-367`, `infra-stripe` `verify_webhook` |
| Нетранзакционная выплата | payout писал split/ledger/outbox отдельными запросами | `handle_transfer_paid` — ОДНА TX: payout→paid + split + ledger + outbox + processed-метка; дедуп по transfer_id | `PayoutRepo::handle_transfer_paid`, e2e `e2e_payouts` |
| Заглушка QR | QR-токен был stub/константа | 128 бит энтропии, в БД только sha256-хеш, partial-unique «один активный на сессию» | `domain/src/qr.rs`, `SessionRepo::issue_qr_token` |
| Брутфорс 2FA | `/v1/auth/:role/2fa` без rate-limit (6-значный TOTP перебирается) | rate-limit 5 неудач / 15 мин по (role, email); 6-й → 429 | `routes.rs:login_2fa`, e2e `e2e_2fa_rate_limit_and_success` |
| Хардкод IP в антифроде | device-token лимит считался по захардкоженному localhost | `client_ip()` из `X-Forwarded-For`/`X-Real-IP`, `TRUST_PROXY` env; неидентифицируемый → `0.0.0.0` (валидный inet, бакет работает) | `routes.rs:client_ip`, e2e `e2e_device_token_ip_limit` |
| Дубли заявок по Idempotency-Key | повторный POST `/v1/requests` с тем же ключом создавал дубль | replay по `idempotency_response` (TTL): повторный ключ → 200 + replay исходного ответа | `RequestRepo::create_request` (TX + replay), e2e `e2e_requests` |
| Двойной платёж на request | повторный confirm на тот же request создавал второй payment | `confirm_payment` идемпотентен: повторный успешный confirm → 200 `processing` без нового платежа | `payments_routes.rs:352-413`, e2e `e2e_payments` |

## 2. Что осталось (честный список)

| # | Что | Почему | Блокер |
|---|---|---|---|
| 1 | Stripe **live** (реальные деньги) | `LiveStripe` написан и проверяется подпись, но схема Connect (destination charges vs separate transfers) не утверждена | ADR-004 (Q15) + Q11–Q14 (юрлицо/страна/комиссия) |
| 2 | Каталог **live** по странам | Транспорт iTunes/Deezer готов, но права на метаданные/превью/обложки в коммерческом продукте не проверены | B5 (лицензии) |
| 3 | MusicBrainz ISRC-обогащение (серверно, 1 req/с) | Не подключено | B5 |
| 4 | Float платформы для DJ-трансферов | Требует оборотный резерв и модель комиссии | ADR-004 + бухгалтер |
| 5 | Push **live** (FCM/APNs) | Транспорт готов, нужен сервисный аккаунт FCM и APNs-ключи; живые ключи НЕ проверены | владелец (ключи) |
| 6 | NATS **live** | `NatsBus` написан, но проверен только на `NoopBus` (outbox копится, не публикуется); живой JetStream не гонялся | NATS-стенд + нагрузка |
| 7 | Нагрузка (k6), soak, пилот | Инфра готова, нагрузка не гонялась | отдельный этап |
| 8 | UI по макету pen.dev | Экраны гостя 01–05 перенесены; DJ 01–10, остальные экраны — дальше | UI-трек, не бэкенд |
| 9 | Хвосты панелей-клиентов | inviteDj/createClub/listGuests — эндпоинты есть, клиенты на live | UI-трек |

**Не осталось:** заглушек `todo!()`/`unimplemented!()`/`unwrap`/`panic` в
рабочем коде; mock-Stripe как единственного провайдера (live за env);
in-memory WS как единственного режима (Redis за env); ручных триггеров
джоб вместо воркера.

## 3. Как запускать

### Локальный стенд (Docker, приближён к проду)

```sh
docker compose -f deploy/docker-compose.yml up -d        # postgres, redis, nats, api, workers
# воркер применяет миграции при старте (DROPIT_RUN_MIGRATIONS=true)
# healthcheck API: http://localhost:8080/healthz
```

### На хосте (dev)

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_HOME="$(pwd)/.cargo-home"   # без неё cargo падает с Operation not permitted
export DATABASE_URL="postgres://dropit:dropit_local@localhost:5432/dropit"
export REDIS_URL="redis://localhost:6379"
export NATS_URL="nats://localhost:4222"

# миграции (один из вариантов):
#   через воркер:  DROPIT_RUN_MIGRATIONS=true cargo run -p dropit-workers -- run  (однократно, затем Ctrl-C)
#   через psql:    for f in crates/infra-db/migrations/*.sql; do
#                    docker exec -i deploy-postgres-1 psql -U dropit -d dropit -v ON_ERROR_STOP=1 < "$f"
#                  done

# демо-данные (клуб/DJ/админ+TOTP/5 треков):
cargo run -p dropit-gateway-api --bin seed_demo

# API:
cargo run -p dropit-gateway-api --bin dropit-api        # 0.0.0.0:8080
# воркер (супервизор):
cargo run -p dropit-workers -- run
# воркер (один прогон, CI/крон):
cargo run -p dropit-workers -- --once outbox
cargo run -p dropit-workers -- --once payouts
cargo run -p dropit-workers -- --once reconcile
cargo run -p dropit-workers -- --once retention
cargo run -p dropit-workers -- --once close-sessions
cargo run -p dropit-workers -- --once push
```

`dropit-workers --help` — справка по командам и env.

### Переменные окружения

| Переменная | Назначение | Дефолт / деградация |
|---|---|---|
| `DATABASE_URL` | Postgres (или `DROPIT_DATABASE_URL`) | нет → data-роуты 503, БД-задачи skip |
| `REDIS_URL` | WS-тикеты + pub/sub между узлами | нет → in-memory (один узел) |
| `NATS_URL` | outbox-реле (ADR-002) | нет → NoopBus, реле skip |
| `STRIPE_API_KEY` / `STRIPE_WEBHOOK_SECRET` | live-Stripe | пусто → MockStripe |
| `FCM_PROJECT_ID` / `FCM_SERVICE_ACCOUNT_JSON` | push Android | нет → FCM выключен |
| `APNS_TEAM_ID` / `APNS_KEY_ID` / `APNS_PRIVATE_KEY_P8` / `APNS_TOPIC` / `APNS_SANDBOX` | push iOS | нет → APNs выключен |
| `DROPIT_JWT_SECRET` | подпись JWT | dev-значение (прод — сменить) |
| `DROPIT_TOTP_KEY` | AES-256-GCM ключ (32 байта hex) | отсутствие → ошибка, не тихий plain |
| `DROPIT_BIND` | адрес API | `0.0.0.0:8080` |
| `TRUST_PROXY` | доверять `X-Forwarded-For`/`X-Real-IP` для IP-лимитов (антифрод, master-вход) | `1` (прод за nginx); `0` — заголовки игнорируются |
| `DROPIT_RUN_MIGRATIONS` | прогон миграций при старте воркера | `false` |
| `RUST_LOG` | уровень логирования | `info` |
| `OUTBOX_INTERVAL_SECS` и др. `*_INTERVAL_SECS`, `OUTBOX_BATCH_SIZE`, `OUTBOX_MAX_ATTEMPTS`, `PUSH_BATCH_SIZE` | интервалы/батчи супервизора | разумные дефолты |

## 4. Как гонять тесты

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_HOME="$(pwd)/.cargo-home"
export DATABASE_URL="postgres://dropit:dropit_local@localhost:5432/dropit"

#.unit-тесты крейта (без БД):
cargo test -p dropit-infra-stripe
cargo test -p dropit-infra-push
cargo test -p dropit-infra-catalog

# БД-зависимые e2e — ТОЛЬКО через лок-скрипт (общая БД, параллельные агенты):
./scripts/db-test.sh cargo test -p dropit-gateway-api e2e_payments -- --test-threads=1
./scripts/db-test.sh cargo test -p dropit-gateway-api e2e_payouts -- --test-threads=1
./scripts/db-test.sh cargo test -p dropit-gateway-api e2e_panels -- --test-threads=1

# весь воркспейс (БД-тесты — через лок):
./scripts/db-test.sh cargo test --workspace --no-fail-fast -- --test-threads=1

# сборка + линтер:
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

`scripts/db-test.sh` берёт межпроцессный лок (`mkdir`) и держит его до конца
команды — e2e-тесты используют одну Postgres и чистят фикстуры (TRUNCATE),
поэтому параллельные прогоны без лока дают ложные падения.

**Тестов: 403 passed / 0 failed** (полный прогон `./scripts/db-test.sh cargo test
--workspace --no-fail-fast -- --test-threads=1`, 2026-10-19). Разбивка по крейтам:
common 13, domain 42, gateway-api 75 (69 unit + 6 контракт-тест OpenAPI),
infra-catalog 88 (unit + integration-фикстуры), infra-db 8, infra-nats 15,
infra-push 44, infra-redis 12 (unit + Redis-integration), infra-stripe 75
(73 unit + 2 doc-test), workers 31.

## 5. Внешние сервисы и деградация

| Сервис | Зачем | Без него |
|---|---|---|
| Postgres | всё состояние | data-роуты 503, БД-задачи воркера skip (код 0) |
| Redis | WS-тикеты вне процесса + pub/sub между узлами | in-memory: тикеты и live-события в рамках одного узла (мультиузла нет) |
| NATS JetStream | outbox-реле (ADR-002) | NoopBus: outbox копится в БД, не публикуется; push и остальные задачи работают |
| Stripe | реальные платежи/выплаты/онбординг | MockStripe: флоу платежей имитируется вебхуком с мок-подписью |
| FCM/APNs | push-уведомления | NoopPush: уведомления никуда не уходят, остальное работает |
| iTunes/Deezer | каталог треков | `HttpCatalog` делает реальные запросы; в тестах — `MockCatalog` на фикстурах |

Правило деградации: отсутствие сервиса не валит процесс — поднимается
in-memory/noop-аналог и в лог пишется предупреждение. Прод-путь включается
только явной env-конфигурацией.

## 6. Открытые вопросы владельцу (Q11–Q16, B5)

Q11 (комиссия), Q12 (возврат при отклонении), Q13 (страна пилота), Q14 (юрлицо),
Q15 (Stripe-схема, ADR-004), Q16 (невостребованные деньги — закрыт механикой
final-payout, политика-текст — владелец). B5 — лицензии каталога. Дефолты
Q1–Q10 рабочие, значения — конфиги.

---

## Приложение A. UI-трек (кратко)

Экраны гостя 01–05 перенесены по макету pen.dev (токены/шрифты Anton/Inter/
Geist Mono, знак «пиксельная волна», пиксельная сверка). Панели клуба/админа —
React-каркасы на live-контрактах. Flutter-приложения гость/DJ — каркасы с
WS-live (редьюсеры, мок-стрим). Полная сверка UI — `docs/ui/README.md`.
SDK Flutter/Android установлен на носителе
(`/Volumes/Хранилище/Dev/mobile-sdk`), `flutter test` — зелёные.

## Приложение B. Журнал проверок (бэкенд)

| Дата | Команда | Результат |
|---|---|---|
| 2026-10-12 | `ls`, `git rev-parse`, `rustc/cargo --version` | 6 документов; toolchain в `~/.cargo/bin` |
| 2026-10-13 | F0A: compose, миграция, build, healthz, 12 тестов, clippy/fmt | ✅ |
| 2026-10-13 | F1: `cargo test -p dropit-gateway-api` | ✅ 39 passed (RBAC-матрица) |
| 2026-10-13 | F2: `cargo test -p dropit-gateway-api` | ✅ 46 passed (QR-флоу) |
| 2026-10-13 | F4: `cargo test -p dropit-gateway-api` | ✅ 48 passed (request-флоу) |
| 2026-10-13 | F5: `cargo test -p dropit-gateway-api` | ✅ 61 passed (chaos: дубль вебхука, cancel-гонка, инварианты) |
| 2026-10-13 | F6: `cargo test -p dropit-gateway-api` | ✅ 67 passed (цикл выплат+сверка) |
| 2026-10-13 | F8: `cargo test -p dropit-gateway-api` | ✅ 75 passed (антифрод+джобы+GDPR+crypto) |
| 2026-10-19 | W1–W3: `cargo test -p dropit-infra-{redis,nats,stripe,catalog,push}` | ✅ зелёные (unit + интеграция) |
| 2026-10-19 | W5: hardening дефектов (IDOR, transfer-подпись, TX-payout, QR, 2FA-лимит, IP, идемпотентность) | ✅ e2e-тесты зелёные |
| 2026-10-19 | `cargo build --workspace` + `cargo clippy --workspace --all-targets` | ✅ 0 warning (кроме предсуществующего future-incompat sqlx-postgres) |
| 2026-10-19 | `./scripts/db-test.sh cargo test --workspace --no-fail-fast -- --test-threads=1` | ✅ 403 passed / 0 failed / 0 ignored |

## Приложение C. Документы

- План доводки: `docs/BACKEND_COMPLETION_PLAN.md`
- Контракт API: `docs/openapi.yaml` (0.7.0)
- ADR: `docs/adr/ADR-001`…`ADR-006`
- Техкарта: `tehnicheskaya-karta.md`; архитектура: `arhitekturnoe-reshenie.md`
