# Аудит: `apps/web-status` ↔ бэкенд, статус `apps/web-dj`

Дата: 2026-10-29. Метод: чтение файлов + живая проверка локального бинаря
`target/debug/dropit-api` против Postgres на :55433 (порты 18099 — с БД, 18098 —
с заведомо недоступной БД; оба процесса остановлены после проверки).
Код не менялся.

## Резюме

Публичная страница `/status/` **полностью обеспечена бэкендом**: `GET /v1/status/services`
существует, отдаёт ровно те четыре поля на сервис (`id`, `name`, `state`, `note`),
которые читает `status.js`, и всегда отвечает `200` (проверено вживую, в том числе
при мёртвой БД). Расхождений по именам полей и по id сервисов между страницей и
сервером **нет**: страница вообще не хардкодит id и порядок — она рендерит то, что
пришло (`apps/web-status/status.js:148`, `crates/gateway-api/src/status_routes.rs:627`).
Реальные пробелы лежат не в контракте, а рядом: у проверки Redis нет таймаута
(`status_routes.rs:310`), строка `payments` жёстко зашита в `ok` и не может
деградировать (`status_routes.rs:645`), страница пишет `data-busy`/`data-stale`, а
CSS ждёт `aria-busy` и вообще не знает про `stale` (`status.js:228` ↔ `status.css:252`),
а деплой и `verify-stand.mjs` не проверяют ни `/v1/status/services`, ни `/status/`.
`apps/web-dj` — статический мок без единого сетевого вызова, публикуется как
`/dj-mock/`; его встроенный комментарий «контракта» описывает эндпоинты, которых в
gateway нет.

## 1. Матрица «сценарий/поле → эндпоинт → статус → доказательство»

| # | Сценарий / поле | Эндпоинт | Статус | Доказательство |
|---|---|---|---|---|
| 1 | Загрузка страницы, первичный и периодический опрос | `GET /v1/status/services` | `200 application/json`, `cache-control: no-store` | `apps/web-status/status.js:27`, `:241`, `:300`; `crates/gateway-api/src/status_routes.rs:667`, `:724`; `:655` (no-store) |
| 2 | Верхнеуровневый `status` | то же | `ok` \| `degraded` \| `down` | `status.js:168`; `status_routes.rs:671`; `docs/openapi.yaml:3461` |
| 3 | `checked_at` | то же | RFC3339 с офсетом `+00:00` | `status_routes.rs:683`; `status.js:171`–`:178`; живой ответ: `"checked_at":"2026-10-28T23:49:28.574476+00:00"` |
| 4 | `services[]`: 6 записей, порядок серверный | то же | `panel, dj, api, requests, guest-entry, payments` | `status_routes.rs:627`–`:652`; `e2e_status_services.rs:86`; `status.js:159` (рендер в порядке ответа) |
| 5 | `services[].id` | то же | 6 стабильных id | `status_routes.rs:591`; живой ответ (см. §6) |
| 6 | `services[].name` | то же | «Club panel», «DJ console», «API», «Music requests», «Guest entry (QR)», «Payments» | `status_routes.rs:628`–`:641`; `status.js:124` (пусто → fallback на id) |
| 7 | `services[].state` | то же | `ok` \| `degraded` \| `down` (публично `unknown` не достигается) | `status_routes.rs:604`–`:651`; `status.js:75`, `:126`; `openapi.yaml:3473` |
| 8 | `services[].note` | то же | непустая человеческая строка | `status_routes.rs:581`–`:588`, `:635`; `status.js:125`, `:139` |
| 9 | БД недоступна | `/v1/status/services` | **`200`**, `status:"down"`; panel/dj/requests/guest-entry = `down`, **api = `degraded`**, payments = `ok` | живая проверка на :18098; `status_routes.rs:227`, `:247`, `:611`, `:618`, `:619`; `e2e_status_services.rs:31`–`:37` |
| 10 | Воркеры отстают (heartbeat 712 c в локальной БД) | `/v1/status/services` | **`200`**, `status:"degraded"`, `requests:"degraded"` | живой ответ; `status_routes.rs:361`; `e2e_status_services.rs:161`–`:162` |
| 11 | Outbox пуст | `/v1/status/services` | `200`, `ok` | `status_routes.rs:413`; `e2e_status_services.rs:182` |
| 12 | Redis недоступен | `/v1/status/services` | `200`, `panel`/`dj` = `degraded` | `status_routes.rs:322`, `:606` |
| 13 | Раздача страницы | `GET /status/` | `200 text/html` из `/srv/dropit/web/status` | `deploy/prod/nginx/dropit.conf:141`–`:145`; сборка: `scripts/build-linux.sh:78`; деплой: `deploy/prod/deploy.sh:217` |
| 14 | Проксирование API | `/v1/` → `127.0.0.1:8080` | passthrough, без `proxy_intercept_errors` | `dropit.conf:76`–`:89` |
| 15 | UI-таймаут запроса | — | `10 000` мс, `AbortController` | `status.js:29`, `:237`, `:238`, `:184` |
| 16 | Пустой `services` | — | ветка «endpoint returned no services» (сервером недостижима) | `status.js:155`; сервер всегда отдаёт 6 (`status_routes.rs:627`) |
| 17 | Режим стенда (не для страницы статуса) | `GET /v1/status` | `200`, `{mode, source, runtime_mode, open_signup, payments}` | `status_routes.rs:146`; `openapi.yaml:81`; потребитель: `packages/ui/DeploymentStatus.tsx:109` |
| 18 | Технический статус для оператора | `GET /v1/admin/status/services` | `401` без токена, `403` клубу, `200` админу | `status_routes.rs:695`; `e2e_status_services.rs:102`–`:137` |

## 2. Расхождения контракта `/v1/status/services` (по полям)

### 2.1 Пофайловое сопоставление

| Поле | Читает UI | Отдаёт сервер | Вердикт |
|---|---|---|---|
| `status` | `status.js:168` (валидирует по `STATE_WORDS`, неизвестное → `unknown`) | `status_routes.rs:671`; enum `openapi.yaml:3461` — без `unknown` | Совпадает. UI — надмножество (защита от мусора), `unknown` сверху сервер не отдаёт |
| `checked_at` | `status.js:171`–`:178` (`Date` + `Intl` UTC, при мусоре — «did not report a check time») | `status_routes.rs:683`, `Utc::now().to_rfc3339()` | Совпадает. Нюанс: микросекунды и офсет `+00:00` вместо `Z` — `Date` парсит, в `title` показывается сырая строка (`status.js:177`) |
| `services` | `status.js:180`, `:256` (требует именно массив) | `status_routes.rs:684` | Совпадает |
| `services[].id` | `status.js:123`, `:148` (пустой id → запись пропускается) | `status_routes.rs:591`; 6 значений | Совпадает. **UI не знает ни одного id** — рассинхрон id «сервер↔страница» невозможен; id зафиксированы только тестом `e2e_status_services.rs:86` |
| `services[].name` | `status.js:124` (пусто → fallback на id) | `status_routes.rs:591`, `:628`–`:641` | Совпадает |
| `services[].state` | `status.js:126`, `:75` (4 значения) | `status_routes.rs:604`–`:651` (фактически 3) | Совпадает |
| `services[].note` | `status.js:125`, `:139` (обязателен, пустая строка допустима) | `status_routes.rs:581`–`:588`, `:635` | Совпадает; сервер никогда не отдаёт пустой note |
| `order` | `status.js:159` — порядок ответа как есть | `status_routes.rs:627`–`:652` — фиксированный `vec![]` | Совпадает де-факто, **не зафиксирован в `openapi.yaml`** (`3439`–`3474` не описывают ни порядок, ни количество) |
| `label`/`detail`/`latency_ms` | не читает | только админский эндпоинт (`status_routes.rs:197`–`:205`) | Корректно: публичный `public_service` (`status_routes.rs:590`) отдаёт ровно 4 поля |

**Вывод по полям: расхождений нет.** Ни одного поля, которое UI читает, а сервер не
отдаёт; ни одного поля, которое UI обязан был бы знать из публичного ответа и не знает.
Порядок и количество сервисов — единственная недокументированная часть контракта.

### 2.2 Честность деградации (какие сигналы реально есть)

| Проверка | Как деградирует | Что видит человек на странице |
|---|---|---|
| PostgreSQL (`status_routes.rs:226`) | нет коннекта → `down` (`:247`) | panel/dj/requests/guest-entry → «Not available right now» |
| Схема (`:257`) | старая схема → `degraded` (`:284`) | **никак** (в публичный список не входит) |
| Redis (`:298`) | ping-ошибка → `down` (`:322`) | panel/dj → `degraded` (`:606`) |
| Воркеры (`:335`) | heartbeat > 45 c → `degraded` (`:361`), > 30 мин → `down` (`:368`), нет строки → `down` (`:375`) | requests → `degraded`/`down` (`:613`) |
| Outbox (`:394`) | при настроенном `NATS_URL` и старых записях → `degraded` (`:440`) / `down` (`:447`); без relay — `ok` с пояснением (`:423`) | requests → `degraded` при включённом relay, иначе `ok` |
| Mail (`:461`) | `failed > 0` или `pending > 50` → `degraded` (`:474`) | **никак** (в публичный список не входит) |
| Платежи (`:510`) | **никак, всегда `ok`** | «Payments — Test mode…» даже при мёртвой БД |

Следствие: три внутренние проверки (schema, mail, redis-detail) честно деградируют, но
в публичный ответ не попадают by design (`status_routes.rs:594`–`:596`, `openapi.yaml:3446`).
Зато `payments` — единственная строка, у которой нет ни одного наблюдаемого сигнала.

## 3. Обработка ошибок и таймаутов: что ожидает UI ↔ что делает сервер

| # | Сценарий | Ожидание UI (строка) | Реальность сервера (строка) | Вердикт |
|---|---|---|---|---|
| 1 | Сеть недоступна / DNS | `TypeError` → «The request did not reach the server» (`status.js:190`) | запрос не дошёл | Совпадает |
| 2 | Таймаут клиента | 10 000 мс → `AbortError` → «did not answer within 10 seconds» (`status.js:29`, `:184`) | — | Таймаут есть, `AbortController` есть. Но бюджет сервера может быть больше: `collect_checks` последовательный (`status_routes.rs:536`–`:546`), 6 проверок с таймаутом 1.5 c (`:35`) ≈ **9 c** + ping Redis вообще без таймаута (`:310`) — на зависшей сети UI сдаётся раньше ответа |
| 3 | HTTP 503 с телом-документом | «это настоящий ответ, а не сетевая ошибка» (`status.js:12`–`:13`); тело читается всегда (`:249`) | **невозможно**: роут всегда возвращает `200` (`status_routes.rs:680`) | Расхождение формулировки: 503 неоткуда взять |
| 4 | HTTP 4xx/5xx без документа | «The endpoint answered HTTP N without a status document» (`status.js:187`, `:262`) | nginx при мёртвом upstream: `502`/`504` + HTML, без `error_page` (`dropit.conf:76`–`:89`, в файле нет `error_page`) | Совпадает; на практике это `502`, а не `503` |
| 5 | Тело не JSON | `JSON.parse` в `try` → `payload=null` → failure-ветка (`status.js:251`–`:265`) | — | Совпадает |
| 6 | JSON без `services`/не объект | «unexpected document» (`status.js:264`) | — | Совпадает |
| 7 | Пустой `services: []` | «The endpoint returned no services» + `aria-busy=false` (`status.js:155`, `:163`) | всегда 6 сервисов (`status_routes.rs:627`) | Ветка недостижима (безвредно) |
| 8 | Неизвестный `state` у сервиса | нормализуется в `unknown` (`status.js:126`), точка серая (`status.css:452`) | публично не отдаётся | Совпадает (защита) |
| 9 | Неизвестный `status` сверху | `unknown` (`status.js:103`) → «Status unknown» (`:42`) | не отдаётся | Совпадает (защита) |
| 10 | Отказ после успешного снимка | «Could not refresh… Showing the last successful check at HH:MM UTC», пилюля `unknown` + `data-stale` (`status.js:204`–`:221`, `:106`) | — | Логика верная; но `[data-stale]` **не имеет CSS** — `grep stale apps/web-status/status.css` даёт 0 совпадений, снаружи это только суффикс «· stale» (`status.js:104`) |
| 11 | Отказ на самой первой загрузке | «Status unavailable right now. Try again in a moment.» + баннер с Retry (`status.js:208`–`:215`) | — | Совпадает |
| 12 | Кнопка Refresh в полёте | `disabled` + «Refreshing…» (`status.js:226`–`:230`) | — | `dataset.busy` (`status.js:228`) не читается CSS: там `.refresh[aria-busy='true'] .refresh-icon` (`status.css:252`, `:261`) → спиннер не крутится никогда |
| 13 | Скрытая вкладка | опрос останавливается, возврат — немедленный запрос (`status.js:278`–`:295`) | — | Совпадает |
| 14 | Гонка запросов | `state.inFlight` (`status.js:233`) | — | Совпадает |
| 15 | Кэш | `cache: 'no-store'` (`status.js:242`) + `no-store` от сервера (`status_routes.rs:655`) | — | Совпадает |

### 3.1 `/v1/status` (общий) — используется ли и нужен ли

Используется панелями, но **не** страницей статуса: `packages/ui/DeploymentStatus.tsx:109`
(и её копии `apps/web-club/src/components/DeploymentStatus.tsx:109`,
`apps/web-admin/src/components/DeploymentStatus.tsx:109`) читает `mode`, `source`,
`open_signup`, `payments`; при отсутствии роута — фолбэк на `/readyz`
(`DeploymentStatus.tsx:122`). Живой ответ: `{"mode":"test","source":"adaptive",
"runtime_mode":"auto","open_signup":false,"payments":"mock"}`. Смыслу страницы статуса
он не нужен: это «режим стенда», а не здоровье сервисов; смешивать не следует.
Единственная мелкая нестыковка: `runtime_mode` задокументирован (`openapi.yaml:3556`),
но панелями не читается.

Отдельно: `/readyz` не зависит от БД by design (`crates/gateway-api/src/routes.rs:237`) и
при мёртвой БД отдаёт `200 {"status":"ok"}` (проверено вживую). Это осознанное решение,
но именно на него опирается health-check деплоя (`deploy/prod/deploy.sh:85`), поэтому
деплой не замечает состояния, которое страница статуса показывает как «outage».

### 3.2 Обращения UI к несуществующим эндпоинтам

- `apps/web-status`: один запрос — `/v1/status/services` (`status.js:27`); роут есть
  (`status_routes.rs:724`). **Мёртвых обращений нет.**
- `apps/web-dj`: сетевых вызовов нет вообще (см. §4).
- Панели: `/v1/status` (`status_routes.rs:723`) и `/readyz` (`routes.rs:91`) — оба есть.
- Единственное место в репозитории, где названы несуществующие роуты, — комментарий
  внутри мока `apps/web-dj/index.html:373`–`:375` (см. §4.3).

## 4. `apps/web-dj`

### 4.1 Подтверждение мока

- Каталог содержит только `index.html` (`ls apps/web-dj`), без `package.json`, зависимостей
  и сборки; в деплое он копируется как есть (`scripts/build-linux.sh:76`).
- Один инлайн-скрипт `apps/web-dj/index.html:370`–`:550`; `grep -nE "fetch\(|XMLHttpRequest|
  WebSocket|EventSource|axios|localStorage"` не даёт **ни одного** совпадения: данных-демо
  в памяти (`index.html:379`–`:384`), «переподключение» — таймер на 2.6 c
  (`index.html:537`–`:538`, оверлей `:362`).
- Публикуется как `/dj-mock/`: `dropit.conf:180` (`location /dj-mock/`), явный комментарий
  «static design prototype, NOT the product» (`dropit.conf:11`, `:177`), деплой кладёт
  `build/web/dj` → `/srv/dropit/web/dj-mock` и удаляет старый `/srv/dropit/web/dj`
  (`deploy.sh:218`, `:221`); `/dj` намеренно не отдельная локация (`dropit.conf:14`–`:18`).
  Проверяется живой проверкой `/dj-mock/` в `scripts/verify-stand.mjs:149`.
- Внешняя зависимость: Google Fonts (`index.html:8`–`:10`) — мок не офлайновый, в отличие
  от статус-страницы.

### 4.2 Какой API понадобился бы для живого режима (справочно, не реализовывать)

Уже существует в gateway:

| Нужно моку | Реальный роут | Где |
|---|---|---|
| Вход DJ | `POST /v1/auth/dj/login` | `crates/gateway-api/src/routes.rs:93` |
| Текущая сессия DJ | `GET /v1/sessions/current` | `crates/gateway-api/src/sessions_routes.rs:148` |
| Очередь (`sort=new|hot`) | `GET /v1/dj/queue` → `{items:[QueueItemDto]}` | `requests_routes.rs:169`, `:67`–`:87` |
| Принять / отклонить / played | `POST /v1/dj/requests/:id/{accept,reject,played}` | `requests_routes.rs:170`–`:172` |
| Пауза / возобновление приёма | `POST /v1/dj/acceptance/pause`, `/resume` | `requests_routes.rs:173`–`:174` |
| Показатели смены | `GET /v1/dj/stats/today` → `{requests_today, accepted, played, rejected, earned_total_minor}` | `panel_routes.rs:81`, `:2395`–`:2401` |
| Смена / присутствие | `GET /v1/dj/shift`, `POST /v1/dj/presence` | `panel_routes.rs:80`, `:52` |
| Живые обновления | `GET /v1/ws` (тикет `POST /v1/ws/ticket`) | `crates/gateway-api/src/ws.rs`, `routes.rs:142` |

Чего не хватает для 1:1 с моком: в `QueueItemDto` **нет суммы заявки** (есть `title`,
`artist`, `style`, `same_track_count`, `created_at`, `status`, `isrc`,
`cover_url`, `preview_url` — `requests_routes.rs:72`–`:87`), а мок показывает цену по
каждой заявке (`index.html:384`); `earned_total_minor` — за всё время, не «сегодня»
(`panel_routes.rs:2385`). Плюс мок не аутентифицирован, а все DJ-роуты требуют JWT
роли DJ (`requests_routes.rs:780`).

### 4.3 Расхождение внутри мока

Комментарий `index.html:373`–`:375` описывает другой контракт: `GET /sessions/{id}/requests`
(плоский массив), `POST /requests/{id}/accept|reject|played`, `PATCH /sessions/{id}/pause`.
Фактически: `/v1/dj/queue` (обёртка `{items}`), `/v1/dj/requests/:id/...`,
`POST /v1/dj/acceptance/pause` + отдельный `/resume`. Это не баг рантайма (мок ничего не
вызывает), но как «спека» комментарий вводит в заблуждение.

## 5. Топ-5 пробелов и точная форма исправлений

### 5.1 Бэкенд: у проверки Redis нет таймаута — единственная проверка, способная подвесить страницу

`crates/gateway-api/src/status_routes.rs:310` вызывает `store.ping().await` без
`tokio::time::timeout`, хотя у всех остальных проверок таймаут `DB_READ_TIMEOUT` есть
(`timed`, `:208`–`:224`; прямой `timeout` в `mail_check`, `:471`), а комментарий файла
обещает «зависший запрос не должен вешать страницу» (`:208`). При зависшем Redis
соединение висит до таймаута клиента, и страница остаётся пустой (UI спасает только
`AbortController`, `status.js:237`).

Исправление (бэкенд, `status_routes.rs:310`):

```rust
match tokio::time::timeout(DB_READ_TIMEOUT, store.ping()).await {
    Ok(Ok(())) => Check::new("redis", "Redis", "ok", /* как сейчас :315 */, Some(started.elapsed().as_millis())),
    Ok(Err(err)) => { tracing::warn!(error = %err, "status services: redis ping failed");
        Check::new("redis", "Redis", "down", "Redis is not reachable", Some(started.elapsed().as_millis())) }
    Err(_) => Check::new("redis", "Redis", "unknown", "Redis check timed out", Some(started.elapsed().as_millis())),
}
```

Дополнительно: `collect_checks` (`:535`) последовательный, худший случай ≈ 6 × 1.5 c =
9 c только на таймаутах БД (+ Redis без таймаута) против 10 c UI-таймаута
(`status.js:29`). Либо `tokio::join!` для независимых проверок, либо `DB_READ_TIMEOUT` ≤ 1 000 мс.

### 5.2 Бэкенд: строка `payments` не может деградировать и дублирует логику админского чека

Публичный список жёстко ставит `payments: "ok"` (`status_routes.rs:645`) и не смотрит на
`checks["payments"]`; сам `payments_check` всегда `ok` (`:515`). При мёртвой БД страница
одновременно пишет «Service outage» и «Payments — Operational» (проверено вживую, §6).
Это единственная строка без наблюдаемого сигнала.

Исправление (бэкенд):
1. в `public_services` брать состояние из проверок, как для остальных:
   `let payments_state = state_of(checks, "payments");` (`status_routes.rs:645`) и
   `public_note(payments_state)` вместо литерала;
2. `payments_check(db_down: bool, payments: &str) -> Check` (`:510`): при `db == "down"` →
   `degraded` («Payment records unavailable»); при `payments == "live"` без реальной
   проверки провайдера → `unknown` («Provider not probed»), `ok` оставить только для
   `mock`, где «работает» проверяемо by design.

### 5.3 Фронтенд: два атрибута, которые JS пишет, а CSS не читает

- `status.js:228` пишет `data-busy`, CSS — `.refresh[aria-busy='true'] .refresh-icon`
  (`status.css:252`, `:261`): анимация обновления не запускается никогда.
- `status.js:106` пишет `data-stale`, в `status.css` селектора `stale` нет вовсе
  (0 совпадений): устаревший снимок отличается только текстом «· stale» (`status.js:104`).

Исправление (фронтенд):
1. `apps/web-status/status.js:227` — добавить `els.refresh.setAttribute('aria-busy', busy ? 'true' : 'false');`
   (оставив `dataset.busy` для стилей);
2. `apps/web-status/status.css:323` (рядом с `.pill[data-state='unknown']`) — добавить
   `.pill[data-stale='true'] { background: var(--c-raised); color: var(--c-text-muted); }`
   и `.pill[data-stale='true'] .pill-dot { background: var(--c-text-muted); }`.

### 5.4 Деплой и verify-stand не проверяют ни страницу, ни её эндпоинт

- `deploy/prod/deploy.sh:161`–`:163`: отсутствие `web/status/index.html` печатается как
  ошибка, но **не** увеличивает `errs` (сравни `:164`–`:167` для `web/dj`, где
  `errs=$((errs + 1))` стоит на `:166`) — preflight пропускает неполный артефакт.
- `health_check` (`deploy.sh:82`–`:108`) проверяет `/healthz`, `/readyz` (не зависит от БД,
  `routes.rs:237`), HTML `/status/` (`:93`) и системные юниты, но **ни разу** не дёргает
  `/v1/status/services`; мёртвая БД проходит все проверки.
- `scripts/verify-stand.mjs` обходит `/register`, `/login`, `/dj/login`, `/admin/`,
  `/dj-mock/`, мобильный `/register` (`:61`, `:84`, `:100`, `:117`, `:134`, `:149`, `:156`),
  но `/status/` — ни разу.

Исправление (скрипты/деплой):
1. `deploy.sh:162` — добавить `errs=$((errs + 1));` после `dc_err` (симметрично `:166`);
2. в `health_check` после `:90`:
   ```bash
   body=$(curl -s --max-time 5 http://127.0.0.1/v1/status/services 2>/dev/null || true)
   case $body in
     *'"status"'*'"services"'*) ok "/v1/status/services -> status document" ;;
     *) bad "/v1/status/services: no status document (API/DB?)" ;;
   esac
   ```
3. в `verify-stand.mjs` — седьмой блок перед `await browser.close()` (`:165`), со своим
   `browser.newContext(desktop)`:
   ```js
   await page.goto(`${BASE}/status/`, { waitUntil: 'networkidle' });
   const rows = await page.locator('li.check').count();
   if (rows !== 6) problems.push(`[status] expected 6 service rows, got ${rows}`);
   ```

### 5.5 Контракт и доставка: `openapi.yaml` молчит про не-200, а nginx отдаёт старый JS

- `docs/openapi.yaml:3450`–`:3474` описывает только `200`; не сказано, что тело-документ
  бывает лишь при 200, а не-2xx приходит от nginx как HTML (отсюда мёртвая предпосылка о
  «503 с документом» в `status.js:12`–`:13`), не зафиксированы порядок и количество
  сервисов.
- `dropit.conf:141`–`:145` для `/status/` не задаёт `Cache-Control` (в отличие от `/admin/`,
  `:169`), а `status.js`/`status.css` — без хэша в имени: после деплоя браузер может
  исполнить старый `status.js` против нового ответа.

Исправление:
1. `openapi.yaml:3450` — в `description` добавить: «тело-документ отдаётся только с 200;
   при недоступном API nginx возвращает 502/504 с HTML-телом», и там же зафиксировать
   «ровно 6 сервисов, порядок — как в ответе»;
2. `status.js:12`–`:13` — переформулировать в «любой HTTP-код с валидным JSON-документом
   рендерится; 502/504 от nginx приходят без документа»;
3. `dropit.conf:143` — добавить `add_header Cache-Control "no-cache" always;`
   (как в `/admin/`, `:169`).

### 5.6 Мелочи (не входят в топ-5)

- `status.js:60`/`:164`: `state.hasRendered` пишется, но не читается — мёртвое поле.
- `status_routes.rs:668`: публичный роут вызывает `collect_checks`, который тянет
  `resolved_mode` (`:545`) — лишнее чтение БД ради поля, которое публичный ответ не отдаёт.
- `status_routes.rs:683`: `checked_at` — время окончания проверок, а не их начала; при
  девятисекундном бюджете проверок подпись «Checked at» относится к моменту ответа, а не
  к моменту, когда сервисы проверялись.
- `status.js:139`: пустой `note` даст пустую третью колонку в трёхколоночной сетке
  (`status.css:409`) — сейчас недостижимо, но контракт `note` пустую строку допускает.

## 6. Живые проверки (воспроизведение)

```text
$ DROPIT_BIND=127.0.0.1:18099 ./target/debug/dropit-api        # БД на :55433
$ curl -s -D- http://127.0.0.1:18099/v1/status/services
HTTP/1.1 200 OK
content-type: application/json
cache-control: no-store
{"checked_at":"2026-10-28T23:49:28.574476+00:00","services":[
 {"id":"panel","name":"Club panel","note":"Working normally","state":"ok"},
 {"id":"dj","name":"DJ console","note":"Working normally","state":"ok"},
 {"id":"api","name":"API","note":"Working normally","state":"ok"},
 {"id":"requests","name":"Music requests","note":"Working with delays","state":"degraded"},
 {"id":"guest-entry","name":"Guest entry (QR)","note":"Working normally","state":"ok"},
 {"id":"payments","name":"Payments","note":"Test mode — no real money moves","state":"ok"}],
 "status":"degraded"}

$ psql ... -tAc "select worker, extract(epoch from (now()-last_seen_at))::int from worker_heartbeat"
dropit-workers|712                      # >45 c → workers degraded → requests degraded
$ psql ... -tAc "select count(*) from outbox where published_at is null"   # 0

$ DROPIT_BIND=127.0.0.1:18098 DATABASE_URL=postgres://dropit@127.0.0.1:1/dropit ./target/debug/dropit-api
$ curl -s -w '\nHTTP %{http_code}\n' http://127.0.0.1:18098/v1/status/services
{"checked_at":"2026-10-28T23:50:43.667859+00:00","services":[
 {"id":"panel","name":"Club panel","note":"Not available right now","state":"down"},
 {"id":"dj","name":"DJ console","note":"Not available right now","state":"down"},
 {"id":"api","name":"API","note":"Working with delays","state":"degraded"},
 {"id":"requests","name":"Music requests","note":"Not available right now","state":"down"},
 {"id":"guest-entry","name":"Guest entry (QR)","note":"Not available right now","state":"down"},
 {"id":"payments","name":"Payments","note":"Test mode — no real money moves","state":"ok"}],
 "status":"down"}
HTTP 200                                # роут не отдаёт 503 никогда

$ curl -s http://127.0.0.1:18098/readyz
{"open_signup":false,"payments":"mock","status":"ok","version":"0.1.0"}   HTTP 200
                                        # readiness не зависит от БД (routes.rs:237)

$ curl -s http://127.0.0.1:18098/v1/status
{"mode":"test","open_signup":false,"payments":"mock","runtime_mode":"auto","source":"adaptive"}
```

Оба процесса остановлены по PID из `lsof -ti tcp:18098` и `tcp:18099`; рабочая БД не
изменялась (только `SELECT`).
