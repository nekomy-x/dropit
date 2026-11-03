# Аудит: `apps/web-club` → бэкенд (матрица «экран → эндпоинт → статус»)

Дата: 2026-10-29, ~03:20 MSK. Ревизия: `5f861be` (`main`) **+ рабочее дерево**, в котором
`apps/web-club` правился параллельно во время аудита (`src/pages/DJConsoleViews.tsx`,
`src/dj-console/*`, `src/api/client.ts`, `src/pages/SessionsPage.tsx` менялись в 02:47–03:11).
Номера строк ниже — по снимку, проверенному после последней правки этих файлов; рядом с
каждой ссылкой в тексте указан смысловой якорь (имя функции/ключ), чтобы утверждение можно
было перепроверить после сдвига строк.

Задача: `task-1`. Автор: `audit-club`.

**Как проверялось.** Только чтение: `apps/web-club/src/**` (все страницы, `api/client.ts`,
`api/connect.ts`, `api/developer.ts`, `dashboard/useClubBoard.ts`, `dj-console/*`, `qr/*`,
`components/**`, `messages/**`), `crates/gateway-api/src/*.rs` (все `.route(...)`,
хендлеры, DTO, `rbac.rs`), `crates/infra-db/src/{requests,api_keys}.rs`,
`crates/infra-db/migrations/*.sql`, `docs/openapi.yaml`, `crates/gateway-api/src/e2e_*.rs`,
`crates/gateway-api/tests/openapi_contract.rs`. `cargo test` **не запускался** (запрет
задачи): наличие e2e подтверждено чтением тестов, а не прогоном. Код приложения и бэкенда
не менялся — создан только этот файл.

---

## 1. Резюме

Проверено **56 сценариев** на **49 различных маршрутах** (метод + путь), которые
`apps/web-club` реально вызывает в production-сборке (`IS_DEMO=false`): 11
авторизационных, 20 клубных панельных, 5 управления API-ключами, 3 документации
разработчика, 9 DJ-консоли, 2 WebSocket, 1 публичная проба статуса, 1 запись
онбординг-заявки.

| Статус | Кол-во | Что это |
|---|---:|---|
| `implemented` | **42** | маршрут зарегистрирован, метод/RBAC/поля сходятся, e2e есть (или хендлер покрыт тестом другого субъекта той же ветки) |
| `partial` | **13** | маршрут есть, но расходится поле/семантика/код ответа либо нет e2e |
| `missing` | **1** | `POST /v1/club/onboarding/apply` — весь публичный визард `/connect/*` |
| `stub` | **0** | серверных заглушек 501 в клубном контуре нет; 501 приходит только из клиентской заглушки `NotImplementedStubError` (`apps/web-club/src/api/connect.ts:93-126`) |

Главное: **сам клубный пульт почти полностью обеспечен бэкендом** — сессии/QR, состав DJ
(инвайт, статус, заметка, удаление/восстановление, presence), тарифы, доходы, API-ключи,
документация и DJ-консоль (включая WebSocket-очередь) имеют живые маршруты с e2e. Пробелы
сосредоточены в трёх местах: (1) публичный онбординг `/connect` шлёт заявку в
несуществующий эндпоинт и всегда заканчивается «черновик сохранён локально»; (2) панель
читает из ответов поля, которых там нет (`timezone`/`dj_id` сессии, `city`/`timezone`/
`currency` клуба), из-за чего после F5 экран смены **утверждает, что DJ не назначен**, а
смена всегда открывается в `Europe/Madrid`; (3) DJ-консоль считает «played», «tips»,
«историю», «now playing» из локальных пустых массивов, поэтому в production показывает
нули при живых данных на сервере.

---

## 2. Как читать статусы

- `implemented` — маршрут есть в коде (`.route(...)`, а не только в `openapi.yaml`), метод
  совпадает, RBAC-гейт корректен, тело принимает то, что шлёт UI, ответ содержит **все**
  поля, которые UI читает, и есть e2e.
- `partial` — маршрут есть, но расходится хотя бы одно: поля, типы, query-параметры,
  статус-коды, обработка ошибок, семантика значений или отсутствует тест.
- `missing` — UI делает вызов, маршрута нет. `stub` — маршрут/клиент отдаёт 501/заглушку.
- «Влияние»: `production` — достижимо в `vite build --mode production`
  (`__APP_MODE__='production'`, `apps/web-club/vite.config.ts:8-14`,
  `apps/web-club/src/app-mode.ts:6`); `demo-only` — только в `--mode demo`.

---

## 3. Полная таблица сценариев

### 3.1 Авторизация, сессии, инвайты

| # | Экран/маршрут | Действие или данные | Ожидаемый эндпоинт | Статус | Доказательство (frontend ↔ backend) | Влияние | Severity |
|---|---|---|---|---|---|---|---|
| 1 | `/login` | вход клуба по паролю; ответ `TokenPair` \| `TotpChallenge` | `POST /v1/auth/club/login` | implemented | `apps/web-club/src/pages/LoginPage.tsx:60` ↔ `crates/gateway-api/src/routes.rs:94`, `:318`; тест `crates/gateway-api/src/e2e_signup.rs` (`/v1/auth/club/login`) | production | — |
| 2 | `/login` шаг 2 | 6-значный TOTP → `TokenPair` | `POST /v1/auth/club/2fa` | partial | `apps/web-club/src/pages/LoginPage.tsx:80` ↔ `crates/gateway-api/src/routes.rs:94`, `:386`; **нет промежуточного mfa-тикета** (`routes.rs:420-423` — «полноценный flow … вводится в F8»), т.е. шаг 2 не доказывает, что пароль был проверен | production | high |
| 3 | `/login` (master-токен) | обмен постоянного токена клуба на JWT | `POST /v1/auth/club/master` | implemented | `apps/web-club/src/pages/LoginPage.tsx:95` ↔ `crates/gateway-api/src/routes.rs:95`, `:842`; e2e `crates/gateway-api/src/e2e_panels.rs:1156` | production | — |
| 4 | транспорт (все запросы) | ротация refresh раз в 15 мин, reuse → ревок семьи | `POST /v1/auth/refresh` | implemented | `apps/web-club/src/api/client.ts:693-725` ↔ `crates/gateway-api/src/routes.rs:96`, `:542`; e2e `crates/gateway-api/src/e2e.rs:298` | production | — |
| 5 | `/register?role=club` | открытая регистрация клуба → 201 + токены | `POST /v1/auth/club/register` | implemented | `apps/web-club/src/pages/RegisterPage.tsx:164` ↔ `crates/gateway-api/src/signup_routes.rs:126`; e2e `crates/gateway-api/src/e2e_signup.rs` | production | — |
| 6 | `/register?role=dj` | регистрация DJ (в т.ч. `club_slug`) | `POST /v1/auth/dj/register` | implemented | `apps/web-club/src/pages/RegisterPage.tsx:168` ↔ `crates/gateway-api/src/signup_routes.rs:127`; e2e `crates/gateway-api/src/e2e_signup.rs` | production | — |
| 7 | `/dj/login` | вход DJ по паролю | `POST /v1/auth/dj/login` | implemented | `apps/web-club/src/pages/DjLoginPage.tsx:37` ↔ `crates/gateway-api/src/routes.rs:93`; e2e `crates/gateway-api/src/e2e_signup.rs` | production | — |
| 8 | `/dj/login` шаг 2 | TOTP для DJ | `POST /v1/auth/dj/2fa` | partial | `apps/web-club/src/pages/DjLoginPage.tsx:36` ↔ `crates/gateway-api/src/routes.rs:94`, `:386`; та же дыра, что №2 | production | high |
| 9 | `/reset-password` | запрос ссылки сброса (не раскрывает наличие адреса) | `POST /v1/auth/password/reset-request` | implemented | `apps/web-club/src/pages/ForgotPasswordPage.tsx:51` ↔ `crates/gateway-api/src/reset_routes.rs:135`; e2e `crates/gateway-api/src/e2e_reset.rs` | production | — |
| 10 | `/reset-password/:token` | смена пароля по одноразовому токену | `POST /v1/auth/password/reset` | implemented | `apps/web-club/src/pages/ResetPasswordPage.tsx:39` ↔ `crates/gateway-api/src/reset_routes.rs:136`; e2e `crates/gateway-api/src/e2e_reset.rs` | production | — |
| 11 | `/dj/invite/:token` | принять инвайт: задать пароль → 200 `{accepted:true}` | `POST /v1/dj/invite/accept` | implemented | `apps/web-club/src/pages/DjInvitePage.tsx:20` (токен из `useParams`, `:9`) ↔ `crates/gateway-api/src/panel_routes.rs:51`, `:1032`; e2e `crates/gateway-api/src/e2e_panels.rs:388` | production | — |

### 3.2 Оболочка, дашборд, смена, QR

| # | Экран/маршрут | Действие или данные | Ожидаемый эндпоинт | Статус | Доказательство (frontend ↔ backend) | Влияние | Severity |
|---|---|---|---|---|---|---|---|
| 12 | оболочка (`Layout`) | имя клуба в рейле; один запрос на загрузку | `GET /v1/club/profile` | partial | `apps/web-club/src/components/Layout.tsx:37` ↔ `crates/gateway-api/src/panel_routes.rs:36-38`, `:216`; ответ не содержит `city`, `timezone`, `currency`, `created_at`, `stripe_account_id` (`panel_routes.rs:247-262`), клиент подставляет константы (`api/client.ts:831-847`) | production | medium |
| 13 | `/dashboard` (доска) | открытая смена, «заработано», 24 ч запросов | `GET /v1/club/dashboard` | partial | `apps/web-club/src/dashboard/useClubBoard.ts:236` → `apps/web-club/src/api/client.ts:771-780` ↔ `crates/gateway-api/src/panel_routes.rs:34`, `:182`; в ответе только `session.id/status` + 2 числа (`panel_routes.rs:229-233`), полей `timezone`/`dj_id` нет | production | high |
| 14 | `/dashboard` доска | состав, `online`, `on_shift` | `GET /v1/club/djs` | implemented | `apps/web-club/src/dashboard/useClubBoard.ts:238` ↔ `crates/gateway-api/src/panel_routes.rs:44`, `:494`; e2e `crates/gateway-api/src/e2e_panels.rs:1507` | production | — |
| 15 | `/dashboard` доска | активный тариф (`valid_to=null`) | `GET /v1/club/pricing` | implemented | `apps/web-club/src/dashboard/useClubBoard.ts:239`, `:185-194` ↔ `crates/gateway-api/src/panel_routes.rs:39-42`, `:279`; e2e `crates/gateway-api/src/e2e_panels.rs:178` | production | — |
| 16 | `/dashboard` доска | ротация QR смены (одноразовый токен) | `POST /v1/club/sessions/:id/qr/generate` | implemented | `apps/web-club/src/dashboard/useClubBoard.ts:311` ↔ `crates/gateway-api/src/sessions_routes.rs:151`, `:443`; e2e `crates/gateway-api/src/e2e_sessions.rs:16` | production | — |
| 17 | `/sessions` (восстановление смены) | после F5 берёт смену из дашборда | `GET /v1/club/dashboard` | partial | `apps/web-club/src/pages/SessionsPage.tsx:109-112` ↔ `crates/gateway-api/src/panel_routes.rs:213-219`; в стейт кладутся `timezone: null, djId: null` (`SessionsPage.tsx:170`), и экран печатает «DJ не назначен» (`SessionsPage.tsx:326`), даже если `session.dj_id` заполнен; отдельного `GET /v1/club/sessions/:id` в API нет (об этом честно написано в `api/client.ts:781-799`) | production | high |
| 18 | `/sessions` | селект DJ для новой смены (только `active`) | `GET /v1/club/djs` | implemented | `apps/web-club/src/pages/SessionsPage.tsx:99`, `:275` ↔ `crates/gateway-api/src/panel_routes.rs:44`; e2e `crates/gateway-api/src/e2e_panels.rs:1507` | production | — |
| 19 | `/sessions` | открыть смену → 201 `{id,status,timezone,dj_id?}` | `POST /v1/club/sessions` | implemented | `apps/web-club/src/pages/SessionsPage.tsx:195-205` ↔ `crates/gateway-api/src/sessions_routes.rs:150`, `:393` (валидация IANA), ответ `SessionResponse` `:68-74`; e2e `crates/gateway-api/src/e2e_sessions.rs:16` | production | — |
| 20 | `/sessions` | закрыть смену (необратимо) → 200 | `PATCH /v1/club/sessions/:id/close` | implemented | `apps/web-club/src/pages/SessionsPage.tsx:227-233` ↔ `crates/gateway-api/src/sessions_routes.rs:154`, `:627`; e2e `crates/gateway-api/src/e2e_sessions.rs:16` | production | — |
| 21 | `/sessions` | выдать/ротировать QR | `POST /v1/club/sessions/:id/qr/generate` | implemented | `apps/web-club/src/pages/SessionsPage.tsx:256` ↔ `crates/gateway-api/src/sessions_routes.rs:151`; e2e `crates/gateway-api/src/e2e_sessions.rs:16` | production | — |
| 22 | `/qr` (студия) | взять открытую смену | `GET /v1/club/dashboard` | implemented | `apps/web-club/src/pages/QrStudioPage.tsx:81`, `:85` ↔ `crates/gateway-api/src/panel_routes.rs:34`; для студии нужен только `id` — этого хватает | production | — |
| 23 | `/qr` (студия) | «использовать живой токен» → подставить в QR | `POST /v1/club/sessions/:id/qr/generate` | implemented | `apps/web-club/src/pages/QrStudioPage.tsx:87-97` ↔ `crates/gateway-api/src/sessions_routes.rs:151`; e2e `crates/gateway-api/src/e2e_sessions.rs:16` | production | — |

### 3.3 Состав DJ (`/djs`)

| # | Экран/маршрут | Действие или данные | Ожидаемый эндпоинт | Статус | Доказательство (frontend ↔ backend) | Влияние | Severity |
|---|---|---|---|---|---|---|---|
| 24 | `/djs` | список состава, автообновление каждые 20 с | `GET /v1/club/djs` | implemented | `apps/web-club/src/pages/DjsPage.tsx:119`, `:121` (`PRESENCE_POLL_MS=20_000`, `:43`) ↔ `crates/gateway-api/src/panel_routes.rs:44`, `:494`; e2e `crates/gateway-api/src/e2e_panels.rs:1507` | production | — |
| 25 | `/djs` | «показать снятых» → `?include_removed=1` | `GET /v1/club/djs?include_removed=1` | implemented | `apps/web-club/src/api/client.ts:926` ↔ `crates/gateway-api/src/panel_routes.rs:485-497`, `:520`; e2e `crates/gateway-api/src/e2e_panels.rs:1792` | production | — |
| 26 | `/djs` | пригласить DJ → 201 `{dj_id,invite_token,invite_expires_at}` | `POST /v1/club/djs/invite` | partial | `apps/web-club/src/pages/DjsPage.tsx:307`, `apps/web-club/src/api/client.ts:958-963` ↔ `crates/gateway-api/src/panel_routes.rs:45`, `:1074`, ответ `:1184-1192`; **письмо собирает другой URL**: `…/dj/invite?token=<hex>` (`panel_routes.rs:1261`), а SPA-маршрут — `/dj/invite/:token` (`apps/web-club/src/App.tsx:73`, `DjInvitePage.tsx:9`) → ссылка из письма не открывает форму | production | high |
| 27 | `/djs` | блокировка/разблокировка доступа | `PATCH /v1/club/djs/:id {status}` | implemented | `apps/web-club/src/pages/DjsPage.tsx:177` ↔ `crates/gateway-api/src/panel_routes.rs:46-49`, `:578` (ответ `{id,display_name,status,note}`, `:709-717`); e2e `crates/gateway-api/src/e2e_panels.rs:1578` | production | — |
| 28 | `/djs` | рабочая заметка (`null` — очистить) | `PATCH /v1/club/djs/:id {note}` | implemented | `apps/web-club/src/pages/DjsPage.tsx:216` ↔ `crates/gateway-api/src/panel_routes.rs:601-765` (разбор `Value`, различает «нет ключа» и `null`, `:561-567`, `:603-619`); e2e `crates/gateway-api/src/e2e_panels.rs:1599` | production | — |
| 29 | `/djs` | снять с состава (soft/hard) | `DELETE /v1/club/djs/:id` | implemented | `apps/web-club/src/pages/DjsPage.tsx:242` ↔ `crates/gateway-api/src/panel_routes.rs:48`, `:744` (soft `:821-862`, hard `:790-815`, отказ при открытой смене `:778-785`); e2e `crates/gateway-api/src/e2e_panels.rs:1810` | production | — |
| 30 | `/djs` | вернуть в состав (идемпотентно) | `POST /v1/club/djs/:id/restore` | implemented | `apps/web-club/src/pages/DjsPage.tsx:267` ↔ `crates/gateway-api/src/panel_routes.rs:50`, `:880`; e2e `crates/gateway-api/src/e2e_panels.rs:1821` | production | — |

### 3.4 Тарифы и доходы

| # | Экран/маршрут | Действие или данные | Ожидаемый эндпоинт | Статус | Доказательство (frontend ↔ backend) | Влияние | Severity |
|---|---|---|---|---|---|---|---|
| 31 | `/pricing` | история тарифов + активный | `GET /v1/club/pricing` | implemented | `apps/web-club/src/pages/PricingPage.tsx:137`, `:393` ↔ `crates/gateway-api/src/panel_routes.rs:39-42`, `:279-315`; e2e `crates/gateway-api/src/e2e_panels.rs:178` | production | — |
| 32 | `/pricing` | новая цена (старая закрывается) → 201 | `POST /v1/club/pricing` | partial | `apps/web-club/src/pages/PricingPage.tsx:198` ↔ `crates/gateway-api/src/panel_routes.rs:342-399`: ответ `{id, price_minor, dj_share_bp}` (`:371-375`) **без** `valid_from/valid_to`, клиент выдумывает их из `new Date()` (`apps/web-club/src/api/client.ts:898-912`); валидация: бэкенд `price_minor > 0` (`:331-333`), UI — 1..10 € (`PricingPage.tsx:26-27`, `:101-106`) | production | medium |
| 33 | `/earnings` | доходы за период (`period=today` жёстко) | `GET /v1/club/earnings?period=today` | partial | `apps/web-club/src/pages/EarningsPage.tsx:40`, `apps/web-club/src/api/client.ts:1115-1126` ↔ `crates/gateway-api/src/panel_routes.rs:43`, `:384-430`; **в ответе нет `currency`** (`:422-427`) → клиент печатает `EUR` константой (`client.ts:1122`) при `club.currency` в БД (`crates/infra-db/migrations/0001_init.sql:27`); `period` не валидируется и эхом возвращается (`panel_routes.rs:418-420`, `:423`); e2e `crates/gateway-api/src/e2e_panels.rs:287` (`?period=week`) | production | medium |

### 3.5 API v1 (`/developers`)

| # | Экран/маршрут | Действие или данные | Ожидаемый эндпоинт | Статус | Доказательство (frontend ↔ backend) | Влияние | Severity |
|---|---|---|---|---|---|---|---|
| 34 | `/developers` ключи | список ключей | `GET /v1/club/api-keys` | implemented | `apps/web-club/src/pages/DeveloperPage.tsx:229`, `apps/web-club/src/api/developer.ts:197-199` ↔ `crates/gateway-api/src/api_key_routes.rs:48-51`; e2e `crates/gateway-api/src/e2e_api_keys.rs:114` | production | — |
| 35 | `/developers` ключи | создать ключ (секрет один раз) | `POST /v1/club/api-keys` | implemented | `apps/web-club/src/pages/DeveloperPage.tsx:336` ↔ `crates/gateway-api/src/api_key_routes.rs:48-51`, `:73-85`; e2e `crates/gateway-api/src/e2e_api_keys.rs:114` | production | — |
| 36 | `/developers` ключи | ротация секрета (`grace_hours`) | `POST /v1/club/api-keys/:id/rotate` | implemented | `apps/web-club/src/pages/DeveloperPage.tsx:356`, `apps/web-club/src/api/developer.ts:217-222` ↔ `crates/gateway-api/src/api_key_routes.rs:59`, `:97-101`; e2e `crates/gateway-api/src/e2e_api_keys.rs:114` | production | — |
| 37 | `/developers` ключи | отозвать ключ → 204 | `DELETE /v1/club/api-keys/:id` | implemented | `apps/web-club/src/pages/DeveloperPage.tsx:369` ↔ `crates/gateway-api/src/api_key_routes.rs:53-58`, `:547` (`NO_CONTENT`); e2e `crates/gateway-api/src/e2e_api_keys.rs:114` | production | — |
| 38 | `/developers` журнал | сводка 24 ч/30 дней + 50 последних вызовов | `GET /v1/club/api-keys/:id/usage` | partial | `apps/web-club/src/pages/DeveloperPage.tsx:1055`, `:1097-1105` ↔ `crates/gateway-api/src/api_key_routes.rs:60`, `:665-679`; **`calls_30d` — это `count(*)` за всё время** (`crates/infra-db/src/api_keys.rs:679-685`), а подпись — «Calls, 30 days» (`apps/web-club/src/messages/developers.ts:146`), то же обещает `docs/openapi.yaml:3335-3350`, схема `ApiKeyUsage` — `:4149-4171`, поле `calls_30d` — `:4158`; e2e `crates/gateway-api/src/e2e_api_keys.rs:114` формы не проверяет | production | medium |
| 39 | `/developers` создание | каталог скоупов и пресетов | `GET /v1/club/api-keys/scopes` | implemented | `apps/web-club/src/pages/DeveloperPage.tsx:231`, `apps/web-club/src/api/developer.ts:237-239` ↔ `crates/gateway-api/src/api_key_routes.rs:52`, `:235-244`; e2e `crates/gateway-api/src/e2e_api_keys.rs:114` | production | — |
| 40 | `/developers` гайды | манифест разделов | `GET /v1/club/developers/docs` | partial | `apps/web-club/src/components/developers/GuidesTab.tsx:33`, `apps/web-club/src/api/developer.ts:241-243` ↔ `crates/gateway-api/src/developer_docs.rs:208`, `:84-109`; **e2e нет** (`grep` по `crates/gateway-api/src/e2e_*.rs` пуст) | production | low |
| 41 | `/developers` гайды | markdown раздела | `GET /v1/club/developers/docs/:section` | partial | `apps/web-club/src/components/developers/GuidesTab.tsx:53` ↔ `crates/gateway-api/src/developer_docs.rs:209`, `:228-248`; e2e нет | production | low |
| 42 | `/developers` reference | OpenAPI 3.1 JSON для таблицы операций | `GET /v1/club/developers/openapi.json` | partial | `apps/web-club/src/components/developers/ReferenceTab.tsx:52`, `apps/web-club/src/api/developer.ts:250-252` ↔ `crates/gateway-api/src/developer_docs.rs:210`, `:250-255`; e2e нет (есть только юнит-тест фильтрации скоупов `developer_docs.rs:257-287`) | production | low |

### 3.6 DJ-консоль (`/dj`)

| # | Экран/маршрут | Действие или данные | Ожидаемый эндпоинт | Статус | Доказательство (frontend ↔ backend) | Влияние | Severity |
|---|---|---|---|---|---|---|---|
| 43 | `/dj` вход из панели | клуб выписывает себе DJ-сессию на смену | `POST /v1/club/dj-console/session` | implemented | `apps/web-club/src/components/DjConsoleEntry.tsx:120`, `apps/web-club/src/api/client.ts:1008-1026` ↔ `crates/gateway-api/src/panel_routes.rs:94-97`, `:2566`; коды отказа `no_open_shift`/`dj_required`/`dj_unavailable`/`no_djs` (`:2591-2679`) совпадают с разбором в `DjConsoleEntry.tsx:40-65`, `:80-90`; e2e `crates/gateway-api/src/e2e_dj_console.rs:79` | production | — |
| 44 | `/dj` очередь | одноразовый тикет на WS (30 с) | `POST /v1/ws/ticket` | implemented | `apps/web-club/src/dj-console/useDjLiveQueue.ts:607`, `apps/web-club/src/api/client.ts:1079-1082` ↔ `crates/gateway-api/src/ws.rs:546`, `:508`; e2e `crates/gateway-api/src/e2e_ws.rs:335` | production | — |
| 45 | `/dj` очередь | подписка `{"subscribe":"queue","id":<session>}` на live-кадры | `GET /v1/ws?ticket=` | implemented | `apps/web-club/src/dj-console/useDjLiveQueue.ts:584`, `apps/web-club/src/api/client.ts:1091-1094` ↔ `crates/gateway-api/src/ws.rs:545`, `:561`; e2e `crates/gateway-api/src/e2e_ws.rs:61` | production | — |
| 46 | `/dj` очередь | очередь `pending/accepted`, `?sort=new|hot` | `GET /v1/dj/queue?sort=` | implemented | `apps/web-club/src/dj-console/useDjLiveQueue.ts:423`, `apps/web-club/src/api/client.ts:1040-1042` ↔ `crates/gateway-api/src/requests_routes.rs:169`, `:775-820` + `crates/infra-db/src/requests.rs:342-366`; e2e `crates/gateway-api/src/e2e_requests.rs:21` | production | — |
| 47 | `/dj` очередь | принять запрос | `POST /v1/dj/requests/:id/accept` | implemented | `apps/web-club/src/dj-console/useDjConsoleData.ts:156-161` ↔ `crates/gateway-api/src/requests_routes.rs:170`, `:822`; e2e `crates/gateway-api/src/e2e_requests.rs:21` | production | — |
| 48 | `/dj` очередь | отклонить с причиной | `POST /v1/dj/requests/:id/reject` | implemented | `apps/web-club/src/api/client.ts:1044-1048` (тело `{reason}`) ↔ `crates/gateway-api/src/requests_routes.rs:171`, `:830-844` (`RejectRequest { reason: Option<String> }`, `:89-92`); e2e `crates/gateway-api/src/e2e_requests.rs:21` | production | — |
| 49 | `/dj` очередь | отметить «сыграно» | `POST /v1/dj/requests/:id/played` | implemented | `apps/web-club/src/dj-console/useDjConsoleData.ts:156-161` ↔ `crates/gateway-api/src/requests_routes.rs:172`, `:846`; e2e `crates/gateway-api/src/e2e_requests.rs:21` | production | — |
| 50 | `/dj` комната | пауза приёма заявок | `POST /v1/dj/acceptance/pause` | implemented | `apps/web-club/src/dj-console/useDjConsoleData.ts:162-165` ↔ `crates/gateway-api/src/requests_routes.rs:173`, `:953`; e2e `crates/gateway-api/src/e2e_requests.rs` | production | — |
| 51 | `/dj` комната | возобновление приёма | `POST /v1/dj/acceptance/resume` | implemented | `apps/web-club/src/dj-console/useDjConsoleData.ts:162-165` ↔ `crates/gateway-api/src/requests_routes.rs:174`, `:957`; e2e `crates/gateway-api/src/e2e_requests.rs` | production | — |
| 52 | `/dj` (везде) | heartbeat присутствия раз в 30 с | `POST /v1/dj/presence` | implemented | `apps/web-club/src/dj-console/useDjConsoleData.ts:104-137` ↔ `crates/gateway-api/src/panel_routes.rs:52`, `:953`; e2e `crates/gateway-api/src/e2e_panels.rs:1649` | production | — |
| 53 | `/dj` вкладка Session | текущая смена (id/status/timezone/club_id) | `GET /v1/dj/shift` | implemented | `apps/web-club/src/dj-console/useDjConsoleData.ts:144-147` ↔ `crates/gateway-api/src/panel_routes.rs:91`, `:2335`; e2e `crates/gateway-api/src/e2e_panels.rs:333` | production | — |
| 54 | `/dj` вкладка Session | счётчики вечера + заработок | `GET /v1/dj/stats/today` | partial | `apps/web-club/src/dj-console/useDjConsoleData.ts:144-147`, `apps/web-club/src/api/client.ts:1060-1063` ↔ `crates/gateway-api/src/panel_routes.rs:92`, `:2365-2406`: `requests/accepted/played/rejected` — за 24 ч (`:2374-2384`), а `earned_total_minor` — **за всё время** (`:2385-2386`) и подписан в UI как «Session total» (`apps/web-club/src/pages/DJConsoleViews.tsx:481`); плюс «played» берётся из локального `dj.history` (`DJConsoleViews.tsx:388`, `:458`), который в live всегда пуст (`useDjConsoleData.ts:177`) → 0 | production | medium |

### 3.7 Публичные экраны и служебное

| # | Экран/маршрут | Действие или данные | Ожидаемый эндпоинт | Статус | Доказательство (frontend ↔ backend) | Влияние | Severity |
|---|---|---|---|---|---|---|---|
| 55 | все экраны (нижняя полоса) | режим стенда: test/maintenance/normal, fallback `/readyz` | `GET /v1/status` (+`/readyz`) | implemented | `apps/web-club/src/components/DeploymentStatus.tsx:109`, `:119-130` ↔ `crates/gateway-api/src/status_routes.rs:812`, `:146-165` (`mode`, `source`, `open_signup`, `payments`), `crates/gateway-api/src/routes.rs:91` (`/readyz`); e2e `crates/gateway-api/src/e2e_panels.rs:1917` | production | — |
| 56 | `/connect/qr` → `/connect/connected` | отправить заявку на подключение клуба | `POST /v1/club/onboarding/apply` | **missing** | `apps/web-club/src/pages/connect/QrStepPage.tsx:59`, `apps/web-club/src/api/connect.ts:93-126` ↔ маршрута нет ни в `crates/gateway-api/src/routes.rs:80-150` (общий router), ни в `docs/openapi.yaml` (`grep onboarding` даёт только Stripe-онбординг, `:2446`, `:2473`); клиент ловит 404/501 и честно уходит в «черновик сохранён локально» (`api/connect.ts:110-112`) | production | blocker (для воронки) |

---

## 4. Расхождения полей и контрактов

### 4.1 Поля, которые UI читает как обязательные, а бэкенд не отдаёт

| Что читает UI | Где читает | Что реально отдаёт бэкенд | Где отдаёт | Последствие | Severity |
|---|---|---|---|---|---|
| `session.timezone`, `session.dj_id` (для восстановленной смены) | `apps/web-club/src/pages/SessionsPage.tsx:170`, `:321-326` | `{session: {id, status}, requests_today, earned_total_minor}` — без `timezone`/`dj_id` | `crates/gateway-api/src/panel_routes.rs:229-233` | после F5 смена показывает «timezone unknown» и **«DJ не назначен»**, даже если DJ назначен (например, через handoff) — оператор видит ложный факт | high |
| `profile.city` | `apps/web-club/src/api/connect.ts:148` (`city: profile.country`) | `city` есть в БД (`crates/infra-db/migrations/0008_panel_tails.sql:30`), но **не селектится** и не отдаётся | `crates/gateway-api/src/panel_routes.rs:247-262` | в визарде «город» подставляется страной; в БД `club.city` пишется только при открытой регистрации (`signup_routes.rs:159`, `:193`) | medium |
| `profile.timezone` | нигде (не читается) | есть в БД (`0008_panel_tails.sql:31`), не отдаётся | `panel_routes.rs:247-262` | следствие: UI не знает зону клуба и открывает смену в `Europe/Madrid` константой (`SessionsPage.tsx:30`) — авто-закрытие в 07:00 чужой зоны (`sessions_routes.rs:466-471` считает `expires_at` по зоне сессии) | high |
| `profile.currency` | нигде | `club.currency` есть (`0001_init.sql:27`), не отдаётся | `panel_routes.rs:247-262` | деньги форматируются как EUR константой (`api/client.ts:1122`, `components/club/board/format.ts:29-30`) | medium |
| `profile.created_at`, `profile.stripe_account_id` | нигде | нет в ответе | `panel_routes.rs:255-262` | клиент подставляет `createdAt: ''`, `stripeAccountId: null` (`api/client.ts:842`, `:874`) — сегодня безвредно, но это фабрикация | low |
| `earnings.currency` | `apps/web-club/src/api/client.ts:1122` | нет в ответе (есть только `period`, `net_minor`, `payments`, `gross_minor`) | `panel_routes.rs:444-449` | валюта угадана | medium |
| `pricing.create.valid_from/valid_to` | `apps/web-club/src/api/client.ts:898-912` | 201 отдаёт только `id`, `price_minor`, `dj_share_bp` | `panel_routes.rs:394-397` | клиент выдумывает даты; сейчас маскируется тем, что страница сразу делает `reload()` (`PricingPage.tsx:206`) | low |
| `stats.played` для метрики «Played» | `apps/web-club/src/pages/DJConsoleViews.tsx:388`, `:458` | значение есть (`played`, `panel_routes.rs:2855`), но UI берёт `dj.history` — в live всегда `[]` | `crates/gateway-api/src/panel_routes.rs:2852-2856`; `apps/web-club/src/dj-console/useDjConsoleData.ts:177` | в production «Played» = 0 и CSV-экспорт пуст при реальных проигранных заявках | high |
| `api_key_usage.summary.calls_30d` | `apps/web-club/src/pages/DeveloperPage.tsx:1105` | SQL: `count(*)` без окна = **за всё время** | `crates/infra-db/src/api_keys.rs:679-685` | подпись «Calls, 30 days» лжёт (и openapi повторяет: `docs/openapi.yaml:4149-4171`, `:4158`) | medium |
| `dj.stats.earned_total_minor` как «Session total» | `apps/web-club/src/pages/DJConsoleViews.tsx:481` | сумма всех DJ-сплитов за всё время, без фильтра по смене | `panel_routes.rs:2840-2841` | сумма за ночь на самом деле — за всё время | medium |

### 4.2 Семантика значений (не ошибка формы, а ошибка смысла)

| Поле | На что рассчитывает UI | Что делает бэкенд | Доказательство |
|---|---|---|---|
| `GET /v1/club/earnings?period=` | `today` = последние 24 ч (так и подписано: `EarningsPage.tsx:121-124`, `client.ts:1097-1103`) | ОК: rolling-окно `interval '1 day'` — семантика совпадает | `panel_routes.rs:418-420` |
| `period=мусор` | ожидается 400/422 | молча `1 day`, а в ответе эхом `"period": "мусор"` | `panel_routes.rs:418-420`, `:423` |
| `GET /v1/dj/queue?sort=мусор` | ожидается 400 | молча сортировка `new` | `requests_routes.rs:809`, `crates/infra-db/src/requests.rs:348-351` |
| `GET /v1/club/dashboard.earned_total_minor` | «all time, settled» — подпись честная | сумма клубных сплитов без фильтра времени | `panel_routes.rs:219`, `useClubBoard.ts:28-31` |
| `GET /v1/club/dashboard.requests_today` | «последние 24 ч, без фильтра оплаты» | именно так | `panel_routes.rs:217` |
| `GET /v1/club/djs.online` | «heartbeat моложе 90 с» | `presence_age_seconds <= DJ_PRESENCE_TTL_SECS` (90) | `panel_routes.rs:480`, `:540-542` |
| `presence_ttl_seconds` | клиент парсит, но нигде не использует | отдаётся | `api/client.ts:953`, `panel_routes.rs:554` |

### 4.3 Query-параметры: что UI добавляет и что делает бэкенд

| Параметр | Кто добавляет | Принимает? | Что делает с невалидным | Пагинация/лимит |
|---|---|---|---|---|
| `?include_removed=1` | `apps/web-club/src/api/client.ts:926` | да: `1`/`true`/`yes` (`panel_routes.rs:485-497`) | `0`/`false`/мусор → только действующий состав, 200 | нет; ответ целиком, сортировка `removed_at NULLS FIRST, display_name` (`panel_routes.rs:538-540`) |
| `?period=today\|week\|month` | `apps/web-club/src/api/client.ts:1117` (значение всегда `today` — `EarningsPage.tsx:40`) | да (`panel_routes.rs:402-404`) | молча `1 day` + эхо значения в ответе (`:395-399`, `:423`) | нет |
| `?sort=new\|hot` | `apps/web-club/src/api/client.ts:1041` | да (`requests_routes.rs:62-64`, `:809`) | `hot` → `same_track_count DESC, created_at ASC`; любой другой → `created_at DESC` (`crates/infra-db/src/requests.rs:348-351`) | лимита нет — вся очередь |
| `?ticket=<hex>` (WS) | `apps/web-club/src/api/client.ts:1093` | да, одноразовый (`ws.rs:550-552`, `:566`) | пустой/чужой/просроченный → 401 (`ws.rs:566-572`) | TTL 30 с (`ws.rs:508-537`) |
| другие (`q`, `limit`, `cursor`, `sort` для роутов клуба) | — | **UI их не отправляет**; поиск/фильтр по составу локальный (`DjsPage.tsx:335`), пагинации в панели нет | — | `LIMIT 50` зашит для `/club/pricing` (`:297`) и `/club/payouts` (`:992`); у `/club/djs`, `/club/earnings`, `/club/dashboard` лимита нет вообще |

### 4.4 Коды ответов и ошибки

| Маршрут | Бэкенд | Клиент | Совпадает? |
|---|---|---|---|
| `POST /v1/club/sessions` | 201 `SessionResponse` (`sessions_routes.rs:426-434`) | читает `id/status/timezone/dj_id` (`client.ts:735-741`) | да |
| `POST /v1/club/pricing` | 201 `{id, price_minor, dj_share_bp}` (`panel_routes.rs:394-397`) | 200/201 не различает, поля маппит (`client.ts:898-912`) | форма — нет (§4.1) |
| `POST /v1/club/djs/invite` | 201 `{dj_id, invite_token, invite_expires_at}` (`:1184-1192`) | маппит ровно эти поля (`client.ts:958-963`) | да; но URL в письме — нет (§3, №26) |
| `DELETE /v1/club/djs/:id` | 200 `{id, display_name?, deleted, status}` (`:810-814`, `:853-862`) | optional `display_name` учтён (`client.ts:166-168`) | да |
| `PATCH /v1/club/djs/:id` без полей | 400 `{code,message}` (`:621-626`) | UI всегда шлёт одно поле (`DjsPage.tsx:177`, `:216`) | да |
| `POST /v1/club/dj-console/session` | 409 `no_open_shift`/`dj_required`/`no_djs`/`dj_unavailable`/`shift_changed` (`panel_routes.rs:3047-3117`) | 4 из 5 переведены в свои экраны; `shift_changed` → общий экран ошибки (`DjConsoleEntry.tsx:63-65`, `:161-175`) | да (UI-косметика) |
| `DELETE /v1/club/api-keys/:id` | 204 (`api_key_routes.rs:547`) | `request` возвращает `undefined` на 204 (`client.ts:513-514`) | да |
| `POST /v1/dj/requests/:id/reject` | требует JSON-тело (`requests_routes.rs:830-834`) | всегда шлёт `{reason}` (`client.ts:1044-1048`) | да |
| `POST /v1/club/onboarding/apply` | **404** (маршрута нет) | ловится как `NotImplementedStubError` (`api/connect.ts:110-112`) | ожидаемо, но это missing |

### 4.5 RBAC-гейт по путям

| Путь | Как защищён | Доказательство | Замечание |
|---|---|---|---|
| `POST /v1/auth/{role}/login|2fa`, `/refresh`, `/register`, `/password/reset*` | публично по замыслу | `crates/gateway-api/src/routes.rs:93-96`, `:99`, `:102` | регистрация дополнительно гейтится `DROPIT_OPEN_SIGNUP` → 403 `signup_disabled` (`signup_routes.rs:9`, `:126-128`) |
| `/v1/club/**` (панельные) | `AuthContext` проверяет JWT (401) + `if ctx.role() != Role::Club` → 403 в каждом хендлере | `crates/gateway-api/src/rbac.rs:107-123`; `panel_routes.rs:206`, `:217`, `:250`, `:280`, `:324`, `:389`, `:499`, `:584`, `:749`, `:885`; `sessions_routes.rs:398`, `:448`, `:632` | слой `role_route` навешен только на `/v1/club/echo` (`routes.rs:111-113`), остальное полагается на проверки в хендлерах — они есть на каждом пути, e2e подтверждает 403 (`e2e_panels.rs:364`) |
| Изоляция клубов | `club_id` берётся из токена и участвует в `WHERE` | `panel_routes.rs:538`, `:634`, `:762`, `:795`, `:894`; `sessions_routes.rs:462`, `:647` | чужой id → 404, e2e `e2e_panels.rs:1700`, `:1709` |
| `/v1/dj/**` | то же, роль DJ в хендлере | `requests_routes.rs:780`, `:863`, `:966`; `panel_routes.rs:977`, `:2336`, `:2366` | — |
| `POST /v1/ws/ticket` | **только «аутентифицирован»**, без ограничения роли | `crates/gateway-api/src/ws.rs:508-512` | клубный токен тоже может выписать тикет; подписка на `queue` валидируется позже (`ws.rs:655-661`) — не дыра в данных, но лишнее право; рекомендуется allowlist `Role::{Club,Dj}` |
| `POST /v1/dj/invite/accept` | публично (пароль задаётся по одноразовому токену) | `panel_routes.rs:51`, `:1032-1064` | токен 64 hex, хранится хешем (`:1042-1045`); rate-limit нет — перебор нереален, но 429 на IP был бы дешёвым усилением |
| `GET /v1/status`, `/readyz` | публично | `status_routes.rs:812`, `routes.rs:91` | утечки нет: только режим стенда |
| `/v1/club/developers/**` | club-only внутри хендлера | `developer_docs.rs:221-255` | — |

### 4.6 Наличие e2e-тестов

| Маршрут/группа | e2e | Доказательство |
|---|---|---|
| dashboard, profile GET/PATCH, pricing GET/POST, earnings, payouts, dj/shift, dj/stats, presence | да | `crates/gateway-api/src/e2e_panels.rs:49` (`e2e_panel_routes`), `:340`, `:333`, `:1649` |
| состав DJ: invite/accept/patch/delete/restore/include_removed | да | `crates/gateway-api/src/e2e_panels.rs:388`, `:1507` |
| сессии, QR generate/activate/close | да | `crates/gateway-api/src/e2e_sessions.rs:16`, `:505` |
| handoff в консоль | да | `crates/gateway-api/src/e2e_dj_console.rs:79` |
| API-ключи | да | `crates/gateway-api/src/e2e_api_keys.rs:114` |
| очередь/accept/reject/played/pause/resume | да | `crates/gateway-api/src/e2e_requests.rs:21` |
| WS ticket + сокет | да | `crates/gateway-api/src/e2e_ws.rs:61`, `:335` |
| регистрация, master-вход, reset | да | `crates/gateway-api/src/e2e_signup.rs`, `e2e_reset.rs`, `e2e_panels.rs:1156` |
| `/v1/club/developers/docs`, `/docs/:section`, `/openapi.json` | **нет** | в `e2e_*.rs` нет ни одного упоминания |
| онбординг-заявка | **нет** (маршрута нет) | — |
| браузерный e2e панели | **нет** (только транспортные тесты `scripts/tests/web-auth.test.mjs:30-99`) | — |
| дрейф `openapi.yaml` ↔ код | да, но только пути/методы | `crates/gateway-api/tests/openapi_contract.rs:239` (`every_code_route_is_described`), `:257` (`no_dead_contract_paths`); тела и схемы не сверяются |

---

## 5. Локальные фичи без бэкенда

| # | Фича | Где | Достижима в production (`IS_DEMO=false`)? | Как подана пользователю | Что нужно, чтобы стать живой |
|---|---|---|---|---|---|
| L1 | Заявка на подключение клуба (`/connect`, 4 шага) | `apps/web-club/src/api/connect.ts:93-126`, `pages/connect/QrStepPage.tsx:59-71` | **да** — весь публичный визард | честно: «черновик сохранён, менеджер свяжется» (`api/connect.ts:8-10`), локальный id `LOCAL-XXXX` (`:82-85`) | `POST /v1/club/onboarding/apply` + админ-очередь заявок (см. §6.1) |
| L2 | Зоны/заполненность на доске | `apps/web-club/src/dashboard/useClubBoard.ts:126-130` | да | `SourceChip` «preview» + баннер «часть данных — превью» (`DashboardPage.tsx:103-107`) | таблицы зон/сканеров + `GET /v1/club/zones` |
| L3 | DJ: now playing, история/лог смены, CSV-экспорт, tips | `apps/web-club/src/dj-console/useDjConsoleData.ts:175-195` (`nowPlaying`, `history: []`, `tips: []`) | да | **не** подписано как недоступное: tips и лог показывают обычные пустые состояния; «Played» = 0 | `GET /v1/dj/history`, `GET /v1/dj/tips`, `POST /v1/dj/tips/:id/acknowledge`, источник now-playing (см. §6.4, §6.6) |
| L4 | Телеметрия консоли: `requestsPerMinute`, `queueLatencyMs`, «events synced» | `useDjConsoleData.ts:179-186` | да | часть честно `unavailable` (`DJConsoleViews.tsx:471`), часть показывает 0 | считать на сервере или убрать |
| L5 | Правила комнаты (explicit/duplicates), жанровый фильтр, сообщение гостям, минимальная цена | `apps/web-club/src/pages/DJConsoleViews.tsx:270-300` | да | честная сноска «Live mode: the club API does not expose these rules to DJs, so nothing here is sent to the server» (`messages/dj-console.ts:150`) | `GET/PATCH /v1/club/sessions/:id/room` + проверка в `POST /v1/requests` (см. §6.5) |
| L6 | Кнопка «Пин» заявки | `DJConsoleViews.tsx` (ветка `live ? null : …`) | нет — в live скрыта | — | не требуется |
| L7 | Дизайн QR-табличек + пресеты | `apps/web-club/src/qr/studio-state.ts:195`, `:223` (`localStorage['dropit.qr.studio.v1']`) | да | «сохраняется в этом браузере» (по коду) | таблица `qr_design` + `GET/POST /v1/club/qr-designs`, если нужен общий доступ |
| L8 | Ссылка `/room` внутри QR-студии и консоли | `apps/web-club/src/pages/QrStudioPage.tsx:63-66` (`${origin}/room`), `pages/DJConsoleViews.tsx:358-360` (`/room/<session?>`, fallback `session_mock_01`) | да | ссылка показана как «ссылка комнаты» | **ни один деплой не обслуживает `/room`** (`deploy/prod/**` не содержит маршрута): нужна гостевая веб-страница или замена URL на deep-link мобильного гостя |
| L9 | Demo-сборка DJ-консоли целиком | `apps/web-club/src/dj-console/useDjConsoleMock.ts`, `mockData.ts`; гейт `useDjConsoleData.ts:167` | нет (только `--mode demo`; маршруты `/login`,`/dj/login` в demo редиректят, `App.tsx:26-50`) | бейдж demo | не пробел бэкенда |

**Отдельно про демо-сборку:** в demo-режиме клубные экраны (`/dashboard`, `/djs`, `/pricing`,
`/earnings`, `/qr`, `/developers`) **не замоканы** — они продолжают ходить в API без токена,
получают 401, и `useAsyncData` уводит на `/login`, который в demo редиректит обратно на
`/dashboard` (`apps/web-club/src/hooks/useAsyncData.ts:82-93`, `App.tsx:58`). Практически это
значит, что demo-сборка пригодна только для консоли DJ и доски, а остальные разделы из неё
«выкидывают». Это не пробел бэкенда, но стоит знать при демонстрациях.

---

## 6. Что нужно реализовать на бэкенде

Ниже — точные формы. Все суммы — minor units (`i64`), время — RFC3339/`timestamptz`,
`club_id`/`dj_id` **всегда** из JWT-контекста, не из тела. Каждый пункт = маршрут +
RBAC + тело/ответ + миграции + тесты.

### 6.1 `POST /v1/club/onboarding/apply` — публичная заявка на подключение (blocker)

- **RBAC:** публичный, без токена. Лимит: 5 заявок/час/IP (`client_ip` —
  `routes.rs:757-759`; готовый образец per-IP лимита — `issue_device`,
  `routes.rs:273-309`) → `429 rate_limited`.
- **Тело** (ровно то, что уже собирает `apps/web-club/src/api/connect.ts:19-79`):
  ```json
  {
    "club":  {"name":"...","city":"...","address":"...","website":"...","contact":"...","genres":["techno"]},
    "venue": {"name":"...","type":"club","address":"...","city":"...","district":"...","capacity":420,"seats":null,"area_m2":null},
    "qr":    {"price_minor":300,"zones":["main-stage","rooftop"]}
  }
  ```
- **Ответ `201`:**
  `{"application_id":"<uuidv7>","status":"pending_moderation","submitted_at":"<rfc3339>"}`
  (клиент уже читает именно эти три поля — `api/connect.ts:44-48`).
- **Ошибки:** `400 validation_failed` (`details.fields`), `429 rate_limited`,
  `503 external_service_unavailable` (нет БД).
- **Миграция `0012_club_application.sql`:**
  ```sql
  CREATE TABLE club_application (
    id uuid PRIMARY KEY,
    status text NOT NULL DEFAULT 'pending_moderation',  -- pending_moderation/approved/rejected
    club jsonb NOT NULL, venue jsonb NOT NULL, qr jsonb NOT NULL,
    contact_email citext NOT NULL,
    submit_ip inet,
    created_at timestamptz NOT NULL DEFAULT now(),
    decided_at timestamptz, decided_by uuid, decision_note text,
    club_id uuid REFERENCES club(id)
  );
  CREATE INDEX ON club_application (status, created_at DESC);
  ```
- **Админская пара (иначе заявку некому обрабатывать):**
  `GET /v1/admin/applications?status=&limit=` → `{items:[{id,status,created_at,club,venue,qr}]}`
  (`Role::Admin`); `POST /v1/admin/applications/:id/approve` → создаёт `club` (реюз
  `admin_clubs_create`, `panel_routes.rs:2462`) и возвращает одноразовый
  `master_token` + `club_id`; `POST .../reject {note}`.
- **Тесты `e2e_onboarding.rs`:** 201 + строка в БД; 400 на `capacity` отсутствующем;
  429 на 6-й заявке; approve → клуб создан, `master_token` работает на
  `POST /v1/auth/club/master` (`routes.rs:849`), повторный approve → 409.

### 6.2 `GET /v1/club/sessions/:id` (или расширить dashboard) — реальные `timezone`/`dj_id`

Предпочтительно маршрут (dashboard остаётся числовым):

- **RBAC:** `Role::Club`, только своя сессия (`club_id = $1`), иначе `404 session_not_found`.
- **Ответ `200`:**
  ```json
  {"id":"…","status":"active","timezone":"Europe/Madrid",
   "dj_id":"…|null","dj_display_name":"…|null",
   "created_at":"…","closes_at":"…|null",
   "qr_active":true,"price_minor":300}
  ```
  где `closes_at` — `expires_at` активного `qr_token` (или `null`), `qr_active` —
  из `qr_token.active` (та же выборка, что в `qr_set_active`, `sessions_routes.rs:512+`),
  `price_minor` — из активной `pricing_policy` (`valid_to IS NULL`).
- **Миграций нет** (всё в `session` + `qr_token` + `pricing_policy`).
- **Альтернатива, если новый маршрут нежелателен:** добавить `timezone`, `dj_id`,
  `dj_display_name` в `GET /v1/club/dashboard` (`panel_routes.rs:229-233`) — тогда правка
  только в `SessionsPage.tsx:109-112` (перестать обнулять поля). Минус: дашборд перестаёт
  быть «числовым» и не отдаёт `qr_active`/`price_minor`.
- **Тесты:** e2e — своя сессия 200/чужая 404/закрытая 200 со `status:"closed"`;
  после handoff (`e2e_dj_console.rs:79`) `dj_id` и `dj_display_name` заполнены.

### 6.3 Профиль клуба: вернуть то, что уже лежит в БД

- `GET /v1/club/profile` — добавить в `SELECT` и в JSON: `city`, `timezone`, `currency`,
  `created_at`, `stripe_account_id` (`panel_routes.rs:247-262`; колонки есть:
  `0008_panel_tails.sql:30-31`, `0001_init.sql:27`).
- `PATCH /v1/club/profile` — принимать `city`, `timezone` (валидация `chrono_tz::Tz`,
  как `sessions_routes.rs:411-414`), `currency` (ISO-4217 whitelist: `EUR|USD|GBP`),
  `logo_url` (уже есть). Ошибка → `422 validation_error` с `details.field`.
- **Миграций нет.** Аудит: строка `club.profile_update` в `audit_log`
  (как `dj.status_change`, `panel_routes.rs:710-720`).
- **Тесты:** e2e — PATCH timezone `Bad/Zone` → 422; PATCH timezone `UTC` → GET отдаёт `UTC`;
  чужой клуб не видит изменений (профиль берёт `club_id` из токена).

### 6.4 `GET /v1/dj/history?session_id=&limit=&cursor=` — лог смены для консоли

- **RBAC:** `Role::Dj`; `session.dj_id == ctx.subject_id()`, иначе `404`.
- **Ответ `200`:**
  ```json
  {"items":[{"id":"…","request_id":"…","status":"played|rejected",
             "occurred_at":"…","title":"…","artist":"…","style":null,
             "cover_url":null,"reject_reason":null,"dj_share_minor":0}],
   "next_cursor":"…|null"}
  ```
- **SQL:** `request` JOIN `track_cache` WHERE `r.session_id=$1 AND r.status IN ('played','rejected')`
  ORDER BY `COALESCE(r.played_at, r.created_at) DESC`; `played_at` уже есть
  (`crates/infra-db/migrations/0004_requests.sql:49`).
- **Миграция:** нужен `rejected_at timestamptz` (сейчас у отказа нет времени) —
  `ALTER TABLE request ADD COLUMN rejected_at timestamptz;` + заполнение в
  `RequestRepo` при переходе в `rejected`.
- **Тесты:** `e2e_dj_console.rs` — accept→played появляется в истории с `played_at`;
  reject попадает туда же; чужой session_id → 404.

### 6.5 Настройки комнаты смены (правила/жанры/сообщение/цена) + их соблюдение

- `GET /v1/club/sessions/:id/room` → `200`
  `{"session_id":"…","intake_paused":false,"min_price_minor":300,"guest_message":"…","allow_explicit":true,"block_duplicates":false,"genres":["techno","house"],"updated_at":"…"}`
- `PATCH /v1/club/sessions/:id/room` (RBAC `Role::Club`, своя сессия) — те же поля,
  частичный патч; `genres` — 0..8 строк 2..40 символов (как `update_dj_styles`,
  `catalog_routes.rs:363-391`). DJ-вариант: `PATCH /v1/dj/room` для своей смены
  (`Role::Dj`, `session.dj_id = sub`).
- **Соблюдение:** `POST /v1/requests` (`requests_routes.rs:184+`, гейты внутри
  `RequestRepo::create_request`) должен возвращать:
  `409 duplicate_blocked` (если `block_duplicates` и такой `canonical_key` уже в очереди),
  `409 genre_not_allowed` (стиль вне `genres`), `409 session_min_price` (`price_minor < min`).
- **Миграция `0013_session_room.sql`:**
  ```sql
  ALTER TABLE session ADD COLUMN min_price_minor bigint;
  ALTER TABLE session ADD COLUMN guest_message text;
  ALTER TABLE session ADD COLUMN allow_explicit boolean NOT NULL DEFAULT true;
  ALTER TABLE session ADD COLUMN block_duplicates boolean NOT NULL DEFAULT false;
  ALTER TABLE session ADD COLUMN genres text[];
  ```
- **Тесты:** e2e — PATCH room → `POST /v1/requests` отклоняется по каждому правилу;
  пауза/резюм (`/v1/dj/acceptance/*`) и `room.intake_paused` не противоречат друг другу.

### 6.6 `GET /v1/dj/tips?session_id=` + `POST /v1/dj/tips/:id/acknowledge`

- **RBAC:** `Role::Dj`, только свои смены.
- **Ответ:** `{"items":[{"id":"…","request_id":"…|null","amount_minor":200,"currency":"EUR","status":"received|acknowledged","received_at":"…","guest_name":"…|null"}]}`;
  ack → `200 {id,status:"acknowledged",acknowledged_at}` (идемпотентно: повтор 200).
- **Миграция `0014_tips.sql`:** `tip(id uuid pk, session_id uuid, request_id uuid null,
  guest_device_id uuid null, dj_id uuid, amount_minor bigint, currency text,
  status text default 'received', received_at timestamptz default now(), acknowledged_at timestamptz)`
  + индексы `(dj_id, status)`, `(session_id)`.
- Пока таблицы нет — UI обязан показывать tips как `unavailable`, а не как «0».

### 6.7 `GET /v1/club/earnings` — валюта и строгий `period`

- Добавить `"currency": club.currency` в ответ (`panel_routes.rs:444-449`, join по `club`);
  клиент уже читает `r.currency`-контракт (`api/client.ts:1122`), только игнорирует.
- `period`: `Option<String>` → `enum EarningsPeriod { Today, Week, Month }` через
  `serde`/ручной матч; неизвестное значение → `400 invalid_period` с
  `details: {allowed:["today","week","month"]}` (сейчас молча 1 день + эхо).
- **Тесты:** e2e — `?period=bogus` → 400; `currency` = `club.currency`; окна 1/7/30 дней
  (уже есть `e2e_panels.rs:287` для `week`).

### 6.8 `GET /v1/dj/stats/today` — разделить «сегодня» и «за всё время»

- Ответ: `{"requests_today":N,"accepted":N,"played":N,"rejected":N,
  "earned_today_minor":N,"earned_total_minor":N,"session_earned_minor":N|null}`,
  где `earned_today_minor` — DJ-сплиты по платежам, захваченным за 24 ч
  (`split ... JOIN payment p ... p.captured_at > now()-interval '24 hours'`),
  `earned_total_minor` — текущий (без окна), `session_earned_minor` — по открытой смене
  (`r.session_id = <open session>`).
- UI: «Session total» → `session_earned_minor`; «Played» → `stats.played` (не
  `dj.history.length`).
- **Тесты:** e2e — три разных окна дают три разных числа на подготовленных платежах.

### 6.9 `GET /v1/club/api-keys/:id/usage` — починить смысл `calls_30d`

- Либо окно: `count(*) FILTER (WHERE at > now() - interval '30 days')`
  (`crates/infra-db/src/api_keys.rs:679-685`), либо переименовать поле в `calls_total`
  и поправить UI/`docs/openapi.yaml:4158`.
- **Тест:** вставить вызов старше 30 дней → `calls_30d` его не считает, `calls_total` — считает.

### 6.10 Письмо-приглашение DJ: путь вместо query

- `crates/gateway-api/src/panel_routes.rs:1261`:
  `"{}/dj/invite?token={}"` → `"{}/dj/invite/{}"` (маршрут SPA —
  `apps/web-club/src/App.tsx:73`, `DjInvitePage.tsx:9`).
- Заодно поддержать оба варианта в SPA (`useSearchParams` fallback), чтобы письма,
  уже ушедшие со старой ссылкой, продолжали работать.
- **Тест:** e2e_mail — в письме `dj.invite` URL содержит `/dj/invite/<64hex>` и не содержит
  `?token=`.

### 6.11 2FA: промежуточный mfa-тикет

- `POST /v1/auth/{role}/login` при включённой 2FA возвращает
  `{"mfa_required":true,"mfa_ticket":"<одноразовый, 5 мин, хранится хешем>"}`
  (сейчас только `{"mfa_required":true}`, `routes.rs:71-73`, `:379`).
- `POST /v1/auth/{role}/2fa` принимает `{mfa_ticket, totp_code}` (email больше не нужен),
  тикет гасится `GETDEL`-семантикой; без тикета → `401 mfa_ticket_required`.
- **Миграция:** `mfa_ticket(hash text pk, role text, subject_id uuid, expires_at timestamptz,
  used_at timestamptz)` — либо Redis, если он есть в деплое (как `ws::TicketStore`).
- **Тесты:** e2e — код без тикета → 401; тикет одноразовый; истёкший → 401.

### 6.12 Мелочи (одной строкой каждая)

1. `GET /v1/club/djs` — добавить `?q=` (по `display_name/email`, `ILIKE`) и `?limit=`,
   когда состав вырастет; сейчас UI не просит, поэтому это не пробел, а запас.
2. `GET /v1/club/dj-console/target` — либо реализовать (read-only превью того, кого
   возьмёт `POST …/session`), либо убрать упоминание из комментария
   `panel_routes.rs:2875-2877`: маршрута нет.
3. `GET /v1/club/payouts` (`panel_routes.rs:53`) — есть на бэкенде, UI не вызывает:
   нужен экран «Выплаты» или явное решение, что выплаты клубу не показываем.
4. Stripe-онбординг клуба (`/v1/club/stripe/onboarding-status|link`) — то же: нет UI.
5. `DELETE /v1/club/api-keys/:id` + `PATCH /v1/club/api-keys/:id` — PATCH/get одного ключа
   не используются: в панели нет «переименовать/сузить скоупы».
6. `POST /v1/club/sessions/:id/qr/activate|deactivate` (`sessions_routes.rs:152-153`) —
   нет UI; студия и сессии умеют только `generate`.
7. `PATCH /v1/dj/styles` (`catalog_routes.rs:115`) — в web-club не вызывается (вероятно,
   только мобильный DJ); панель жанровых предпочтений DJ не имеет.

---

## 7. Топ-10 по приоритету

| # | Пробел | Почему первым | Где | Severity |
|---|---|---|---|---|
| 1 | `POST /v1/club/onboarding/apply` отсутствует — публичная воронка `/connect` не доходит до сервера | единственный `missing`; весь маркетинговый вход в продукт заканчивается «сохранено локально», заявки теряются | §3 №56, §6.1 | blocker |
| 2 | Нет способа узнать `timezone`/`dj_id` открытой смены → после F5 экран утверждает «DJ не назначен», смена открывается в `Europe/Madrid` | ложный операционный факт + авто-закрытие в чужой зоне; виден каждую смену | §4.1, §6.2 | high |
| 3 | DJ-консоль: «Played» и лог смены берутся из пустого локального `history`, а не из API | в production метрика и CSV всегда нулевые при живых данных | §4.1, §6.4 | high |
| 4 | Письмо-приглашение DJ ведёт на `/dj/invite?token=…`, а SPA ждёт `/dj/invite/:token` | приглашённый DJ не может активировать аккаунт по ссылке из письма | §3 №26, §6.10 | high |
| 5 | 2FA без промежуточного тикета: шаг 2 не доказывает, что пароль проверен | безопасность аккаунтов клуба/DJ | §3 №2/№8, §6.11 | high |
| 6 | `club.timezone/city/currency` есть в БД, но не отдаются и не редактируются | панель не может настроить свой профиль; валюта и зона — константы | §4.1, §6.3 | high |
| 7 | Local-only правила комнаты (explicit/duplicates/жанры/сообщение) при живой кнопке «Apply in this preview», которая на самом деле меняет production-состояние приёма | DJ думает, что настраивает превью, а реально закрывает/открывает приём заявок | §5 L4/L5, §6.5 | medium |
| 8 | Tips/now-playing/телеметрия в консоли — локальные нули без пометки «недоступно» | оператор видит «0 чаевых» как факт | §5 L3/L4, §6.6 | medium |
| 9 | `calls_30d` в usage-журнале = за всё время; `earned_total_minor` в «Stats today» = за всё время | две метрики с неверной подписью в двух разных экранах | §4.1, §4.2, §6.8, §6.9 | medium |
| 10 | Нет e2e и валидации у трёх `/v1/club/developers/*`; `period`/`sort` не валидируются (мусор → тихий дефолт и эхо) | документация разработчика — витрина API v1, а контракт не проверяется тестом | §4.2, §4.3, §4.6 | low–medium |

---

## Приложение A. Инвентарь: что прочитано и чем подтверждено

**`apps/web-club`** — `src/App.tsx` (маршруты production и demo), `src/pages/*.tsx`
(Dashboard, Sessions, Pricing, Djs, Earnings, QrStudio, Developer, Login, Register,
DjLogin, DjInvite, ForgotPassword, ResetPassword, DjConsole + `DJConsoleViews`),
`src/pages/connect/*` (Start/Club/Venue/Qr/Connected), `src/api/{client,connect,developer}.ts`,
`src/dashboard/useClubBoard.ts`, `src/dj-console/{useDjConsoleData,useDjLiveQueue,types,mockData}.ts`,
`src/hooks/useAsyncData.ts`, `src/components/{Layout,DjConsoleEntry,DeploymentStatus}.tsx`,
`src/components/developers/*`, `src/components/club/board/*`, `src/qr/*`,
`src/messages/*` (ключи `dj.applyInPreview`, `dj.rulesLiveNote`, `developers.usageCalls30d`).

**`crates/gateway-api`** — `routes.rs` (router + RBAC + master-login), `panel_routes.rs`
(все клубные/DJ/админские панельные роуты), `sessions_routes.rs`, `requests_routes.rs`,
`catalog_routes.rs`, `api_key_routes.rs`, `developer_api.rs`, `developer_docs.rs`, `ws.rs`,
`status_routes.rs`, `signup_routes.rs`, `reset_routes.rs`, `mail_routes.rs`, `rbac.rs`,
`e2e_panels.rs`, `e2e_sessions.rs`, `e2e_dj_console.rs`, `e2e_api_keys.rs`,
`e2e_requests.rs`, `e2e_ws.rs`, `e2e_signup.rs`, `e2e_reset.rs`,
`tests/openapi_contract.rs`.

**Схема/контракты** — `crates/infra-db/migrations/0001_init.sql`, `0004_requests.sql`,
`0005_payments.sql`, `0008_panel_tails.sql`; `docs/openapi.yaml`; `crates/infra-db/src/requests.rs`,
`crates/infra-db/src/api_keys.rs`.

**Чем подтверждено, что маршрута нет** — `grep -n "onboarding" docs/openapi.yaml` даёт только
Stripe (`:2446`, `:2473`); в `crates/gateway-api/src/routes.rs:80-150` нет ни `merge` для
onboarding-роутера, ни соответствующего модуля; `crates/gateway-api/tests/openapi_contract.rs:239`
требует, чтобы каждый код-маршрут был описан, т.е. «в коде есть, в спеке нет» исключено тестом.

## Приложение B. Ограничения аудита

- `cargo test` не запускался (правило задачи); «есть e2e» = тест-функция прочитана, а не прогнана.
- Рабочее дерево менялось во время аудита (см. шапку): строки в `apps/web-club/src/pages/DJConsoleViews.tsx`,
  `src/dj-console/*`, `src/api/client.ts`, `src/pages/SessionsPage.tsx` сверены после последней
  правки, но при следующем рефакторинге этих файлов номера сдвинутся — якоря (имена функций/ключей)
  указаны в тексте, чтобы перепроверка была однострочной.
- Браузерная проверка UI не выполнялась (не требуется задачей); выводы о поведении экранов
  сделаны по коду.