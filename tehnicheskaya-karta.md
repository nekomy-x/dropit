# DROP IT — Техническая карта проекта (master plan)

**Версия:** 1.0
**Дата:** октябрь 2026
**Назначение:** единый навигационный документ технической части — чтобы не запутаться: что делаем, в каком порядке, из чего состоит, где какие решения зафиксированы, какие дыры закрыты, какие вопросы открыты.

**Связанные документы:**
- `trebovania.md` — ТЗ заказчика
- `otvet-zakazchiku.md` — коммерческое предложение
- `koncepciya.md` — общая концепция и статус
- `arhitekturnoe-reshenie.md` — архитектурное решение (Rust, HA, нагрузка)
- `dizayn-zadanie.md` — задание дизайнеру

---

## ЧАСТЬ I. ОБЩАЯ КАРТА СИСТЕМЫ

### 1. Компоненты системы (полный список)

| # | Компонент | Технология | Роль | Статус плана |
|---|---|---|---|---|
| C1 | Приложение гостя | Flutter | QR → поиск → превью → оплата → статус запроса | Дизайн частично готов |
| C2 | Приложение диджея | Flutter | Очередь, стили, пауза, статусы запросов | Дизайн не начат |
| C3 | Панель клуба | React + TS + Tailwind | Профиль, тарифы, сессии, QR, доходы | Дизайн не начат |
| C4 | Админ-панель DROP IT | React + TS + Tailwind | Клубы, платежи, комиссии, инциденты, аудит | Дизайн не начат |
| C5 | Лендинг + сайт клубов | Next.js | Маркетинг | Не начат |
| C6 | API Gateway | Rust (axum) | REST: авторизация, RBAC, rate-limit, валидация | — |
| C7 | WS Gateway | Rust (tokio) | Realtime: очередь диджею, статусы гостю | — |
| C8 | Core (домены) | Rust | Сессии, запросы, каталог, биллинг, выплаты | — |
| C9 | Воркеры | Rust | Outbox-релей, payout-планировщик, сверка, cleanup | — |
| C10 | Webhook-intake Stripe | Rust | Приём и валидация вебхуков платежей | — |
| C11 | PostgreSQL 16 | — | Единственный источник истины | — |
| C12 | Redis | — | Кэш каталога, hot-состояния сессий | — |
| C13 | NATS JetStream | — | События (outbox → воркеры → WS) | — |
| C14 | Инфраструктура | Hetzner EU + Cloudflare + Docker | Хостинг, DNS, WAF, деплой | — |

### 2. Rust workspace (структура репозитория)

```
dropit/
├── crates/
│   ├── common/            # типы, ошибки, деньги (Money minor units), id
│   ├── domain/            # чистые домены: session, request, billing, catalog
│   ├── infra-db/          # sqlx-пулы, репозитории, миграции
│   ├── infra-nats/        # публикация/подписка, JetStream-стримы
│   ├── infra-redis/       # кэш, hot-данные
│   ├── gateway-api/       # C6: REST (бинарник)
│   ├── gateway-ws/        # C7: WebSocket (бинарник)
│   ├── workers/           # C9: outbox-relay, payout, reconcile, cleanup (бинарник)
│   └── webhook-stripe/    # C10 (бинарник, отдельный endpoint)
├── apps/
│   ├── mobile-guest/      # Flutter
│   ├── mobile-dj/         # Flutter
│   ├── web-club/          # React
│   ├── web-admin/         # React
│   └── sites/             # Next.js лендинги
├── deploy/                # docker-compose, Nomad/K8s манифесты, Grafana-дашборды
├── docs/                  # OpenAPI, ADR (architecture decision records), runbooks
└── tests/                 # e2e, load (k6), фикстуры
```

Правило: **одна кодовая база, пять бинарников** (api, ws, workers, webhook, migrations-runner). Домены в `domain/` не знают про HTTP/БД (чистые функции + трейты) — это делает будущее вынесение в сервисы механическим.

### 3. Роли и идентичность (закрытие дыры «кто есть кто»)

| Роль | Идентичность | Хранение | Регистрация |
|---|---|---|---|
| Гость | **Device Token** (UUIDv7, выдаётся API при первом входе, хранится в iOS Keychain / Android Keystore) | таблица `guest_devices` | Без регистрации. Опциональный апгрейд до аккаунта (email) — для «My requests» на новом устройстве |
| Диджей | Email + пароль (Argon2id) + **TOTP 2FA** | `dj` | Приглашение клубом |
| Клуб | Email + пароль + 2FA | `club` | Регистрация + верификация админом |
| Админ | Email + пароль + 2FA (обязательно) | `admin_user` | Создаётся вручную |

JWT access 15 мин / refresh 30 дней; веб-панели — httpOnly cookie + CSRF-токен; мобильные — Bearer.

**Решённые тонкости:**
- Гость без аккаунта: «My requests» живёт на device token; при апгрейде — перенос истории привязкой device→account.
- Push гостю — опционально (permission запрашивается после первой оплаты, не на онбординге — меньше отказов).
- Потеря device token = потеря «My requests» — приемлемо, показываем подсказку «привяжите email».

---

## ЧАСТЬ II. МОДЕЛЬ ДАННЫХ (полная)

Все суммы — `BIGINT` в минорных единицах (центы). Время — `TIMESTAMPTZ` (UTC). ID — `UUID` v7. Мягкие удаления не используются; статусные поля.

```sql
-- СПРАВОЧНИКИ И ПОЛЬЗОВАТЕЛИ
platform_settings (id=1, commission_mode, commission_percent_bp,   -- бипсисы 10000=100%
                    commission_fixed_minor, min_price_minor, max_price_minor,
                    dj_share_of_remainder_bp, currency, updated_at)
club  (id, name, slug, address, country, email, phone, logo_url,
       stripe_account_id, stripe_onboarding_status,        -- pending/verified/restricted
       status,          -- active/suspended/blocked
       created_at)
dj    (id, club_id NULLABLE, email, password_hash, totp_secret_enc,
       display_name, stripe_account_id, stripe_onboarding_status, status, created_at)
admin_user (id, email, password_hash, totp_secret_enc, role, status, created_at)
guest_device (id, account_id NULLABLE, platform, app_version, first_seen, last_seen)

-- ТАРИФЫ (цена = заведение, комиссия = платформа; снимок в каждом платеже)
pricing_policy (id, club_id, price_minor, dj_share_bp NULLABLE,  -- если NULL — дефолт платформы
                valid_from, valid_to, created_by, created_at)    -- история обязательна

-- СЕССИИ ВЕЧЕРА
session (id, club_id, dj_id, status,            -- created/active/paused_acceptance/closed
         pricing_policy_id, opened_at, closed_at, timezone)
qr_token (id, session_id, token_hash, active, generated_at, expires_at)
         -- ротация на сессию; expires_at = плановое авто-закрытие сессии (не фикс. +24ч)
session_guest (id, session_id, guest_device_id, joined_at, revoked_at NULL)
              -- ГРАНТ входа: выдаётся в POST /sessions/join, ОБЯЗАТЕЛЕН в POST /requests
              -- (клиентский session_id НЕ принимается на веру — защита от чужих сессий)
              -- UNIQUE(session_id, guest_device_id)

-- КАТАЛОГ (кэш найденного)
track_cache (id, source, source_id,             -- UNIQUE(source, source_id) + upsert
             isrc, title, artist, cover_url, preview_url,
             preview_url_checked_at,             -- lazy-refresh истекающих превью-URL
             style_tags jsonb, updated_at)
-- Счётчик «той же песни» и сортировка hot — по (source, source_id), НЕ по id строки
-- Клиентский результат каталога проходит серверную whitelist-валидацию:
-- домены cover/preview (*.mzstatic.com, deezer), длины полей; ISRC перепроверяется MusicBrainz

-- ЗАПРОСЫ И ДЕНЬГИ
request (id, session_id, guest_device_id, track_cache_id, style,
         price_minor,           -- из pricing_policy на момент создания
         status,                -- pending/accepted/rejected/played/expired/refunded
         reject_reason NULL, played_at NULL, created_at,
         version)               -- optimistic locking: гонки accept/played
payment (id, request_id, stripe_payment_intent_id, idempotency_key,   -- UNIQUE(key)
         amount_minor, currency, status,        -- см. стейт-машину ниже
         refunded_minor DEFAULT 0,              -- частичные рефанды
         failure_reason NULL, stripe_error_code NULL,   -- поддержка «с меня списали»
         split_snapshot jsonb,  -- ПОЛНЫЙ снимок правил разреза на момент оплаты, включая tax
         created_at, captured_at)
split (id, payment_id, payee_type,               -- platform/club/dj
       payee_id, amount_minor, reversed_minor DEFAULT 0,
       status)           -- pending/paid_out/partially_reversed/reversed
idempotency_response (key UNIQUE, guest_device_id, request_id,
                      response jsonb, created_at)  -- replay-ответ, TTL 24 ч
ledger_entry (id, txn_id, account, currency, amount_minor, entry_type, ref, created_at)
              -- двойная запись; счёт = (account, currency) уже сейчас (мультивалюта без миграции)
              -- счета: revenue/payable/expense:stripe_fees/expense:stripe_dispute_fees/tax:collected
outbox (id, aggregate_type, aggregate_id, event_type, payload jsonb,
        created_at, published_at NULL, attempts) -- transactional outbox

-- ВЫПЛАТЫ И СВЕРКА
payout (id, payee_type, payee_id, period_start, period_end,
        amount_minor, min_threshold_minor,      -- порог, напр. 1000 (10 €)
        status,                                 -- scheduled/in_flight/paid/failed/on_hold
        stripe_transfer_id, fail_reason, attempts, created_at, paid_at)
reconciliation_run (id, period_start, period_end, stripe_report_id,
                    expected_minor, actual_minor, discrepancies jsonb, status, created_at)

-- БЕЗОПАСНОСТЬ И АУДИТ
webhook_event (id, source, event_id UNIQUE, event_type, payload, processed_at) -- идемпотентность
audit_log (id, actor_type, actor_id, action, entity, diff jsonb, at)  -- append-only, партиции по месяцам
fraud_flag (id, guest_device_id, session_id, rule, score, resolved_by, resolved_at)
```

**Ключевые индексы:** `(session_id, status)` на request; `(request_id)` на payment; `(payee_type, payee_id, status)` на payout; `token_hash` уникальный; `event_id` уникальный у webhook_event; ledger — по `(txn_id)` и партиции по месяцам.

**Правила:**
- `payment.split_snapshot` неизменяем после captured — аудит «почему разделили именно так».
- Изменение `pricing_policy` создаёт **новую строку** (valid_to у старой) — старые платежи ссылаются на старую политику.
- `ledger_entry` — только append; баланс считается агрегатом, ошибки исправляются сторнирующей проводкой, не UPDATE.

---

## ЧАСТЬ III. СТЕЙТ-МАШИНЫ (единственный источник переходов)

### Session
```
created → active → paused_acceptance ⇄ active → closed
```
Закрытие: вручную (клуб/диджей «завершить смену») или авто-джоба в 07:00 по timezone клуба при неактивности >2 ч. При `closed`: все `pending` запросы → `expired`.

### Request
```
pending → accepted → played
pending → rejected (reason) 
pending → expired            (сессия закрыта / DJ на паузе >N мин — гасим)
accepted → played
accepted → rejected           (передумал до игры)
```
Переходы только эти; в Rust — `match` по (from, to), недопустимый переход = ошибка компиляции/500 + лог.

### Payment
```
intent → processing → captured → partially_refunded → refunded
intent → failed      (отказ Stripe/карты; failure_reason + stripe_error_code)
intent → cancelling  (TTL-воркер: отмена интента В STRIPE, а не просто локальная метка)
cancelling → cancelled   (Stripe подтвердил отмену)
cancelling → captured    (гонка: захват успел раньше отмены — деньги у гостя!
                          → обычный путь capture, закрываем в ledger)
intent → cancelled   (гость закрыл экран, TTL 15 мин; сancel в Stripe)
captured → captured: NO-OP (дубль вебхука с другим event_id — штатно, не 500!)
```
`refunded` — заложен с первого дня (политика «без возврата» — настройка, не отсутствие механики).

**Правило идемпотентности обработчиков:** идемпотентность не по `event_id` (у Stripe он разный для `payment_intent.succeeded` и `charge.succeeded`), а по **`(aggregate_id, event_type)`**; повторный переход в то же состояние — no-op 200 + лог, а не ошибка. Это же правило для запросов DJ: `accept` для уже `played` → no-op, `played` для `pending` → разрешён (DJ сыграл, не нажав accept).

### Payout
```
scheduled → in_flight → paid
             in_flight → failed → (retry ≤5, backoff) → on_hold  (ручной разбор админом)
```
Завершение Transfer отслеживается вебхуками `transfer.created/failed/paid` (идемпотентно по `stripe_transfer_id`) + часовой poller незакрытых `in_flight` — иначе воркер, упавший после Stripe-вызова, оставит payout висеть навсегда. При отрицательном нетто (dispute после выплаты) — блокировка выплаты + инцидент, долговое удержание из следующих недель по политике. Финальная выплата без порога — при блокировке/уходе клуба (подпороговые деньги иначе зависают навсегда).

---

## ЧАСТЬ IV. КОНТРАКТЫ

### 1. REST API (OpenAPI в `docs/openapi.yaml`)

**Гость (guest Bearer/device-token):**
```
POST /v1/guest/device                    → issue device token (лимит 5/час/IP; выше — captcha)
POST /v1/sessions/join                   { qr_token } → session info (клуб, DJ, стили, цена!) + ГРАНТ session_guest
POST /v1/ws/ticket                       → одноразовый билет WS (TTL 30 с)
GET  /v1/sessions/current                → текущая сессия устройства (по живому гранту)
GET  /v1/catalog/search?q=&style=        → треки (кэш→iTunes→Deezer)
POST /v1/requests                        { track_id, Idempotency-Key }
         → request pending + payment intent
         → UNIQUE(idempotency_key): повтор = replay сохранённого ответа (idempotency_response)
         → проверка живого session_guest + session.status == active (не клиентский session_id!)
POST /v1/payments/:id/confirm            { stripe_payment_method..., Idempotency-Key } → processing
         → GUARD: session == active AND request == pending, иначе 409 + авто-cancel intent
         → (иначе: гость платит в паузу/закрытую сессию → «заплачено за expired»)
GET  /v1/my/requests                     → история + статусы
GET  /v1/my/requests/:id                 → карточка (статус, позиция, счётчик той же песни)
POST /v1/guest/upgrade                   { email } → привязка аккаунта (опц.)
```

**Диджей (Bearer + 2FA):**
```
POST /v1/auth/dj/login · /v1/auth/dj/2fa · /v1/auth/refresh
GET  /v1/dj/shift                        → текущая сессия
GET  /v1/dj/queue                        → очередь (сортировка new|hot)
PATCH /v1/dj/styles                      → [salsa, bachata...]
POST /v1/dj/acceptance/pause · /resume
POST /v1/dj/requests/:id/accept · /reject · /played
GET  /v1/dj/stats/today
```

**Клуб (httpOnly cookie + CSRF):**
```
POST /v1/auth/club/login · 2fa · refresh
GET/PATCH /v1/club/profile
GET/POST /v1/club/pricing                → тарифы + история
POST /v1/club/sessions · PATCH /:id/close
POST /v1/club/sessions/:id/qr/generate   → qr + guest-preview payload
POST /v1/club/sessions/:id/qr/activate · /deactivate
GET  /v1/club/djs · POST /v1/club/djs/invite
GET  /v1/club/earnings?period= · GET /v1/club/payouts
GET  /v1/club/stripe/onboarding-status · POST /v1/club/stripe/onboarding-link
```

**Админ:**
```
POST /v1/auth/admin/login · 2fa
GET  /v1/admin/dashboard/now             → сессии, req/min, оборот
CRUD /v1/admin/clubs · /djs · /guests (read + block/suspend)
GET  /v1/admin/payments?filters · GET /v1/admin/payments/:id/splits
GET  /v1/admin/incidents · POST /:id/resolve
GET  /v1/admin/audit-log
GET/PATCH /v1/admin/platform-settings    → комиссия, мин/макс цена, доли
GET  /v1/admin/reconciliation-runs
```

**Общие правила:** пагинация cursor; ошибки — единый envelope `{code, message, details}` с i18n-ключом; `Idempotency-Key` обязателен на всех POST денег; rate-limit заголовки `X-RateLimit-*`.

### 2. WebSocket протокол (`/v1/ws`, токен в query → рукопожатие)

```
Клиент→сервер: ws-ticket рукопожатие (см. ниже), subscribe(session_id), heartbeat ping
Гостю:      request.accepted | request.rejected | request.played | request.expired
            session.price_changed | session.closed
            + НА ПОДПИСКУ: snapshot его активных запросов (гость в кармане терял WS —
              получит актуальные статусы сразу, не «вечное ожидание»)
Диджею:     request.created (полная карточка) | request.updated
            session.pause_required (клуб сменил настройки)
            события адресуются по dj_id (все его устройства), не только по session
```
**Аутентификация WS — одноразовый билет, не JWT в query** (JWT в URL утекает в логи Cloudflare/Loki): `POST /v1/ws/ticket` → single-use ticket TTL 30 с (Redis) → передаётся в `Sec-WebSocket-Protocol`. Гость подписывается только на события **своих** request'ов (грант session_guest + фильтр по guest_device_id), DJ/клуб — по роли и привязке к сессии.
Переподключение: экспоненциальный backoff 0.5→8 с + jitter; после reconnect — полный snapshot (гость: свои запросы; DJ: очередь). Клиентский fallback: при недоступности WS сам переключается на REST-poll (30 с, только пока есть активный pending-запрос) — иначе «деградация» превращается в молчание. WS только уведомляет, **источник истины — REST/БД**.

### 3. NATS JetStream subjects

```
events.payment.captured        → воркер ledger → события в WS
events.payment.failed · .refunded
events.request.created · .accepted · .rejected · .played
events.session.closed
payout.schedule (cron-воркер)  → payouts.worker
reconcile.daily (cron)         → reconcile.worker
inbox.webhook.stripe           ← webhook-intake (после валидации подписи)
```
Все консьюмеры — durable, ack manual, max-deliver 10 → DLQ `events.$STREAM.dlq` + алерт.

---

## ЧАСТЬ V. ПЛАТЁЖНЫЙ ЦИКЛ — ПОШАГОВО (полное закрытие)

```
1. Гость жмёт «Заказать»
   → POST /requests с Idempotency-Key
   → TX БД (всё атомарно): request(pending) + payment(intent) + pricing снимок
     (чтение политики FOR SHARE — защита от гонки смены тарифа) + session_guest-грант-чек
     + outbox(request.created)
   → UNIQUE(idempotency_key): дубликат POST = replay сохранённого ответа, второй TX нет
   → payment intent создаётся в Stripe (amount = цена клуба, application_fee = комиссия)
2. Гость платит (Apple Pay / GPay / карта через Stripe SDK в приложении)
   → GUARD в confirm: session == active AND request == pending, иначе 409 + cancel intent
   → при паузе/закрытии сессии DJ/клубом: все живые intents форс-cancel
3. Stripe → webhook payment_intent.succeeded → webhook-intake:
   → валидация подписи; INSERT webhook_event (UNIQUE event_id)
   → inbox.webhook.stripe → идемпотентность ПО (aggregate_id, event_type), НЕ по event_id
     (payment_intent.succeeded и charge.succeeded — разные event_id одного платежа)
4. Воркер billing: TX: payment→captured + ledger двойная проводка
   (revenue:platform / payable:club / payable:dj / expense:stripe_fees / tax:collected)
   + outbox(payment.captured); баланс Stripe-fee известен постфактум (balance_transaction)
   → вторая проводка fee, сплит считается от P − C, fee — расход платформы (модель A, ADR-004)
5. Outbox-relay публикует captured → WS: диджей получает запрос в очереди
6. Диджей: accept/reject/played → статусы (version/optimistic-lock; no-op на гонки)
   ДЕНЬГИ УЖЕ ЗАКРЫТЫ в шаге 4
   ИСКЛЮЧЕНИЕ: expired при живом captured → АВТОВОЗВРАТ (default) или инцидент возврата
   (конфиг; иначе «заплачено за несыгранное» = chargeback + претензия по EU consumer law)
   DJ/клуб без verified Stripe-онбординга: доля пишется в ledger, payout → on_hold + нотификация
   (плюс: клуб без verified → новые POST /requests блокируются)
7. Payout-воркер (еженедельно, пн 10:00 UTC): pending-splits → НЕТТО (pending − reversed)
   ≥ min_threshold (10 €) → Stripe Transfer → payout(in_flight → paid по вебхуку transfer.paid)
   < порога → перенос на след. неделю (scheduled)
   отрицательное нетто → блокировка + инцидент (dispute после выплаты)
   финальная выплата без порога при блокировке/уходе клуба
   ПЕРЕД циклом: проверка доступного баланса платформы (float для DJ-трансферов)
8. Reconciliation-воркер (ежедневно 05:00): Stripe-отчёт + список charges/refunds/disputes
   из API vs ledger → расхождение → incident → алерт → ручное закрытие
9. Ночной инвариант-чекер (ПО КАЖДОМУ payee, не глобально):
   Σ ledger(payable:X) == Σ split(X,pending) + Σ split(X,в выплатах in_flight|paid)
                            − Σ некомпенсированных reversed
   + микро-инвариант на каждый txn_id: Σ ledger_entry(txn) == 0
   → нарушение: стоп выплат + PagerDuty
```

**Отказоустойчивость цикла (каждая точка):**

| Точка сбоя | Что происходит | Деньги |
|---|---|---|
| Приложение умерло до подтверждения | TTL-воркер: intent → **cancelling** → Stripe-cancel → cancelled | не списаны |
| Гость закрыл экран, захват успел позже отмены | cancelling → **captured** (гонка закрыта стейт-машиной) | закрыты в ledger |
| Webhook не дошёл | часовой fallback: poll Stripe по незакрытым intents (включая cancelled/cancelling) | не потеряны |
| Webhook пришёл дважды (разные event_id!) | идемпотентность по (aggregate_id, event_type), no-op | — |
| Воркер ledger упал до проводки | событие в JetStream не ack → redelivery | не потеряны |
| NATS лежит 2 часа | outbox копится в БД, relay досылает после подъёма | не потеряны |
| Transfer выплаты не прошёл | payout(failed) → retry ×5 → on_hold; воркер упал после Stripe-вызова → transfer.paid-вебхук или часовой poller in_flight | на балансе Stripe |
| Chargeback (после выплаты!) | dispute → reversal проводка → отрицательное нетто payee → блокировка будущих выплат + dispute fee на expense:stripe_dispute_fees | закрыт сторно |
| Частичный рефанд | payment.refunded_minor + split.reversed_minor пропорционально снапшоту | закрыт |
| Рефанд руками в дашборде Stripe | ловится сверкой (API-список refunds) + вебхуком, не только отчётом | закрыт |
| DJ не прошёл Stripe-онбординг | доля копится, payout on_hold + нотификация; при уходе — политика невостребованных | на балансе |
| Клуб заблокирован с 8 € < порога | финальный payout без порога | выплачены |

---

## ЧАСТЬ VI. ВНЕШНИЕ ИНТЕГРАЦИИ

### Stripe (все вызовы через circuit breaker + retry-jitter)
- Connect: отдельные аккаунты клуба и диджея, onboarding по Stripe-линкам (статусы храним; `verified` — условие приёма платежей клубом и выплат DJ)
- PaymentIntent + `application_fee_amount` + `transfer_data` (destination charges к клубу, диджею — отдельный Transfer из расчёта) — **финальную схему сплита фиксирует ADR-004 после обсуждения с бухгалтерией заказчика** (destination vs separate transfers & fees). В ADR-004 обязательный пункт: **float платформы** — DJ-трансферы идут с баланса платформы, нужен оборотный резерв ≥ недельных DJ-долей, мониторинг баланса перед payout-циклом; альтернатива — separate_charges_and_transfers с отложенными transfer_group
- Fee модели в ledger: сплит считается от P − C, Stripe-fee — расход платформы (expense:stripe_fees), проводка по balance_transaction-вебхуку — тогда сверка сходится копейка-в-копейку
- Stripe Tax — расчёт VAT; tax-часть фиксируется в split_snapshot отдельной строкой, инвариант Σ splits == amount сходится
- Radar — правила + наши флаги
- Мин. порог выплаты 10 € (конфиг)

### Каталог
1. Redis-кэш (нормализованный query+market, TTL 24 ч) — хранит **список id track_cache**, не копию метаданных (иначе рассинхрон при upsert) →
2. iTunes Search API из **клиента** (Apple блокирует серверные IP) → сервер **whitelist-валидирует** клиентский ответ (домены mzstatic/deezer, длины, canonical-ключ source+source_id пересчитывается и сверяется — защита от отравления кэша модифицированным клиентом) → кэширует upsert'ом по UNIQUE(source, source_id) →
3. MusicBrainz: ISRC-обогащение серверно (rate 1 req/с) — вторая, серверная точка верификации трека →
4. Deezer — резерв при отсутствии превью
- **Истекающие превью-URL:** preview_url_checked_at; при выдаче старше 7 дней (Deezer 1 дня) — фоновый re-resolve; 404 при проигрывании → клиент делает re-search → update кэша

### Push
- FCM/APNs; гостю — события его запросов; диджею — «новый запрос» при убийстве WS-фона

---

## ЧАСТЬ VII. ИНФРАСТРУКТУРА, НАБЛЮДАЕМСТВО, БЕЗОПАСНОСТЬ

### Окружения
| Env | Назначение | Данные |
|---|---|---|
| local | docker-compose: всё в 1 контейнере-наборе | синтетика |
| staging | Hetzner, копия прода в миниатюре | Stripe test-mode, синтетика |
| production | 2×API/WS узла, БД+реплика, NATS, воркеры | реальные |

### CI/CD (GitHub Actions)
```
PR:    fmt + clippy -D warnings + cargo test + cargo-deny + sqlx migrate check
       + OpenAPI-diff (breaking change — блок)
main:  build (musl, ~30 МБ) → deploy staging → e2e (k6 smoke: 100 RPS) → manual gate → prod canary 10 % → 100 %
```
Rollback: предыдущий образ тегируется, деплой = подмена тега (30 с). Миграции БД — только expand→contract (никогда не ломают старый бинарник).

### Наблюдаемость
- Prometheus + Grafana: RPS, p50/p99, WS-соединения, длина outbox, age старейшего outbox-события, DLQ-глубина, выплаты/ошибки, **NTP-drift узлов, device-tokens-per-IP (антифрод-флуд), глубина очереди незакрытых intents**
- **алерты (PagerDuty + Telegram):** p99 > 200 мс 5 мин; outbox age > 60 с; DLQ > 0; инвариант ledger нарушен; failover Postgres; Stripe error-rate > 5 %; NTP |drift| > 1 мин; нет партиции на завтра
- Sentry (Rust + Flutter + React); Loki — логи с trace_id сквозным
- SLO: доступность 99,9 %; API p99 < 200 мс; WS-доставка события < 500 мс

### Эксплуатационные правила (дыры ревью)
- **Часы:** chrony/NTP на всех узлах; drift в метрики; отказ подписи вебхука из-за timestamp — отдельный лог от отказа подписи
- **DST/таймзоны:** авто-закрытие считает «следующий 07:00 в IANA-зоне клуба» на каждом тике; «активность» = last request/payment/WS-событие сессии
- **Партиции:** partition-manager-воркер создаёт месячные партиции ledger/audit на 2 месяца вперёд + алерт «нет завтрашней»; миграции — ATTACH/DETACH; contract на финансовых таблицах = пометка deprecated, НЕ DROP; lock_timeout в миграциях
- **Антифрод-грань лимитов (спецификация, реализуется в F8, лимиты уже в F2):** max N запросов/устройство/сессию, max M устройств/сессию (алерт клубу); шаринг QR-ссылкой — принятая модель, треки платные
- **Redis-падение:** клиент сам переключается на REST-poll при WS-ошибке (30 с, пока есть активный запрос)

### Безопасность — чек-лист реализации
- [ ] TLS 1.3, HSTS, Cloudflare WAF
- [ ] Argon2id, TOTP, rate-limit логина (5/15 мин)
- [ ] RBAC-мидлварь на каждый роут (тест: гость не может в DJ-эндпоинты)
- [ ] QR: 128-bit токен, hash в БД, ротация на сессию, rate-limit скана 10/мин/устройство + 5/час/IP на выпуск device-token (выше — captcha); лимит device-tokens-per-IP в алерты
- [ ] WS: одноразовый ticket вместо JWT в query (логи Cloudflare/Loki не должны видеть токены)
- [ ] Idempotency-Key на денежных POST + replay-ответы (idempotency_response)
- [ ] Webhook: верификация подписи Stripe + tolerance 5 мин (при clock-drift — отдельный лог)
- [ ] Секреты: SOPS+age в репо / Vault на проде
- [ ] GDPR: guest-данные — retention 180 дней; финансовые записи НЕ удаляются, а **псевдонимизируются** (detach device → anon-id); DSR = export + анонимизация, не delete
- [ ] Аудит-лог на все админ-действия и деньги
- [ ] Бэкапы: ежедневно + гео-копия; **restore-drill ежемесячно** (иначе бэкапа нет)
- [ ] cargo-deny + Dependabot; fuzz вебхук-парсера (cargo-fuzz)

---

## ЧАСТЬ VIII. ПОРЯДОК РАЗРАБОТКИ — ФАЗЫ С ЗАВИСИМОСТЯМИ

```
F0 Инфраструктура скелета
 └─ F1 Идентичность и роли
     └─ F2 Сессии + QR + вход гостя
         ├─ F3 Каталог (поиск/кэш)
         └─ F4 Запросы + WS (без денег)
              └─ F5 Stripe: интент→захват→ledger
                   └─ F6 Выплаты + сверка
                        └─ F8 Антифрод/аудит/hardening
                             └─ F9 Нагрузка → пилот → сторы
     └─ F7 Панели (клуб, админ) — параллельно F3–F6
```

| Фаза | Содержимое | Критерий приёмки (demo-able) |
|---|---|---|
| **F0** (нед. 1) | repo, CI-пайплайн, docker-compose local, миграции-скелет, OpenAPI-заготовка, ADR-001..003 | `make dev` поднимает всё; PR-гейты зелёные |
| **F1** (нед. 1–2) | device-token, логины DJ/клуб/админ, JWT+refresh, 2FA, RBAC | e2e-тест: все 4 роли, wrong-role → 403 |
| **F2** (нед. 2) | club/dj/session/qr, join по QR, авто-close | скан тест-QR из Flutter-заглушки → session info |
| **F3** (нед. 2–3) | search через кэш→iTunes, ISRC-обогащение | «Marc Anthony Vivir Mi Vida» за p99<300 мс, кэш-hit 2-й раз <20 мс |
| **F4** (нед. 3–4) | request lifecycle, WS-очередь, стили, пауза | запрос появляется у DJ < 500 мс; пауза блокирует POST |
| **F5** (нед. 5–6) | PaymentIntent, webhook-intake, outbox, ledger, все fallback-пути | chaos-набор: «убить воркер посреди оплаты», cancel-гонка (cancelling→captured), двойной вебхук с разными event_id, дубль Idempotency-Key, dispute после payout → деньги не теряются |
| **F6** (нед. 7) | payout-воркер, порог, transfer-вебхуки, reconciliation, инвариант по payee + txn-инвариант, partition-manager, авто-закрытие по IANA-TZ | тестовая неделя: выплаты сошлись копейка-в-копейку; инцидент симулирован и закрыт |
| **F7** (нед. 5–8, паралл.) | панель клуба (тарифы, QR-генератор с превью гостя), админка (мин. набор) | клуб создаёт сессию и QR без разработчика |
| **F8** (нед. 8–9) | антифрод-правила, аудит-лог, 2FA-принуждение, GDPR-джобы, security-чеклист | прогон чек-листа VII на 100 % |
| **F9** (нед. 10–12) | k6: 50 RPS API / 10k WS / payment-шторм 100 вебхуков/с; пилот в клубе; публикация | SLO держится 24 ч soak-теста; сторы приняли |

**Критический путь: F0→F1→F2→F4→F5.** Дизайн должен опережать каждую фазу на ≥1 неделю (см. `dizayn-zadanie.md`, приоритеты совпадают).

### Команда (минимум)
- 2 × Rust-бекенд (bus-factor — обязательно двое)
- 1 × Flutter
- 1 × React/TS (панели) — может совмещать с Next.js лендингами
- 0.5 × QA (полный день с F5 — деньги)
- DevOps — время от времени (инфра под контролем разработчиков через IaC)

---

## ЧАСТЬ IX. РЕЕСТР ВОПРОСОВ (дыры и их статус)

### Закрыто дефолтом (работаем, пока заказчик не переопределит)

| # | Вопрос | Дефолт (реализовано как конфиг) |
|---|---|---|
| Q1 | Комиссия DROP IT | **30 %** с запроса; остаток 50/50 клуб/диджей. Меняется в `platform_settings` + per-club override. *(стартовое ТЗ 50 % DROP IT считаем устаревшим — уточнить)* |
| Q2 | Мин/макс цена клуба | 1–10 € (валидация на API + в панели) |
| Q3 | Возврат при отклонении DJ | Нет; НО: `expired` при живом captured → **автовозврат по умолчанию** (см. Q16) |
| Q4 | Порог выплаты | 10 €; еженедельно; финальная выплата без порога при уходе/блокировке; Instant — v2 |
| Q5 | Автозакрытие сессии | 07:00 по IANA-зоне клуба при неактивности 2 ч; active = last request/payment/WS |
| Q6 | Смена цены | Новая строка pricing_policy (valid_to у старой, чтение FOR SHARE); действует на новые запросы; на оплаченные — нет |
| Q7 | Повторный запрос той же песни тем же гостём | Разрешён, но предупредить в UI; счётчик запросов песни виден всем; лимит запросов/устройство/сессию — антифрод (F8, спецификация в VII) |
| Q8 | Одна сессия — один DJ | MVP: да; мульти-DJ/мульти-room — v2 |
| Q9 | Языки MVP | EN + ES (RU — по желанию заказчика, infra готова) |
| Q10 | Валюта MVP | EUR; счёт ledger = (account, currency) с первого дня — мультивалюта без миграции |

### Требует ответа заказчика (блокирует конкретные шаги, не старт)

| # | Вопрос | Что блокирует | Дедлайн ответа |
|---|---|---|---|
| Q11 | Финальная комиссия (Q1) и модель фикс/процент | значения в прод-конфиге | до F5 прод |
| Q12 | Возврат при отклонении (Q3) — окончательная политика | тексты UI условий оплаты | до F4 дизайн |
| Q13 | Страна/страны пилота | Stripe-методы (Bizum?), VAT-настройка, юрист-проверка | до F5 |
| Q14 | Юрлицо для Stripe/сторов | онбординг Connect, аккаунты | до F9 |
| Q15 | Схема сплита Stripe (destination vs separate) + float платформы — ADR-004 | реализация F5, согласовать с бухгалтером заказчика | до F5 |
| Q16 | Политика невостребованных денег (DJ ушёл с долей < порога, клуб заблокирован) | воркеры финальных выплат, договор | до F6 |

### Технические решения, зафиксированные ADR

| ADR | Решение |
|---|---|
| ADR-001 | Rust модульный монолит, 5 бинарников, чистые домены |
| ADR-002 | Transactional outbox + NATS JetStream (at-least-once + идемпотентность) |
| ADR-003 | Деньги: BIGINT minor units, снимки тарифов, двойная запись ledger |
| ADR-004 | Схема Stripe-сплитов — **открыт** (см. Q15) |
| ADR-005 | WS только уведомляет; истина — REST/БД; reconnect + snapshot |
| ADR-006 | Каталог: клиентский iTunes-поиск + серверный кэш/ISRC; Deezer резерв |

---

## ЧАСТЬ X. ВЕРИФИКАЦИЯ ДОКУМЕНТА (самопроверка + внешнее ревью)

Прогон «где мы можем запутаться / что забыли»:

- [x] Все 4 роли описаны с идентичностью и каналами доступа
- [x] Каждый статус из ТЗ диджея имеет экран И endpoint И WS-событие
- [x] «Полный цикл закрытия денег»: 9 шагов + 12 точек отказа с исходами
- [x] Идемпотентность: POST денег (UNIQUE + replay) / вебхуки (по aggregate_id+event_type) / NATS-redelivery (no-op переходы) / повторный QR-скан
- [x] Часовые fallback-и: poll Stripe при потере вебхука, включая cancelled/cancelling-строки
- [x] Chargeback/dispute ПОСЛЕ выплаты: reversal + отрицательное нетто + dispute-fee на отдельный счёт
- [x] Рефанд-механика — есть, включая частичный (refunded_minor, reversed_minor)
- [x] Время: UTC в БД, IANA-зона клуба для авто-закрытия (DST), у гостя — локаль
- [x] Округление сплита: остаток цента → в комиссию платформы; описано в примере §1.1 arch-doc
- [x] Порог выплаты, перенос недели ниже порога, финальная выплата при уходе
- [x] GDPR-retention guest-данных: псевдонимизация финансовых записей, DSR = export+anon
- [x] Migration-strategy БД (expand/contract; партиции через ATTACH/DETACH, partition-manager)
- [x] Секреты, бэкапы с restore-drill, fuzz вебхуков
- [x] Локализация длин текста (RU/ES +30 %) — передано дизайнеру
- [x] App Store: покупка услуги реального мира без IAP — позиция зафиксирована в КП §6.4
- [x] Потеря device token → потеря «My requests» — принято, UI-подсказка
- [x] Поиск из клиента (IP-блок Apple) — клиент вызывает iTunes, сервер whitelist-валидирует/кэширует (анти-отравление)
- [x] Гонка cancel/захвата: стейт `cancelling` + переход cancelling→captured
- [x] Гонка accept/played: version + no-op-семантика
- [x] Оплата в паузу/закрытую сессию: guard в confirm + форс-cancel intents
- [x] Заплаченное-expired: автовозврат по умолчанию
- [x] Связка guest↔session: грант session_guest, не клиентский session_id
- [x] WS-аутентификация: одноразовый ticket, не JWT в query
- [x] Инвариант-чекер: по payee + txn-микроинвариант (не глобальная сумма, которая ложно фложила)
- [x] Stripe-fee в ledger (expense:stripe_fees) — сверка сходится; конфликт формул arch/karta устранён (fee — расход платформы)
- [x] Float платформы для DJ-трансферов — в ADR-004
- [x] Превью-URL истекают: preview_url_checked_at + lazy-refresh
- [x] track_cache: UNIQUE(source, source_id), счётчики по паре, не по id
- [ ] **Open:** ADR-004 (схема Stripe-сплитов + float) — назначить обсуждение с бухгалтером заказчика
- [ ] **Open:** Q11–Q16 — реестр выше; ни один не блокирует старт разработки
- [ ] **Open:** финальная валидация OpenAPI-схемы — после согласования этого документа (спецификация пишется в F0)

**История проверок:** v1.0 — самопроверка + внешнее независимое ревью (финансовая целостность, идемпотентность, realtime, QR-безопасность, каталог, эксплуатация). Найдено 5 критичных + 14 важных дыр, все закрыты правками этого документа; отдельные закрытия видны в пометках «дыры ревью».

---

**Следующий шаг:** согласовать дефолты Q1–Q10 (молчание = работаем по дефолту) и назначить обсуждение Q15 (бухгалтер). После этого F0 стартует без ожиданий.
