# Аудит: `apps/web-admin` → бэкенд (матрица «экран → эндпоинт → статус»)

Дата: 2026-10-29. Ревизия: `5f861be` (`main`) + рабочее дерево (в нём изменён `apps/web-admin/src/App.tsx`; цитаты ниже — по рабочему дереву).
Задача: `task-2`. Автор: `audit-admin`.

**Как проверялось.** Только чтение: `apps/web-admin/src/**`, `crates/gateway-api/src/**`,
`crates/infra-db/src/{identity,api_keys,mail}.rs`, `crates/infra-db/migrations/*.sql`,
`docs/openapi.yaml`; фактическая схема БД — `psql "$DATABASE_URL"` на локальном Postgres
(`:55433`, `scripts/agent-db.sh status` → running). `cargo test` **не запускался** (запрет
задачи): наличие e2e подтверждено чтением `e2e_*.rs`, а не прогоном. Код приложения и
бэкенда не менялся.

---

## 1. Резюме

Проверено **20 экранов-маршрутов** (`apps/web-admin/src/App.tsx:100-131`) + **5 алиасов**
(`/requests`, `/moderation`, `/roles`, `/feature-flags`, `/integrations`) и **28 операций
клиента** (27 сетевых: 3 авторизационных + 24 админских; 1 клиентская заглушка 501).
Дополнительно — 2 публичные пробы статуса (`/v1/status`, `/readyz`).

| Статус | Кол-во | Доля | Что это |
|---|---:|---:|---|
| `implemented` | **18** | 33% | маршрут зарегистрирован, метод/RBAC/поля сходятся, e2e есть |
| `partial` | **9** | 16% | маршрут есть, но расходятся поля/типы/пагинация/коды, либо нет теста |
| `missing` | **19** | 35% | операции UI без маршрута (в основном — Control center) |
| `stub` | **1** | 2% | `GET /v1/admin/clubs/:id` — клиент отдаёт `NotImplementedStubError`, на бэкенде только `PATCH` (GET → 405) |
| `local-only` | **8** | 15% | работает на `adminMock.ts`/`localStorage`, серверной части нет |
| **Всего** | **55** | 100% | строки таблицы §3 |

Три главных вывода:

1. **Control center — витрина без бэкенда.** Все 8 разделов работают на локальном сторе
   (`apps/web-admin/src/data/adminMock.ts:220-234`; пять пишут в него, `access` и `settings`
   держат собственные ключи — `RolePermissionMatrix.tsx:14`, `AdminSettingsPage.tsx:17`,
   `overview` только читает). Эти маршруты **входят в production-сборку**
   (`App.tsx:124-130`, навигация `components/Layout.tsx:16-52`): пользователь видит
   «создано/удалено/откатано», хотя на сервере не изменилось ничего.
2. **Живые экраны врут в трёх местах.** `/clubs` и `/djs` показывают Stripe-статус
   константой `'pending'` (`api/client.ts:526`, `:583`), `/payments` не показывает
   возвраты и причины отказа (`api/client.ts:625-630`), хотя эти колонки есть в БД
   (`crates/infra-db/migrations/0005_payments.sql:27-41`).
3. **RBAC админки — один флаг.** `admin_user.role` (`.../0002_identity.sql:79`,
   значения `superadmin/operator/readonly`) **не читается ни одним запросом**
   (`crates/infra-db/src/identity.rs:118-126`): любой админ получает полный доступ.
   Матрица ролей в UI — `localStorage` (`components/admin-console/RolePermissionMatrix.tsx:14`).

---

## 2. Как читать статусы

| Статус | Определение (из task-2) |
|---|---|
| `implemented` | маршрут реально зарегистрирован (`.route(...)`), метод совпадает, RBAC корректен, тело/ответ соответствуют тому, что читает UI, **есть e2e-тест** |
| `partial` | маршрут есть, но расходятся поля/типы/query-параметры/коды ответа, **либо нет теста** |
| `missing` | UI-операция без маршрута |
| `stub` | 501 / заглушка |
| `local-only` | работает на mock/localStorage, серверной части нет |

`Влияние`: `production` — экран/вызов входят в `npm run build` (не-demo);
`production (общий бандл)` — Control center, он в обеих сборках, но данные локальные.
Demo-сборка (`npm run build:demo`) — см. §6.

---

## 3. Полная таблица сценариев

Колонки — по task-2. `↔` разделяет фронтенд и бэкенд.

### 3.1 Авторизация и живые экраны (production)

| # | Экран/маршрут | Действие или данные | Ожидаемый эндпоинт | Статус | Доказательство (frontend ↔ backend) | Влияние | Severity |
|---:|---|---|---|---|---|---|---|
| 1 | `/login` | email+пароль → TOTP → пара токенов | `POST /v1/auth/admin/login`, `POST /v1/auth/admin/2fa` | implemented | `pages/LoginPage.tsx:31-46,71-90`; `api/client.ts:411-435` ↔ `crates/gateway-api/src/routes.rs:93-94,318-442`; e2e 2FA: `e2e.rs:485-570` (ветка `admin/login` отдельным тестом не покрыта, общий `:role` тестируется на dj/club — `e2e.rs:332`, `e2e_reset.rs:143`) | production | Low |
| 2 | `/dashboard` | 4 KPI «сейчас» | `GET /v1/admin/dashboard/now` | implemented | `pages/DashboardPage.tsx:30`; `api/client.ts:500-509` ↔ `panel_routes.rs:63,1500-1533`; e2e `e2e_panels.rs:299` | production | Low (оборот считается по `payment.created_at`, а не `captured_at` — `panel_routes.rs:1512`) |
| 3 | `/clubs` | список клубов | `GET /v1/admin/clubs` | partial | `api/client.ts:512-531` (7 полей синтезируются: `slug/address/country/phone/logoUrl/stripeAccountId/createdAt`, `stripeOnboardingStatus:'pending'` — `:526`) ↔ `panel_routes.rs:1543-1562` (6 полей: `id,name,city,email,status,turnover_minor`); e2e `e2e_panels.rs:303` | production | **High**: колонка Stripe всегда «pending»; `LIMIT 100` без пагинации (`panel_routes.rs:1548`) |
| 4 | `/clubs` (форма) | создание клуба + `qr_master_token` | `POST /v1/admin/clubs` | implemented | `pages/ClubsPage.tsx:53-68`; `api/client.ts:533-543` ↔ `panel_routes.rs:64,2007-2090`; e2e `e2e_panels.rs:729` | production | Low |
| 5 | `/clubs` | activate / suspend / block | `PATCH /v1/admin/clubs/:id` | implemented | `pages/ClubsPage.tsx:70-88`; `api/client.ts:551-572` ↔ `panel_routes.rs:65,1576-1624`; e2e `e2e_panels.rs:311` | production | Low |
| 6 | карточка клуба | `GET /v1/admin/clubs/:id` | — | **stub** | `api/client.ts:546-548` (`NotImplementedStubError`, 501) ↔ на `panel_routes.rs:65` только `patch()` → GET даёт **405**; `docs/openapi.yaml:2567` — только `patch` | production | Medium |
| 7 | клуб: правка name/address/phone, удаление | правка полей, удаление клуба | `PATCH /v1/admin/clubs/:id` (шире), `DELETE /v1/admin/clubs/:id` | missing | `api/client.ts:150-156` тип обещает `name/address/country/phone`, но `:552-553` шлёт только `status`; `ClubPatch` = только `status` (`panel_routes.rs:1567-1572`); `DELETE` не зарегистрирован (`panel_routes.rs:65`) | production (в CC операции есть — `AdminManagementView.tsx:371,378`) | High |
| 8 | `/djs` | список DJ | `GET /v1/admin/djs` | partial | `api/client.ts:575-588` (`stripeOnboardingStatus:'pending'` — `:583`, `createdAt:''`) ↔ `panel_routes.rs:74,1967,1978-1980` (5 полей; `dj.stripe_onboarding_status` в SELECT нет, хотя колонка есть — `0002_identity.sql:40`); **e2e отсутствует** (путь есть только в объявлении роутера) | production | **High**: Stripe-колонка всегда «pending» |
| 9 | DJ: управление из админки | создать/пригласить/сменить статус/удалить | `POST/PATCH/DELETE /v1/admin/djs*` | missing | в `panel_routes.rs:31-87` нет админ-маршрутов DJ; инвайт/статус/удаление есть только у клуба: `panel_routes.rs:45-50,578-945,1074-1193` | production | High |
| 10 | `/guests` | список гостей по клубу | `GET /v1/admin/guests?club_id=` | implemented | `pages/GuestsPage.tsx:20-23`; `api/client.ts:590-603` ↔ `panel_routes.rs:66,2107-2198`; e2e `e2e_panels.rs:832-1030` | production | Low (UI игнорирует `next_cursor` — >50 гостей не увидеть, §4.7) |
| 11 | бан/разбан гостя | забанить/разбанить устройство | `PATCH /v1/admin/guests/:id {banned}` | missing | `guest_device.banned` только читается (`panel_routes.rs:2154,2192`); запись `banned` отсутствует во всём `crates/` (колонка — `0008_panel_tails.sql:39`); кнопка бана есть лишь в CC: `AdminActivityPage.tsx:77-86` | production | Medium |
| 12 | `/payments` | список платежей, фильтр `status` | `GET /v1/admin/payments` | partial | `api/client.ts:612-633`: `currency:'EUR'`, `refundedMinor:0`, `failureReason:null`, `capturedAt:null` ↔ `panel_routes.rs:1682` не селектит `currency/refunded_minor/failure_reason/captured_at`, хотя колонки есть (`0005_payments.sql:33,35,36,40`); e2e `e2e_panels.rs:1369` | production | **High**: возвраты (`PaymentsPage.tsx:104-108`) и причины отказа (`:113-117`) не видны никогда |
| 13 | возврат / захват платежа админом | refund / capture | `POST /v1/admin/payments/:id/{refund,capture}` | missing | нет маршрутов: только `POST /v1/payments/:id/confirm` (`payments_routes.rs:105,358`); refund приходит лишь вебхуком (`payments_routes.rs:581`) | production (кнопки Capture/Refund — только в CC: `AdminManagementView.tsx:129,213-214`) | High |
| 14 | `/payouts` | список выплат | `GET /v1/admin/payouts` | implemented | `pages/PayoutsPage.tsx:28`; `api/client.ts:468-470` ↔ `payouts_routes.rs:97,271-304`; e2e `e2e_payouts.rs:75` | production | Low |
| 15 | `/payouts` | ручной цикл выплат | `POST /v1/admin/payouts/run` | implemented | `PayoutsPage.tsx:42` ↔ `payouts_routes.rs:96,150-269`; e2e `e2e_payouts.rs:584` | production | Low |
| 16 | `/payouts` | ручная сверка | `POST /v1/admin/reconciliation/run` | implemented | `PayoutsPage.tsx:60` ↔ `payouts_routes.rs:98,488-515`; e2e `e2e_payouts.rs:123` | production | Low |
| 17 | `/incidents` | открытые инциденты | `GET /v1/admin/incidents` | implemented | `IncidentsPage.tsx:24`; `api/client.ts:482-484` ↔ `payouts_routes.rs:99,517-542`; e2e `e2e_payouts.rs:132` | production | Low |
| 18 | `/incidents` | резолв инцидента | `POST /v1/admin/incidents/:id/resolve` | implemented | `IncidentsPage.tsx:31-45` ↔ `payouts_routes.rs:100,544-573`; e2e `e2e_payouts.rs:292` | production | Low |
| 19 | `/audit-log` | журнал (последние 100) | `GET /v1/admin/audit-log` | partial | `api/client.ts:636-649` (`diff:null` — `:645`) ↔ `panel_routes.rs:1766` (SELECT без `diff`, колонка есть — `0001_init.sql:45`); нет `limit/cursor`, жёсткий `LIMIT 100` (`:1767`); e2e `e2e_panels.rs:319` | production | Medium: до/после изменения не восстановить |
| 20 | `/platform-settings` | чтение настроек | `GET /v1/admin/platform-settings` | partial | `api/client.ts:660-672`: `updatedAt` из отсутствующего `updated_at` (`:670`), `commission_mode:'unset'` → `'percent'` (`:662`) ↔ `panel_routes.rs:1810-1831` (нет `updated_at`; `unset` — валидное состояние, `0001_init.sql:21,32`); e2e `e2e_panels.rs:94` | production | **High**: на неинициализированном стенде экран показывает «percent 0%» и сохранит это |
| 21 | `/platform-settings` | запись настроек | `PATCH /v1/admin/platform-settings` | partial | `pages/PlatformSettingsPage.tsx:43-54` (никогда не шлёт `clear_*`, не умеет «не задано») ↔ `panel_routes.rs:1837-1861,1909-1934`; e2e `e2e_panels.rs:1907` | production | Medium |
| 22 | `/api-keys` | список всех ключей | `GET /v1/admin/api-keys?limit=200` | partial | `api/developer.ts:23-40` (тип требует `club_id`), `:84-86` ↔ `api_key_routes.rs:194-200` (добавляет только `club_name`), `infra-db/src/api_keys.rs:144-161` (в `ApiKeyView` нет `club_id`); e2e `e2e_api_keys.rs:646` | production | Low (UI использует `club_name \|\| club_id`) |
| 23 | `/api-keys` | отзыв ключа | `POST /v1/admin/api-keys/:id/revoke` | implemented | `ApiKeysPage.tsx:110-119` ↔ `api_key_routes.rs:64,605-643` (204); e2e `e2e_api_keys.rs:690` | production | Low |
| 24 | `/api-keys` | сводка + журнал вызовов | `GET /v1/admin/api-keys/:id/usage` | implemented | `ApiKeysPage.tsx:370-460`; `api/developer.ts:60-77` ↔ `api_key_routes.rs:65,645-662`, `api_keys.rs:209-214,655-671`; e2e `e2e_api_keys.rs:715` | production | Low |
| 25 | `/mail` | журнал писем, фильтры `status/q/cursor` | `GET /v1/admin/mail` | implemented | `MailPage.tsx:440-449`; `api/client.ts:795-803` ↔ `mail_routes.rs:58,231-314`; e2e `e2e_mail.rs:331-400` | production | Low |
| 26 | `/mail` | KPI доставки 24 ч | `GET /v1/admin/mail/stats` | implemented | `MailPage.tsx:437`; `api/client.ts:806-808` ↔ `mail_routes.rs:60,317-339`; e2e `e2e_mail.rs:429` | production | Low |
| 27 | `/mail` (drawer) | письмо + html/text/vars/events | `GET /v1/admin/mail/:id` | implemented | `MailPage.tsx:122`; `api/client.ts:811-813` ↔ `mail_routes.rs:67,342-386`; e2e `e2e_mail.rs:405` | production | Low |
| 28 | `/mail` (drawer) | переотправка | `POST /v1/admin/mail/:id/resend` | implemented | `MailPage.tsx:175` ↔ `mail_routes.rs:68,392-421`; e2e `e2e_mail.rs:436` | production | Low |
| 29 | `/mail` | подавления (suppressions) | `GET /v1/admin/mail/suppressions?limit=` | implemented | `MailPage.tsx:451`; `api/client.ts:824-828` ↔ `mail_routes.rs:61,424-459`; e2e `e2e_mail.rs:488` | production | Low |
| 30 | `/mail` | снятие подавления | `DELETE /v1/admin/mail/suppressions/:email` | implemented | `MailPage.tsx:498-510` ↔ `mail_routes.rs:62-65,462-483`; e2e `e2e_mail.rs:501` | production | Low |

### 3.2 Control center (`apps/web-admin/src/data/adminMock.ts`) — локальные операции

Все строки 31-53 используют один и тот же стор: `data/adminMock.ts:220-234` (localStorage
`dropit.admin.mock.v1`), хуки `hooks/useAdminData.ts:31-77`.

| # | Экран/маршрут | Действие или данные | Ожидаемый эндпоинт | Статус | Доказательство (frontend ↔ backend) | Влияние | Severity |
|---:|---|---|---|---|---|---|---|
| 31 | `/` (overview) | 4 KPI: клубы/DJ/оборот/платежи | `GET /v1/admin/dashboard/now` | local-only | `AdminOverviewPage.tsx:69-71,79-87` (считает по mock-массивам) ↔ эндпоинт есть (`panel_routes.rs:1500`) и e2e `e2e_panels.rs:299`, но экран его не вызывает | production (общий бандл) | **High**: цифры не имеют отношения к платформе |
| 32 | `/` (overview) | тренд запросов/выручки, «комнаты» | — | local-only | `AdminOverviewPage.tsx:89-98,118-127`; `data/analyticsMock.ts:28-62` — детерминированные константы, тикают таймером `:73-77` | production (общий бандл) | Medium (в UI помечено «simulated», `:136-138,186-190`) |
| 33 | `/manage/users` | список/поиск/сортировка/фильтр | `GET /v1/admin/users` | missing | `AdminManagementView.tsx:243,252,269-285`; `adminMock.ts:76-89` ↔ в `crates/gateway-api/src/*` нет `/v1/admin/users` (полный инвентарь путей, §7.1) | production (общий бандл) | **High** |
| 34 | `/manage/users` | создание пользователя | `POST /v1/admin/users` | missing | `AdminManagementView.tsx:390-402` (`store.create`), `adminMock.ts:369-378` ↔ нет маршрута; `admin_user` создаётся только вручную (`0002_identity.sql:73`) | production (общий бандл) | **High** |
| 35 | `/manage/users` | правка (inline/в drawer) | `PATCH /v1/admin/users/:id` | missing | `AdminManagementView.tsx:367-373,385-388`; `adminMock.ts:380-392` ↔ нет маршрута | production (общий бандл) | **High** |
| 36 | `/manage/users` | удаление | `DELETE /v1/admin/users/:id` | missing | `AdminManagementView.tsx:335-339`; `adminMock.ts:394-402` ↔ нет маршрута | production (общий бандл) | **High** |
| 37 | `/manage/users` | bulk: approve/suspend/role/delete | `POST /v1/admin/users/bulk` | missing | `AdminManagementView.tsx:79-82,332-345`; `adminMock.ts:404-430` ↔ нет маршрута | production (общий бандл) | **High** |
| 38 | `/manage/users`, `/manage/activity` | экспорт CSV/JSON | `GET /v1/admin/export` | missing | `AdminManagementView.tsx:404-409`; `AdminActivityPage.tsx:102`; `adminMock.ts:452-504` (Blob в браузере) ↔ нет маршрута | production (общий бандл) | Medium |
| 39 | `/manage/djs` | список DJ | `GET /v1/admin/djs` | local-only | `AdminManagementView.tsx:244,252` ↔ живой маршрут есть (`panel_routes.rs:74`), но экран читает mock; живой список — `/djs` (`DjsPage.tsx`) | production (общий бандл) | High |
| 40 | `/manage/djs` | приглашение/статус/правка/удаление | `POST/PATCH/DELETE /v1/admin/djs*` | missing | `AdminManagementView.tsx:397` (create), `:375-383` (status), `:332-345` (bulk) ↔ админ-маршрутов DJ нет (`panel_routes.rs:31-87`) | production (общий бандл) | **High** |
| 41 | `/manage/clubs` | создание клуба | `POST /v1/admin/clubs` | local-only | `AdminManagementView.tsx:398` ↔ живой маршрут есть (`panel_routes.rs:64`), но CC пишет в mock | production (общий бандл) | High |
| 42 | `/manage/clubs` | правка/удаление/bulk | `PATCH/DELETE /v1/admin/clubs*`, bulk | missing | `AdminManagementView.tsx:371,378,335-339` ↔ только `PATCH status` (`panel_routes.rs:1567-1572`), `DELETE`/bulk отсутствуют | production (общий бандл) | **High** |
| 43 | `/manage/transactions`, `/requests` | список транзакций | `GET /v1/admin/payments` | local-only | `AdminManagementView.tsx:245,252,278-285`; `adminMock.ts:124-140,192-198` ↔ живой маршрут есть (`panel_routes.rs:67`), CC его не вызывает | production (общий бандл) | **High** |
| 44 | `/manage/transactions` | создание/правка/удаление, capture, refund | `POST/PATCH/DELETE`, `/refund`, `/capture` | missing | `AdminManagementView.tsx:399` (create), `:205-216` (patch capture/refund), `:335-339` (delete) ↔ финансовых админ-мутаций нет (§7.4) | production (общий бандл) | **Blocker**: «возврат» в UI не возвращает деньги |
| 45 | `/manage/activity`, `/moderation` | поток аудита (severity, actor, payload) | `GET /v1/admin/audit-log` | local-only | `AdminActivityPage.tsx:34-38,43-51`; `adminMock.ts:141-154,199-203` ↔ живой журнал есть (`panel_routes.rs:1748`), но без `diff` и из CC не читается | production (общий бандл) | **High** |
| 46 | `/manage/activity`, `/moderation` | «Rollback» события | `POST /v1/admin/audit-log/:id/rollback` | missing | `AdminActivityPage.tsx:65-75`; `adminMock.ts:433-450` ↔ нет маршрута; `audit_log` append-only (`0001_init.sql:39-47`) | production (общий бандл) | **Blocker**: кнопка «откатить» ничего не откатывает |
| 47 | `/manage/activity`, `/moderation` | «Ban target» | `PATCH /v1/admin/guests/:id {banned}` | missing | `AdminActivityPage.tsx:77-86` ↔ `banned` нигде не пишется (`0008_panel_tails.sql:39` — только колонка) | production (общий бандл) | High |
| 48 | `/manage/access`, `/roles` | матрица ролей × права | `GET/PUT /v1/admin/roles*` | local-only | `AdminAccessPage.tsx:76-78`; `RolePermissionMatrix.tsx:14,42,69` (`dropit.admin-console.role-permissions`) ↔ таблиц `role/permission/role_permission` нет (полный список таблиц БД — §7.6) | production (общий бандл) | **High** (ложное ощущение управления доступами) |
| 49 | `/manage/settings`, `/feature-flags` | комиссия/лимит/max price/auto-hold | `GET/PATCH /v1/admin/platform-settings` | local-only | `AdminSettingsPage.tsx:17,26-47,57-59,75-86` (localStorage `dropit.admin.settings`) ↔ живой API есть (`panel_routes.rs:70-73`), но `auto_hold_threshold` нет в схеме (`0001_init.sql:19-29`) | production (общий бандл) | **High**: два экрана правят «экономику» по-разному, один — в никуда |
| 50 | `/manage/settings`, `/feature-flags` | 4 фича-флага | `GET/PATCH /v1/admin/feature-flags` | missing | `AdminSettingsPage.tsx:10-15,67-73` ↔ нет таблицы и маршрута (в БД нет `feature_flag`) | production (общий бандл) | High |
| 51 | `/integrations` | настройки интеграций (Stripe и пр.) | `GET/PATCH /v1/admin/integrations` | missing | `App.tsx:130` → рендерит `PlatformSettingsPage` (экономика/режим стенда, `PlatformSettingsPage.tsx:18`) ↔ Stripe настраивается только env-переменными: `routes.rs:213-219` (`dropit_infra_stripe::from_env()`), таблицы настроек интеграций нет | production (общий бандл) | Medium |
| 52 | `/requests` | админский список заявок | `GET /v1/admin/requests` | missing | `App.tsx:126` → `AdminManagementView view="transactions"` ↔ `/v1/requests` — только гость (`requests_routes.rs:164`), модерация — только DJ (`requests_routes.rs:169-174`); админского листинга нет | production (общий бандл) | High |
| 53 | `/moderation` | модерация заявок/контента | `GET /v1/admin/requests`, `POST .../approve\|reject` | missing | `App.tsx:127` → `AdminActivityPage` (аудит) ↔ модерация только у DJ (`requests_routes.rs:169-174`), админских решений нет | production (общий бандл) | High |

### 3.3 Сквозные проверки

| # | Экран/маршрут | Действие или данные | Ожидаемый эндпоинт | Статус | Доказательство (frontend ↔ backend) | Влияние | Severity |
|---:|---|---|---|---|---|---|---|
| 54 | вся админка | гранулярность прав администратора | RBAC по `admin_user.role` | partial | UI различает 4 роли (`adminMock.ts:8-9`, `AdminManagementView.tsx:72`), БД — `superadmin/operator/readonly` (`0002_identity.sql:79`) ↔ JWT несёт только `Role::Admin` (`rbac.rs:20-28`), а `role` не выбирается в `identity.rs:118-126`; ни один хендлер его не проверяет | production | **Blocker** (readonly-админ имеет полный доступ) |
| 55 | все list-эндпоинты | мусор в query-параметрах | `Query<T>` | partial | `api/client.ts:354-367` ждёт JSON-конверт `{code,message}` ↔ ни один `Query` не имеет кастомного rejection (grep по `crates/gateway-api/src` — нет `QueryRejection`/`InvalidQuery`); axum 0.7 (`Cargo.toml:24`) отдаёт `400 text/plain`, клиент превращает это в `{code:'http_400'}` | production | Medium |

---

## 4. Расхождения полей и контрактов

### 4.1 Клуб: 5 полей из 13 приходят с сервера, 8 — константы

`GET /v1/admin/clubs` возвращает `id,name,city,email,status,turnover_minor`
(`panel_routes.rs:1559-1561`). Клиент заполняет остальное константами:

| Поле UI (`api/client.ts:123-137`) | Что делает клиент | Что на сервере |
|---|---|---|
| `slug`, `address`, `country`, `phone` | `''` (`api/client.ts:519-523`) | есть в таблице `club` (`0002_identity.sql:50-68`, `0008_panel_tails.sql:30-31`), но не в ответе |
| `logoUrl`, `stripeAccountId` | `null` (`:524-525`) | есть в `club` |
| `stripeOnboardingStatus` | **`'pending'` всегда** (`:526`) | не селектится (`panel_routes.rs:1543-1548`) |
| `createdAt` | `''` (`:528`) | не селектится |
| `turnover_minor` | не читается UI | возвращается |

Следствие: `/clubs` показывает «Stripe: pending» у всех, включая клубов с
`stripe_onboarding_status='verified'`.

### 4.2 DJ: Stripe-статус — константа

`GET /v1/admin/djs` → `id,display_name,email,club_id,status` (`panel_routes.rs:1978-1980`).
Клиент ставит `stripeOnboardingStatus:'pending'` и `createdAt:''` (`api/client.ts:583-585`),
`DjsPage.tsx:80` рисует бейдж. При этом колонка `dj.stripe_onboarding_status` в БД **есть**
(`0002_identity.sql:40`, рядом с `stripe_account_id` — `:39`), её просто не селектят
(`panel_routes.rs:1967`), а клиент подменяет константой (`api/client.ts:583`).

### 4.3 Платежи: UI читает 4 поля, которых нет в ответе

| Поле UI (`api/client.ts:188-198`) | Клиент | Бэкенд |
|---|---|---|
| `refundedMinor` | `0` (`:627`) | колонка `payment.refunded_minor` (`0005_payments.sql:35`) — не селектится (`panel_routes.rs:1682`) |
| `failureReason` | `null` (`:628`) | `payment.failure_reason` (`0005_payments.sql:36`) — не селектится |
| `capturedAt` | `null` (`:630`) | `payment.captured_at` (`0005_payments.sql:40`) — не селектится |
| `currency` | `'EUR'` (`:625`) | `payment.currency` (`0005_payments.sql:33`) — не селектится |

`PaymentsPage.tsx:104-117` эти поля рендерит → блок «refunded» и текст ошибки не появятся
никогда. `stripe_payment_intent_id` сервер отдаёт (`panel_routes.rs:1704`), UI его не читает.

### 4.4 Аудит: `diff` потерян дважды

`audit_log.diff JSONB` существует (`0001_init.sql:45`), но `GET /v1/admin/audit-log` его не
выбирает (`panel_routes.rs:1766`), а клиент принудительно ставит `diff: null`
(`api/client.ts:645`). В Control center payload строится из локального `diff`
(`AdminActivityPage.tsx:22-28,50`) — то есть у живого журнала нет «до/после», а у
фейкового есть. Это же ломает любой серверный rollback (§7.5).

### 4.5 Platform settings

| Поле | Клиент | Бэкенд |
|---|---|---|
| `updatedAt` | `String(r.updated_at ?? '')` (`api/client.ts:670`) → всегда `''` | `updated_at` есть в таблице (`0001_init.sql:28`), но не в SELECT (`panel_routes.rs:1810-1815`) |
| `commissionMode` | `r.commission_mode === 'fixed' ? 'fixed' : 'percent'` (`:662`) | `unset` — штатное состояние (`0001_init.sql:21,32`); клиент молча превращает его в `percent` |
| `clear_*` | не отправляются ни разу (`PlatformSettingsPage.tsx:43-54`) | 5 флагов сброса в NULL (`panel_routes.rs:1851-1860`) — недостижимы из UI |
| `autoHoldThreshold` (CC) | localStorage (`AdminSettingsPage.tsx:23,42`) | колонки нет вовсе (`0001_init.sql:19-29`) |

### 4.6 API-ключи

`api/developer.ts:23-40` объявляет `club_id: string` обязательным, но `ApiKeyView`
(`infra-db/src/api_keys.rs:144-161`) его не содержит: в админ-список добавляется только
`club_name` (`api_key_routes.rs:194-200`). UI использует `club_name || club_id`
(`ApiKeysPage.tsx:241`) — визуально не ломается, но тип врёт, и фильтра по `club_id`
на сервере нет (фильтры в UI локальные, `ApiKeysPage.tsx:91-98`).

### 4.7 Пагинация: сервер умеет, UI не просит

| Экран | Сервер | UI |
|---|---|---|
| `/guests` | `next_cursor` + `limit` (`panel_routes.rs:2134,2194`) | `next_cursor` не читается (`api/client.ts:590-603`), `limit` не шлётся → максимум 50 |
| `/clubs` | `LIMIT 100`, курсора нет (`panel_routes.rs:1548`) | >100 клубов не увидеть |
| `/djs` | `LIMIT 100` (`panel_routes.rs:1967`) | >100 DJ не увидеть |
| `/payments` | `limit` 1..500, курсора нет (`panel_routes.rs:1690`) | `limit` не шлётся → 100 |
| `/audit-log` | `LIMIT 100` (`panel_routes.rs:1767`) | параметров нет |
| `/mail` | `cursor` + `next_cursor` (`mail_routes.rs:293-305`) | ✅ «Load more» (`MailPage.tsx:473-491`) — единственный экран с рабочей пагинацией |

### 4.8 Что сходится точно (для контраста)

- Mail: список/деталь/статистика/подавления — все поля совпадают построчно:
  `mail_routes.rs:566-584` ↔ `api/client.ts:720-736`; `MailStats` `infra-db/src/mail.rs:220-240`
  ↔ `api/client.ts:745-756`; валидация статуса — единый список из 9 значений
  (`mail_routes.rs:250-260` ↔ `api/client.ts:705-715`).
- Payouts/incidents: `payouts_routes.rs:70-92` ↔ `api/client.ts:78-112`.
- Guests: `panel_routes.rs:2185-2195` ↔ `api/client.ts:594-602`.
- Auth: `TokenPair`/`TotpChallenge` `routes.rs:60-73` ↔ `api/client.ts:57-69`.

---

## 5. Query-параметры: принимает ли бэкенд и что при мусоре

Ни один `Query<T>` в `crates/gateway-api/src` не имеет кастомного rejection-хендлера
(в коде нет `QueryRejection`/`InvalidQuery`). При невалидном типе axum 0.7.9
(`Cargo.toml:24`) отдаёт **400 `text/plain`**, а не `{code,message,details}`;
`api/client.ts:354-367` синтезирует `{code:'http_400', message:'Bad Request'}`.
Для «строковых» фильтров (`status`) валидация ручная и даёт корректный конверт.

| Параметр | UI (строка) | Бэкенд | Валидация | Мусор |
|---|---|---|---|---|
| `GET /admin/payments?status=&club_id=&limit=` | `api/client.ts:612-619`; `PaymentsPage.tsx:32-34` (шлёт только `status`) | `PaymentsParams` `panel_routes.rs:1641-1649` | `status` по списку 8 значений → `400 invalid_status` (`:1671-1680`); `limit` clamp 1..500 (`:1690`); `club_id` — `Uuid` | `status=bogus` → 400 `invalid_status` (e2e `e2e_panels.rs:1434`); `club_id=abc`/`limit=abc` → 400 text/plain |
| `GET /admin/mail?status=&q=&limit=&cursor=` | `api/client.ts:795-803`; `MailPage.tsx:440-449` | `MailListQuery` `mail_routes.rs:231-237` | `status` по 9 значениям → `400 invalid_status` + `allowed` (`:278-287`); `limit` clamp 1..200 (`:289-292`); `q` обрезается | `status=garbage` → 400 (e2e `e2e_mail.rs:366`); `limit=abc` → 400 text/plain; **битый `cursor` молча игнорируется** (`infra-db/src/mail.rs:903` и далее) → выдаётся первая страница (риск дублей) |
| `GET /admin/guests?club_id=&limit=&cursor=` | `api/client.ts:590-593` (только `club_id`) | `GuestsParams` `panel_routes.rs:2093-2098` (`club_id` обязателен) | нет `club_id` → 400 (e2e `e2e_panels.rs:934`); неизвестный клуб → `404 club_not_found` (`:2125-2132`); `limit` clamp 1..100 (`:2134`); битый cursor → `(None,None)` (`parse_cursor`, `:2297-2312`) | `club_id=abc` → 400 text/plain; битый курсор → первая страница без ошибки |
| `GET /admin/api-keys?limit=` | `api/developer.ts:84-86` (всегда 200) | `AdminListQuery` `api_key_routes.rs:103-107`; кламп внутри репозитория `api_keys.rs:352` | clamp 1..200; `limit=0` → 1; `limit=-5` → 1 | `limit=abc` → 400 text/plain (конверта нет) |
| `GET /admin/mail/suppressions?limit=` | `api/client.ts:824-827` (50) | `MailLimitQuery` `mail_routes.rs:244-247` | clamp 1..500 (`:438`) | `limit=abc` → 400 text/plain |
| `GET /admin/mail/stats?window_hours=` | не передаётся (дефолт 24) | `mail_routes.rs:240-242` | clamp 1..720 (`:331`) | `window_hours=abc` → 400 text/plain |
| `GET /admin/reconciliation-runs?limit=&cursor=` | UI не вызывает | `panel_routes.rs:2201-2205` | clamp 1..50 (`:2228`) | битый cursor → первая страница |
| `GET /v1/status` (публичный) | `components/DeploymentStatus.tsx:108` | `status_routes.rs:723` | — | не влияет (есть fallback на `/readyz`) |

**Вывод по query-параметрам:** «строковые» фильтры валидируются и дают стабильные коды;
числовые/`Uuid`-параметры дают неконвертный 400; курсоры не валидируются (тихий сброс
пагинации). Для админ-клиента это значит: любая опечатка в `limit`/`club_id` выглядит как
«Bad Request» без кода, а испорченный курсор может показать дубли вместо ошибки.

---

## 6. Поставка: demo vs production

| Факт | Где |
|---|---|
| Режим задаётся сборочным `define`, не рантаймом | `apps/web-admin/vite.config.ts:18-20`; `src/app-mode.ts:4-6` |
| `npm run build` → production, `npm run build:demo` → demo | `apps/web-admin/package.json:8-9` |
| В **demo** смонтированы только `/`, `/manage/:section`, `/mail`, `/api-keys`; остальное редиректит на `/` | `src/App.tsx:79-97` |
| В **production** смонтированы 11 экранов с живым API + `/login` + Control center (`/`, `/manage/:section`, `/requests`, `/moderation`, `/roles`, `/feature-flags`, `/integrations`) | `src/App.tsx:100-131` (`124-130` — CC и алиасы) |
| Навигация одинакова в обеих сборках и ведёт в CC: `/`, `/manage/users`, `/manage/djs`, `/manage/clubs`, `/manage/transactions`, `/manage/activity`, `/manage/access`, `/manage/settings` | `src/components/Layout.tsx:16-52` |
| Живые экраны `/clubs`, `/djs`, `/payments` **отсутствуют в рельсе** (доступны только по прямому URL; `/clubs` — ещё из шортката dashboard) | `Layout.tsx:16-52` vs `App.tsx:107-110` |
| Магазин CC пишет в `localStorage` ключ `dropit.admin.mock.v1` | `src/data/adminMock.ts:220-234,250` |
| Честная плашка «local preview store» есть, но кнопки продолжают «работать» | `src/pages/AdminManagementView.tsx:442-445`; `AdminSettingsPage.tsx:109-112` |

**Вердикт:** Control center **виден в production-сборке** и является основным содержимым
разделов «Command» и «Platform» в рельсе. Demo-сборка, наоборот, прячет живые экраны
(кроме mail/api-keys) и оставляет CC как главный экран. То есть риск не «demo-only»:
оператор в проде нажимает «Delete», получает тост «Удалено 3» и идёт дальше.

Исключение по честности: mail и api-keys в demo-сборке остаются живыми и при отсутствии
бэкенда показывают ошибку, а не сид (`App.tsx:87-92`; `MailPage.tsx:546-550`).

---

## 7. Control center: что нужно на бэкенде

Ниже — по каждой операции точная форма. Все маршруты — под `AuthContext`
(`crates/gateway-api/src/rbac.rs:52-68`, 401 без Bearer) и проверкой
`ctx.role() == Role::Admin` (как в `panel_routes.rs:1500-1503`), плюс запись в `audit_log`
(таблица `0001_init.sql:39-47`). Нумерация миграций — со `0019` (последняя занята:
`0018_worker_heartbeat.sql`). Тесты — в `crates/gateway-api/src/e2e_panels.rs` по образцу
`e2e_admin_clubs_create` (`:729`) и `e2e_admin_guests` (`:832`).

### 7.1 Users (директория пользователей) — сейчас `missing`

Нужна проекция над четырьмя таблицами: `admin_user` (`0002_identity.sql:73-83`),
`club` (`:50-68`), `dj` (`:31-43`), `guest_account`/`guest_device`
(`0010_guest_account.sql:18-24`, `0002_identity.sql:17-24`).

```
GET /v1/admin/users?role=&status=&q=&limit=&cursor=
  → 200 {items:[{id, role, status, display_name, email, club_id,
                 created_at, last_seen_at}], next_cursor}
  role ∈ admin|club_staff|dj|guest ; status ∈ active|invited|suspended|blocked
  limit 1..200 (кламп), cursor — keyset (created_at,id) как в panel_routes.rs:2297-2328

POST /v1/admin/users
  body {role, email, display_name, club_id?}
  → 201 {id, role, invite_token?}          # при role=admin — ещё {totp_secret, otpauth_url}
  ⚠ admin_user.totp_secret_enc NOT NULL (0002_identity.sql:77) — создать админа без
    enrollment-ответа нельзя; иначе логин упадёт на routes.rs:374-378.

PATCH /v1/admin/users/:id
  body {display_name?, email?, role?, status?} → 200 {id, ...}
  запреты: смена своей роли; понижение последнего superadmin; 409 при активной сессии

DELETE /v1/admin/users/:id → 204   (soft: status='blocked' + revoke refresh_token,
                                    как в panel_routes.rs:837-843)

POST /v1/admin/users/bulk
  body {ids:[uuid], action: approve|suspend|block|delete|role, role?}
  → 200 {affected, ids}                # одна транзакция + одна запись аудита

GET /v1/admin/users/export?format=csv|json&<те же фильтры>
  → 200 (text/csv | application/json)  # потоково, не через браузерный Blob
```

Таблицы/миграции: новых не нужно, кроме индексов (`admin_user(role,status)`,
`guest_account(created_at)`). Тесты: `e2e_admin_users` — 401/403, список с пагинацией,
создание админа с `otpauth_url`, запрет self-delete, bulk, аудит на каждую мутацию.

### 7.2 DJ — админские мутации `missing`

```
PATCH /v1/admin/djs/:id   body {status?, club_id?, display_name?}
  → 200 {id, display_name, email, club_id, status, stripe_account_id}
  status ∈ invited|active|suspended|blocked ; при suspend/block — revoke refresh_token

DELETE /v1/admin/djs/:id → 200 {id, deleted:"soft"|"hard"}
  повторить логику клуба: hard — если password_hash='!' и нет смен (panel_routes.rs:787-818),
  soft — removed_at + status='suspended' (panel_routes.rs:821-862)

POST /v1/admin/djs   body {email, display_name, club_id}
  → 201 {dj_id, invite_token, invite_expires_at}   # переиспользовать club_djs_invite
                                                   # (panel_routes.rs:1074-1193) с club_id из тела

POST /v1/admin/djs/:id/restore → 200 {id, status:"active", removed:false}
```

Таблицы: `dj` (`0002_identity.sql:31-44`, `0008_panel_tails.sql:21-22`). Тесты:
`e2e_admin_djs` — сейчас `/v1/admin/djs` **не покрыт ни одним e2e** (§3, #8).

### 7.3 Clubs — расширить существующее

```
GET /v1/admin/clubs?limit=&cursor=
  → items дополнить: slug, address, country, phone, logo_url, stripe_account_id,
    stripe_onboarding_status, created_at  (всё есть в club, 0002_identity.sql:50-68 + 0008)

GET /v1/admin/clubs/:id  → 200 полная карточка (сейчас только PATCH → GET = 405)
PATCH /v1/admin/clubs/:id  body {name?, city?, address?, country?, phone?, email?, status?}
  → 200 ; аудит club.update (сейчас только club.status_change, panel_routes.rs:1606-1614)
DELETE /v1/admin/clubs/:id → 204 soft (status='blocked'); 409 при открытой сессии/незакрытых выплатах
POST /v1/admin/clubs/bulk  body {ids, action: activate|suspend|block|delete} → 200 {affected}
```

### 7.4 Payments — финансы (сейчас только чтение)

```
GET /v1/admin/payments?status=&club_id=&limit=&cursor=
  SELECT дополнить: p.currency, p.refunded_minor, p.failure_reason, p.captured_at,
  s.club_id (JOIN request→session), p.stripe_payment_intent_id
  → items[{id, request_id, club_id, amount_minor, currency, status,
           refunded_minor, failure_reason, created_at, captured_at}] + next_cursor

POST /v1/admin/payments/:id/refund
  body {amount_minor?}                     # без суммы — полный возврат
  → 200 {refund_id, refunded_minor, status}
  шаги: provider refund (Idempotency-Key "refund:{payment_id}:{amount}") →
        payment.refunded_minor += amount, status partially_refunded|refunded →
        split.reversed_minor (пропорционально) → ledger_entry (двойная запись) →
        audit_log action='payment.refund', diff={amount, before_status, after_status}

POST /v1/admin/payments/:id/capture   → 200 {status:"captured"}   # админ-оверрайд
POST /v1/admin/payments/:id/void      → 200 {status:"cancelled"}  # DELETE запрещён: финансовые строки
```

Таблицы: `payment`, `split`, `ledger_entry`, `webhook_event`
(`0005_payments.sql:27-70,88`). ⚠ Инвариант Σ`split` == `payment.amount_minor`
(`0005_payments.sql:49`) должен сохраняться при возврате. Тесты: e2e с MockStripe —
полный возврат, частичный, повторный вызов (идемпотентность), 409 на чужой статус.

### 7.5 Audit, rollback, ban, moderation

```
GET /v1/admin/audit-log?limit=&cursor=&actor_type=&action=&entity=&from=&to=
  → items дополнить полем diff (колонка есть — 0001_init.sql:45)
  (сейчас panel_routes.rs:1766 diff не селектит, LIMIT 100 без параметров)

POST /v1/admin/audit-log/:id/rollback
  → 201 {rollback_audit_id, applied:true|false}
  ⚠ требует, чтобы КАЖДЫЙ пишущий хендлер клал в diff before-image; существующие
    записи (club.status_change и др.) его не кладут — старые события откатить нельзя.
    Безопаснее: сузить до конкретных сущностей и всегда писать новую строку аудита.

PATCH /v1/admin/guests/:id   body {banned: bool} → 200 {id, banned}
  + гейт в requests_routes: забаненное устройство не создаёт заявку (403 guest_banned)

GET  /v1/admin/requests?status=&club_id=&session_id=&cursor=
  → items[{id, session_id, club_id, dj_id, guest_device_id, track_title, amount_minor,
           status, created_at}]
POST /v1/admin/requests/:id/approve|reject   body {reason?} → 200 {id, status}
```

Таблицы: `guest_device.banned` (`0008_panel_tails.sql:39`), `request` (`0004_requests.sql:40`),
`session` (`0003_sessions.sql:36`). Тесты: `e2e_admin_requests_moderation`,
`e2e_admin_guest_ban` (включая, что забаненный гость получает 403).

### 7.6 Roles / permissions — `local-only`

Нужны три таблицы (миграция `0019_roles.sql`):

```sql
CREATE TABLE role        (code TEXT PRIMARY KEY, title TEXT NOT NULL, description TEXT);
CREATE TABLE permission  (code TEXT PRIMARY KEY, title TEXT NOT NULL, group_name TEXT NOT NULL,
                          description TEXT);
CREATE TABLE role_permission (role_code TEXT REFERENCES role(code),
                              permission_code TEXT REFERENCES permission(code),
                              PRIMARY KEY (role_code, permission_code));
```

Сиды: `role` = `superadmin, operator, readonly` (значения уже заявлены в
`0002_identity.sql:79`) + `platform_admin, club_manager, dj, support` — как в UI
(`RolePermissionMatrix.tsx:6-13`); `permission` = `users.manage, venues.manage,
refunds.issue, queue.edit, audit.view` (`:7`).

```
GET /v1/admin/roles → {items:[{code,title,permissions:[code]}],
                       permissions:[{code,title,group,description}]}
PUT /v1/admin/roles/:code/permissions  body {permissions:[code]} → 200
  запрет: менять `superadmin`; аудит action='role.permissions_update'
```

**Обязательное условие:** gateway должен читать `admin_user.role` (сейчас запрос
`identity.rs:118-126` её не выбирает) и проверять permission на каждый маршрут
(middleware `require(Permission::UsersManage)`), иначе таблица останется декорацией.

### 7.7 Feature flags — `missing`

```sql
CREATE TABLE feature_flag (
  key TEXT PRIMARY KEY, enabled BOOLEAN NOT NULL DEFAULT false,
  description TEXT, updated_at TIMESTAMPTZ NOT NULL DEFAULT now(), updated_by UUID);
```

```
GET   /v1/admin/feature-flags → {items:[{key,enabled,description,updated_at}]}
PATCH /v1/admin/feature-flags/:key  body {enabled} → 200 {key,enabled,updated_at}
```

Флаги из UI (`AdminSettingsPage.tsx:10-15`): `guest_requests`, `dj_invites`,
`instant_payouts`, `auto_moderation`. ⚠ Последние два сейчас не имеют продукта:
выплаты — недельный cron (`payouts_routes.rs:4-6`), модерации нет вообще. Флаги нужно либо
подключить к реальным гейтам, либо убрать из UI — иначе это переключатели без эффекта.

### 7.8 Settings/economics в CC — переиспользовать `platform-settings`

`GET/PATCH /v1/admin/platform-settings` уже есть (`panel_routes.rs:70-73`), нужно:

1. добавить `updated_at` в SELECT (`:1810-1815`);
2. добавить колонку под `auto_hold_threshold` (`0019`), либо убрать поле из
   `AdminSettingsPage.tsx:23`;
3. не терять `unset`: `commission_mode='unset'` должен доезжать до UI как отдельное
   состояние (сейчас `api/client.ts:662` превращает его в `percent`);
4. перевести CC с `localStorage` на эти два вызова (проценты → bp: 12 % = 1200).

### 7.9 Integrations — `missing`

```
GET /v1/admin/integrations
  → {items:[{provider:"stripe"|"resend", mode:"live"|"mock",
             configured:bool, account_id?, webhook_url?, updated_at}]}   # секреты замаскированы
PATCH /v1/admin/integrations/:provider  body {mode?, config?} → 200
```

Сейчас провайдеры выбираются только окружением: `routes.rs:213-219`
(`dropit_infra_stripe::from_env()`), Resend — `mail_routes.rs:89-100`
(`webhook_verifier_from_env()`). Нужна таблица `integration_setting(provider PK, mode,
config JSONB, updated_at)` + аудит; ключи — в secret-store, не в БД.

### 7.10 Export — `missing`

```
GET /v1/admin/export?entity=users|clubs|djs|payments|audit&format=csv|json&<фильтры>
  → 200 потоковый файл; RBAC Admin; аудит action='export.created'
```

Причина: `adminMock.ts:488-504` собирает CSV в браузере, а список с сервера ограничен
100-200 строками (§4.7) — «экспорт всех» без серверного стрима невозможен.

### 7.11 Overview — заменить симуляцию

```
GET /v1/admin/dashboard/series?period=24h|7d|30d&metric=requests|revenue|sessions
  → {points:[{label, value}], step}
```

Источник — агрегаты по `request`/`payment`/`session` (`0004_requests.sql:40`,
`0005_payments.sql:27`, `0003_sessions.sql:36`); при росте объёма — materialized view/rollup.
Пока эндпоинта нет, `AdminOverviewPage.tsx:89-98,118-127` показывает константы
(`analyticsMock.ts:28-62`) — их нужно либо убрать, либо оставить с явным «simulated» (сейчас
пометка есть, `:136-138,186-190`).

---

## 8. Что ещё замечено (не входит в топ-10)

1. **Бэкенд-поверхности без UI:** `POST /v1/admin/mail/test` (`mail_routes.rs:66,491-559`),
   `GET /v1/admin/api-keys/scopes` (`api_key_routes.rs:63`; клиент `api/developer.ts:97-99`
   объявляет, но не вызывает), `GET /v1/admin/payments/:id/splits`
   (`panel_routes.rs:68`), `GET /v1/admin/reconciliation-runs` (`:76-78`),
   `GET /v1/admin/status/services` (`status_routes.rs:725`). Не дефект, но
   «функционал панели» они не покрывают — в отчёте о покрытии их учитывать нельзя.
2. **RBAC-паттерн:** гейт `role_route` (`routes.rs:190-197`) применён только к
   `/v1/admin/echo` + mail; остальные админ-роуты полагаются на ручную проверку внутри
   хендлера. Работает корректно (проверено во всех хендлерах `panel_routes.rs`,
   `payouts_routes.rs`, `api_key_routes.rs`), но новый хендлер без проверки станет дырой
   без какого-либо предупреждения компилятора.
3. **Мелочи типов:** `patchClub` синтезирует `Club` с пустым `name`
   (`api/client.ts:557-571`) — тип врёт, но `ClubsPage` ответ игнорирует и делает `reload()`.
4. **`/dashboard` считает оборот по `payment.created_at`** (`panel_routes.rs:1512`), а
   `AdminOverviewPage` суммирует mock-массив вообще без фильтра по времени
   (`AdminOverviewPage.tsx:79-81`); при подключении живых данных окна разойдутся.
5. **`turnover_minor`** в `/admin/clubs` считается без фильтра по времени
   (`panel_routes.rs:1545-1547`) — это «за всё время», а не «за неделю»; UI его не читает,
   но при подключении легко принять за недельный.
6. **Два экрана правят одну и ту же «экономику»:** `PlatformSettingsPage` (живой API) и
   `AdminSettingsPage` (localStorage). После подключения CC (§7.8) один из них надо удалить.
7. **`guest_device.banned`** получает значение только из `DEFAULT` миграции — ни один
   хендлер его не меняет; при этом `AdminActivityPage` показывает «Ban target».
8. **UI-e2e нет.** В `apps/web-admin` нет тестов (поиск `*.test.*`/`*.spec.*` пуст);
   `scripts/panel-audit.mjs:348-360` — визуальный аудит и для mail подменяет сеть
   фикстурами. То есть ни один админ-сценарий не проверяется автоматически со стороны UI.

---

## 9. Топ-10 по приоритету

| # | Проблема | Почему сначала | Куда смотреть |
|---:|---|---|---|
| 1 | **Control center в production-сборке пишет в localStorage** (`App.tsx:124-130`, `Layout.tsx:16-52`, `adminMock.ts:220-234`) | оператор в проде «удаляет» и «откатывает» без сервера; журнал аудита не пополняется; вводит в заблуждение при инциденте | скрыть CC за флагом сборки до появления API либо перевести на живые вызовы (§7.1-7.5) |
| 2 | **Возврат/«capture» в CC — фикция** (`AdminManagementView.tsx:205-216,335-339`) | финансовое действие, которое выглядит выполненным | §7.4: `POST /v1/admin/payments/:id/refund` с провайдером, split-reversal, ledger и аудитом |
| 3 | **RBAC админки = один `Role::Admin`; `admin_user.role` не читается** (`identity.rs:118-126`, `0002_identity.sql:79`) | «readonly»-админ имеет полный доступ, включая деньги | §7.6: читать роль + middleware permission |
| 4 | **`/clubs` и `/djs`: Stripe-статус — константа `pending`** (`api/client.ts:526,583`) | ложное состояние онбординга на двух живых экранах | отдавать `stripe_onboarding_status`/`stripe_account_id` (обе таблицы уже имеют колонки) |
| 5 | **`/payments`: нет `refunded_minor`/`failure_reason`/`captured_at`/`currency`** (`api/client.ts:625-630` ↔ `panel_routes.rs:1682`) | UI рендерит блоки, которых не будет никогда; возвраты невидимы | расширить SELECT (колонки есть — `0005_payments.sql:33,35,36,40`) |
| 6 | **Матрица ролей и фича-флаги — localStorage** (`RolePermissionMatrix.tsx:14`, `AdminSettingsPage.tsx:17`) | два «пульта управления» без эффекта | §7.6, §7.7 (таблицы `0019`) |
| 7 | **Rollback и Ban в аудите — фикция** (`AdminActivityPage.tsx:65-86`) | «откат» инцидента не откатывает; бан не банит | §7.5: before-image в `diff`, `PATCH /v1/admin/guests/:id {banned}`, rollback как новая запись |
| 8 | **`GET /v1/admin/audit-log` без `diff` и пагинации** (`panel_routes.rs:1766-1767`) | без «до/после» журнал бесполезен для разбора, 100 строк — потолок | §7.5 |
| 9 | **`platform-settings`: `unset` → `percent`, нет `updated_at`, нет `clear_*`** (`api/client.ts:662,670`; `PlatformSettingsPage.tsx:43-54`) | тихое изменение денежных правил на пустом стенде | §7.8 |
| 10 | **Экраны `/requests`, `/moderation`, `/integrations` ведут не туда** (`App.tsx:126-130`) | обещанные разделы не имеют ни API, ни содержимого; `/integrations` показывает экономику | §7.4/7.5/7.9; до реализации — убрать из рельса |

---

### Приложение: что прочитано

Фронтенд: `apps/web-admin/src/App.tsx`, `app-mode.ts`, `main.tsx`, `menu.tsx`, `modal.tsx`,
`nav-drawer.tsx`, `api/client.ts`, `api/developer.ts`, `hooks/useAdminData.ts`,
`hooks/useAsyncData.ts`, `data/adminMock.ts`, `data/analyticsMock.ts`,
`pages/*.tsx` (17 файлов), `components/Layout.tsx`, `components/DeploymentStatus.tsx`,
`components/admin-console/{AuditLogStream,EntityDetailDrawer,QuickActionConsole,RolePermissionMatrix}.tsx`,
`components/admin-table/PowerTable.tsx`, `components/ui/{Select,StatCard,TrashAction}.tsx`.

Бэкенд: `crates/gateway-api/src/{routes,panel_routes,mail_routes,payouts_routes,api_key_routes,status_routes,payments_routes,requests_routes,signup_routes,rbac}.rs`,
`crates/infra-db/src/{identity,api_keys,mail}.rs`, `crates/infra-db/migrations/*.sql` (18 файлов),
`crates/common/src/error.rs`, `docs/openapi.yaml`, `apps/web-admin/vite.config.ts`,
`apps/web-admin/package.json`, `scripts/panel-audit.mjs`.

Схема БД: `psql` — 38 доменных таблиц (+ `_sqlx_migrations`); подтверждено отсутствие таблиц `role/permission/feature_flag/
integration_setting` и наличие колонок `payment.refunded_minor/failure_reason/captured_at/
currency`, `audit_log.diff`, `guest_device.banned`, `admin_user.role`,
`platform_settings.updated_at/runtime_mode`.
