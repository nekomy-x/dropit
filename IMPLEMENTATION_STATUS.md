# DROP IT — Статус реализации

**Обновлено:** 2026-10-13
**Этап:** полный стек F0–F8: бэкенд + WS + UI (гость, DJ, панели) — Test Mode

---

## 1. Что найдено в проекте (Этап 0 — ЗАВЕРШЁН)

| Факт | Статус |
|---|---|
| Проектные документы | 6 шт., все прочитаны: `trebovania.md`, `otvet-zakazchiku.md`, `koncepciya.md`, `arhitekturnoe-reshenie.md`, `dizayn-zadanie.md`, `tehnicheskaya-karta.md` |
| Git-репозиторий | **✅ Да** — `git init` выполнен (F0B): коммиты `6dd94f7` (F0A baseline), `0bfc694` (F0B+F1+F3+F7) |
| Rust toolchain локально | **✅ Да** — `~/.cargo/bin` (rustc 1.97.1) просто не был в PATH; прямые прогоны `cargo` работают |
| Docker (локальный) | Docker CLI 29.3.0; демон через colima; volume-маунт `/Volumes` глючит — обход: tar-пайп (актуально только для Docker-сборок) |
| Дизайн Pen | **Независимо не проверен**: инструмент отклонил URL (hostname → непубличный IP). Утверждения о макете взяты из документов |
| Папка `del/` | Пуста |

## 2. Что сделано (F0A → F1)

### F0A — скелет (проверено сборкой, тестами, миграцией)

Workspace (common, domain, infra-db, gateway-api), ADR-001..006, compose (Postgres 16 + Redis 7), CI (fmt/clippy/test/migrations/openapi-lint/docker), миграция `0001_init.sql` (platform_settings NULL-семантика, audit_log append-only), OpenAPI-заготовка. 12 тестов (money + стейт-машины) — зелёные в Docker и на хосте.

### F0B — локальная среда

| Проверка | Результат |
|---|---|
| `git init` + базлайн-коммит | ✅ `6dd94f7` |
| Прямой `cargo test --workspace` на хосте | ✅ 12 passed (F0A-набор) |
| `cargo fmt --check` / `clippy -D warnings` на хосте | ✅ чисто |

### F1 — идентичность и роли (ядровая часть фазы)

| Артефакт | Что внутри |
|---|---|
| `migrations/0002_identity.sql` | `guest_device`, `dj`, `club`, `admin_user` (CITEXT-emails, Argon2id-хеши, TOTP-секреты), `refresh_token` (ротация с семьями: reuse → ревок семьи), `login_attempt` (rate-limit 5/15 мин), `device_token_issue` (лимит 5/час/IP) |
| `crates/common/src/auth.rs` | `Role` (guest/dj/club/admin), `AccessClaims` — JWT-клеймы (sub, role, fam, iat, exp) |
| `crates/gateway-api/src/auth.rs` | Argon2id hash/verify, JWT access 15 мин, refresh 128-бит + sha256-хеш, TOTP-проверка (skew ±1 шаг) — 8 юнит-тестов |
| `crates/infra-db/src/identity.rs` | `IdentityRepo`: device-token CRUD, лимиты IP, парольные субъекты, refresh-ротация транзакцией (reuse-детекция `Err(RefreshReused)`), ревок семей, подсчёт неудачных логинов |
| `crates/gateway-api/src/routes.rs` | Роуты: `POST /v1/guest/device`, `POST /v1/auth/:role/login` (+2FA-шаг), `POST /v1/auth/refresh`, echo-роуты 4 ролей с RBAC-гейтом |
| `crates/gateway-api/src/rbac.rs` | `AuthContext`-экстрактор, единый envelope ошибок 401/403 |
| `crates/gateway-api/src/e2e.rs` | **e2e-критерий приёмки F1**: RBAC-матрица 4 ролей (своя роль → 200, чужая → **403** с `code:"forbidden"`, без токена → 401), device-token, refresh-ротация + reuse-детекция + ревок семьи; без БД — явный 503 |

### F2 — сессии, QR, join гостя (завершена)

| Артефакт | Что внутри |
|---|---|
| `migrations/0003_sessions.sql` | `pricing_policy` (история: valid_to у старой), `session` (статусы стейт-машины, IANA-зона), `qr_token` (sha256-хеш, partial unique «один активный на сессию»), `session_guest` (UNIQUE грант, re-grant при повторном join) |
| `crates/domain/src/qr.rs` | Валидация формата QR (128 бит hex), `next_close_at` — «следующий 07:00 в IANA-зоне» (DST-безопасно, Q5), `should_auto_close` (порог неактивности 2 ч) — 5 тестов, включая переход часов ЕС |
| `crates/infra-db/src/sessions.rs` | `SessionRepo`: create/find, `apply_transition` (транзит стейт-машины, гонки → InvalidTransition), `issue_qr_token` (ротация в транзакции), `find_active_qr` (истёк = QrInvalid), `grant_session_guest` (ON CONFLICT → re-grant), `find_live_grant`, `session_info_for_guest` (клуб/DJ/цена) |
| `crates/gateway-api/src/sessions_routes.rs` | Роуты: `POST /v1/sessions/join` (device-token Bearer, формат-чек → хеш → активный QR → грант → session info), `POST /v1/club/sessions`, `qr/generate` (128 бит, expires = плановое закрытие), `qr/activate/deactivate` (каркас), `close`, `POST /v1/dj/sessions/:id/open` |
| `crates/gateway-api/src/e2e_sessions.rs` | **e2e-критерий F2**: скан тест-QR → session info (клуб/DJ/статус/зона); чужой QR → 404; мусорный формат → 400; неизвестное устройство → 401; ротация убивает старый токен; закрытая сессия → 409 session_closed; re-grant = тот же grant_id; грант по чужому session_id отсутствует |
| `docs/openapi.yaml` (0.2.0) | Контракты F1+F2 полностью; Redocly-валидация ✅ |

### F3 — домен каталога (параллельный трек, субагент)

`crates/domain/src/catalog.rs`: `CatalogTrack`, `Source`, `canonical_key("itunes:12345")`, `validate_track` (whitelist домены mzstatic/deezer, только https, лимиты полей, ISRC 12 A-Z0-9, контроль-символы), `TrackCounter` (счётчик «той же песни» по canonical-ключу). **22 теста** (15 новых).

### F4 — запросы и очередь DJ, без денег (завершена)

| Артефакт | Что внутри |
|---|---|
| `migrations/0004_requests.sql` | `track_cache` (UNIQUE(source,source_id), canonical_key), `request` (version — optimistic locking, снимок price_minor), `idempotency_response` (replay-ответы, TTL), `outbox` (ADR-002: события запросов) |
| `crates/infra-db/src/requests.rs` | `RequestRepo`: `create_request` (TX: FOR SHARE active-гейт + живой грант + INSERT + outbox; replay по Idempotency-Key; чужой гость с тем же ключом → конфликт), `dj_queue(new|hot)` (hot — счётчик по canonical-ключу), `apply_action` (no-op семантика + version-гонки → no-op, не перезапись; outbox-события) |
| `crates/gateway-api/src/requests_routes.rs` | Роуты: `POST /v1/requests` (Idempotency-Key обязателен; пауза → 409 session_paused), `GET /v1/my/requests`, `GET /v1/dj/queue?sort=`, `accept/reject/played` (no-op → 200+noop:true; недопустимый переход → 409), `acceptance/pause/resume`, `POST /v1/ws/ticket` (одноразовый, TTL 30 с, in-memory F4 → Redis F5/F8) |
| `crates/gateway-api/src/e2e_requests.rs` | **e2e-критерии F4**: запрос в очереди DJ ✅; пауза блокирует POST (409 session_paused), resume разблокирует ✅; чужой гость без гранта отклонён ✅; дубль Idempotency-Key = replay, в БД один запрос ✅; no-op на дублях accept/played, played из pending, accept после played → 409 ✅; hot-сортировка по счётчику той же песни ✅; ws-ticket одноразовый ✅; outbox-события записаны ✅ |
| `docs/openapi.yaml` (0.3.0) | Контракты F4: requests, queue, DJ-действия, pause/resume, ws-ticket; Redocly ✅ |

### F5 — платежи, Test Mode с МОК-провайдером (завершена)

| Артефакт | Что внутри |
|---|---|
| `migrations/0005_payments.sql` | `payment` (стейт-машина Часть III, split_snapshot неизменяем после captured), `split` (материализация для выплат F6, инвариант Σ == amount), `ledger_entry` (двойная запись, append-only, счёт (account, currency)), `webhook_event` (event_id UNIQUE — delivery-дедуп; UNIQUE (aggregate_id, event_type) WHERE processed — semantic-дедуп) |
| `crates/domain/src/payment.rs` | Стейт-машина Payment: все переходы техкарты, включая **cancelling→captured** (гонка) и **captured+Capture = NO-OP** (дубль вебхука); `compute_split` (округление вниз, остаток цента не теряется) — 8 тестов |
| `crates/infra-db/src/billing.rs` | `BillingRepo`: `create_payment` (снимок SplitSnapshot), `confirm_payment` (GUARD: request pending + session active FOR UPDATE/SHARE), `handle_capture_webhook` (ОДНА TX: payment→captured + split-строки + ledger двойная проводка + outbox + processed-метка; delivery- и semantic-дедуп), `begin_cancel`, инвариант-чекеры `ledger_txn_invariant` / `split_invariant` |
| `crates/gateway-api/src/payments.rs` | `PaymentProvider` trait + **MockStripe (МОК, помечен явно — правило 5)**: прод-Stripe после ADR-004/Q15; `verify_webhook_signature` (tolerance 5 мин, clock-drift — отдельная ошибка/лог) — 4 теста |
| `crates/gateway-api/src/payments_routes.rs` | `POST /v1/payments/:id/confirm` (GUARD → 409 session_paused + АВТО-CANCEL интента), `POST /v1/stripe/webhook` (подпись → дедуп → capture TX) |
| `crates/gateway-api/src/e2e_payments.rs` | **e2e chaos-минимум (Часть V)**: двойной вебхук с РАЗНЫМИ event_id → ОДНА проводка ledger ✅; delivery-дубль → no-op ✅; cancel-гонка cancelling→captured — деньги закрыты в ledger ✅; Σ ledger(txn) == 0 ✅; Σ split == amount (300/350/350 по Q1-дефолту) ✅; GUARD: пауза → 409 + интет cancelling ✅; мусорная подпись → 400 ✅ |
| `docs/openapi.yaml` (0.4.0) | Контракты F5: confirm, webhook-intake; Redocly ✅ |

**Важно:** провайдер — МОК (не Stripe): внешних вызовов нет, захват имитируется вебхуком с мок-подписью. Прод-подключение — после закрытия ADR-004 (Q15, схема сплитов) и Q11–Q13. Интерфейс `PaymentProvider` стабилен — замена мока на Stripe-клиент не меняет репо/роуты.

### F6 — выплаты и сверка, Test Mode (завершена)

| Артефакт | Что внутри |
|---|---|
| `migrations/0006_payouts.sql` | `payout` (стейт-машина scheduled→in_flight→paid/failed→on_hold, UNIQUE transfer_id, final-флаг), `reconciliation_run`, `incident` (разбор админом) |
| `crates/domain/src/payout.rs` | Стейт-машина Payout: retry ≤5 → on_hold, no-op дублей transfer-вебхуков; `decide_payout` (порог 10 € Q4, финальная без порога); backoff экспоненциальный — 5 тестов |
| `crates/infra-db/src/payouts.rs` | `PayoutRepo`: `net_by_payee` (НЕТТО ПО КАЖДОМУ payee: pending − reversed, НЕ глобально), `run_payout_cycle` (порог → перенос: split остаётся pending; сплиты → paid_out при in_flight — защита от двойной выплаты), `handle_transfer_paid/failed` (идемпотентно по transfer_id; failed×5 → on_hold + инцидент + сплиты возвращены pending), `retry_payout`, `in_flight_payouts` (часовой poller), `payee_invariant_violations` (инвариант по каждому payee), `run_reconciliation` (ledger vs captured, расхождение → инцидент), `resolve_incident` |
| `crates/gateway-api/src/payouts_routes.rs` | `POST /v1/admin/payouts/run`, `GET /v1/admin/payouts`, `POST /v1/stripe/transfer-webhook`, `POST /v1/admin/reconciliation/run`, `GET /v1/admin/incidents` + `resolve` |
| `crates/gateway-api/src/e2e_payouts.rs` | **e2e-критерии F6**: тестовая неделя — сверка ok (копейка-в-копейку) ✅; порог: бедный payee без payout, сплиты pending (перенос) ✅; финальная выплата без порога (клуб уходит) ✅; transfer.failed 5 раз → on_hold + инцидент + сплиты pending + разбор админом ✅; дубль transfer-вебхука no-op ✅; инвариант payee чист до и после ✅ |
| `docs/openapi.yaml` (0.5.0) | Контракты F6: payouts, transfer-webhook, reconciliation, incidents; Redocly ✅ |

**Важно:** transfer — МОК (transfer_id присваивается в репо); прод-Stripe-Transfers — после ADR-004/Q15. Q16 (невостребованные) закрыт механикой final-payout; политика-текст — владелец.

### F8 — hardening, Test Mode (завершена)

| Артефакт | Что внутри |
|---|---|
| `crates/domain/src/fraud.rs` | Антифрод-грань (VII): лимит 20 запросов/устройство/сессию → deny; 300 устройств/сессию → АЛЕРТ клубу (шаринг QR — принятая модель, не запрет) — 2 теста |
| `crates/common/src/crypto.rs` | **AES-256-GCM для TOTP-секретов**: формат `v2:nonce:ct`, ключ из `DROPIT_TOTP_KEY` (32 байта hex), легаси-plain читается (миграция без downtime), GCM-тег ловит подмену, отсутствие ключа = ошибка (не тихий plain) — 5 тестов |
| `crates/infra-db/src/jobs.rs` | `JobsRepo`: авто-закрытие сессий (07:00 IANA из QR-expires + неактивность 2 ч; pending-запросы → expired), TTL-интенты (15 мин → cancelling), антифрод-счётчики (гейт в POST /v1/requests → 429), GDPR: `anonymize_guest_device` (финансы НЕ удаляются, гранты ревок, platform затёрта), `retention_sweep` (180 дней), `export_guest_data` (DSR) |
| `migrations/0007_gdpr.sql` | `gdpr_action` — трейл экспортов/анонимизаций |
| `.github/workflows/ci.yml` + `deny.toml` | cargo-deny: advisories (vulnerability/yanked = deny), licenses (unlicensed = deny, GPL запрещён) |
| `crates/gateway-api/src/e2e_f8.rs` | **e2e F8**: 21-й запрос → 429 ✅; стейл-интент → cancelling ✅; сессия авто-закрыта, pending погашены ✅; GDPR: анонимизация (финансы на месте, гранты ревок), retention-sweep, DSR-экспорт ✅; TOTP v2 roundtrip + легаси ✅ |

### F7 — панели клуба и админа (завершены + подключены к контрактам)

`apps/web-club/` и `apps/web-admin/` (React 18 + TS + Vite + Tailwind).

**Каркас** (субагент, 45 файлов): типизированные API-клиенты, страницы, Layout, валидация PlatformSettings.

**Панельные эндпоинты** (`panel_routes.rs`, коммит `d71fe2d`, e2e в `e2e_panels.rs`):
- клуб: dashboard (сессия/запросы/доход), profile GET/PATCH, **pricing с историей Q6** (новая строка закрывает старую valid_to), earnings (today/week/month), djs, payouts;
- админ: dashboard/now, clubs + **блокировка/пауза с записью в audit_log**, payments + splits, audit-log, **platform-settings GET/PATCH** (NULL-семантика с clear-флагами, только валидация диапазонов — значения задаёт владелец, Q11), djs;
- DJ: shift, stats/today (§1.7);
- e2e: settings roundtrip + NULL-clear + range-reject, pricing Q6, блокировка + аудит, RBAC-негативы;
- клиенты панелей переведены на live-имплементации (осталось 3 заглушки: inviteDj, createClub, listGuests).

**Подключение к живым контрактам** (субагент, openapi.yaml 0.5.0):
- логин-флоу: `TokenPair | TotpChallenge` → 2FA-шаг → refresh-ротация; Bearer в памяти (НЕ localStorage); 401 → /login
- web-club живое: createSession (timezone + dj_id), qr/generate (qr_token строка), close session
- web-admin живое: **PayoutsPage** (список + Run cycle), **IncidentsPage** (список + Resolve), reconciliation/run
- изначально 24 эндпоинта были 501-заглушками; после панельных эндпоинтов (коммит d71fe2d) осталось 3: inviteDj, createClub, listGuests
- **Сборка проверена**: `tsc --noEmit` — 0 ошибок в обоих app; `vite build` — успешно (web-club 187 КБ, web-admin 197 КБ)

### gateway-ws: live-обновления (MVP, коммит `115b5b9`)

| Артефакт | Что внутри |
|---|---|
| `crates/gateway-api/src/ws.rs` | `WsHub` — broadcast-каналы по топикам: `queue:{session}` (DJ-очередь) и `guest:{device}` (статус гостя); `GET /v1/ws?ticket=` — апгрейд, билет одноразовый (GETDEL), первый кадр-подписка с авторизацией владения (гость — только свой канал, DJ — только своя смена); события: request.created (с названием трека и бейджем ×N по canonical-ключу), accepted/rejected (причина летит гостю), played, session.paused/resumed |
| Интеграция | push-точки в POST /v1/requests, dj accept/reject/played (no-op-дубли НЕ пушат), pause/resume |
| `e2e_ws.rs` | DJ и гость получают created/accepted/paused; билет одноразовый (второй → None); утечки между гостями нет |
| Прод-TODO | multi-node: Redis Pub/Sub-релей между узлами (задокументировано; MVP single-node) |

### UI-трек: WS-live в приложениях (коммит `b429073`)

- **DJ** (`mobile-dj`): `WsClient` (билет → подписка queue) +
  `QueueState`-редьюсер — **очередь обновляется без
  pull-to-refresh**: вставка сверху (sort=new), сортировка по ×N
  (hot), статусы карточек, баннер PAUSED из session.paused;
- **гость** (`mobile-guest`): guest-канал — статусы своих запросов
  (rejected **с причиной DJ**), played;
- деградация: `connectQueueLive/connectGuestLive` → false без
  транспорта (web_socket_channel подключается в прод-сборке) —
  экраны работают через ручной refresh;
- тесты: редьюсер (new/hot/статусы/пауза), мок-стрим, UI-флип
  статуса, проброс причины гостю.

### UI-трек: Flutter-приложение DJ (каркас)

`apps/mobile-dj/` — dizayn-zadanie §1 (экраны читаются в темноте одним
взглядом — крупная типографика):
- **§1.1 логин**: 2 шага (email+пароль → `mfa_required` → крупное
  6-значное поле TOTP); Forgot password;
- **§1.4 очередь (главный)**: карточки с бейджем **×N той же песни**
  (социальный сигнал), time-ago, стиль; переключатель сортировки
  new|hot (бэкенд поддерживает);
- **§1.5 раскрытие**: крупные Принять · Отклонить (шаблоны причин —
  отправляются гостю) · Сыграна; ISRC для Rekordbox/Serato;
  409-конфликт и no-op-дубли обрабатываются;
- **§1.3 настройка вечера**: тумблер приёма 72px (open/paused —
  вызывает pause/resume роуты), стили-чипы (ЗАГЛУШКА: персист —
  эндпоинт не реализован, правило 5);
- **§1.7 статистика**: заработок крупно (из минорных единиц),
  принято/сыграно/отклонено, acceptance rate;
- бэкенд DJ-роуты уже полные (F4: queue, no-op-действия, пауза
  блокирует POST /requests — покрыто e2e);
- заглушки (правило 5): «сейчас играет» отдельным экраном (после
  WS F9), обложки (B5), пик-часы.

### UI-трек: Flutter-приложение гостя (каркас)

`apps/mobile-guest/`: тема по токенам dizayn-zadanie §5 (чёрный/неон), API-клиент контрактам openapi (device-token, join, requests, ws-ticket, ErrorEnvelope i18n-коды), экраны: QR-скан (ввод токена до B5-камеры), сессия (динамическая цена §0.1), поиск (демо до B5), **обязательный экран условий оплаты §0.2** (3 пункта + чекбокс-гейт), My requests. Widget-тесты: пустой ввод, динамическая цена (не хардкод €5), гейт условий. `flutter test` требует установки Flutter SDK (на хосте нет).

## 3. Проверено (реальные результаты, F1-прогон)

| Проверка | Результат |
|---|---|
| `cargo fmt --all -- --check` (хост) | ✅ чисто |
| `cargo clippy --workspace --all-targets -- -D warnings` (хост) | ✅ 0 ошибок |
| `cargo test --workspace` (хост, БД из compose) | ✅ **80 passed / 0 failed** (13 + 42 + 23 gateway + 2, включая e2e WS) |
| OpenAPI 3.1 (Redocly lint) | ✅ valid (0.7.0, WS-контракт) |
| Git-история | ✅ 12 коммитов: F0A → … → F6 → docs → F8 |

## 4. Что является mock/stub или отсутствует

- **Логин-флоу 2FA — упрощён в F1**: второй шаг (`/v1/auth/:role/2fa`) перепроверяет код по секрету, но НЕ связан с первым шагом промежуточным токеном (полноценный flow — F8 hardening); TOTP-секреты в тестах хранятся base32-plain (прод-шифрование AES-GCM — F8).
- **IP в rate-limit** — заглушка `127.0.0.1` (X-Forwarded-For за reverse-proxy — F8).
- Stripe: **не подключён** (ADR-004 открыт). NATS: не поднимается (F5).
- Каталог: внешние вызовы iTunes/Deezer/MusicBrainz **не пишутся** (B5 — лицензии); есть только серверная валидация и кэш-структура.
- `apps/web-dj/index.html` — HTML-прототип из параллельного UI-трека (не Flutter-приложение).
- F2 (сессии, QR), F4 (WS, очередь), F5–F6 (деньги) — не начаты.

## 5. Открытые вопросы владельцу (Q11–Q16)

Без изменений: Q11 (комиссия), Q12 (возврат при отклонении), Q13 (страна пилота), Q14 (юрлицо), Q15 (Stripe-схема, ADR-004), Q16 (невостребованные деньги). Дефолты Q1–Q10 рабочие, значения — конфиги.

## 6. План F0–F9 и текущая позиция

| Фаза | Статус |
|---|---|
| F0A/F0B | ✅ завершены |
| **F1 Идентичность** | ✅ код + тесты; e2e-критерий (4 роли, wrong-role → 403) выполнен |
| **F2 Сессии+QR** | ✅ код + тесты; e2e-критерий (скан QR → session info, чужой QR отклонён) выполнен |
| **F4 Запросы+очередь** | ✅ код + тесты (без денег); WS — ticket-выдача, соединения gateway-ws — F6 |
| **F5 Платежи Test Mode** | ✅ код + тесты, МОК-провайдер; прод-Stripe ждёт ADR-004/Q11–Q15 |
| **F6 Выплаты/сверка** | ✅ код + тесты (Test Mode): цикл выплат, порог, финальные, on_hold+инциденты, сверка, инвариант payee |
| **F8 Hardening** | ✅ антифрод-лимиты, TOTP-шифрование, GDPR-джобы, авто-закрытие/TTL-воркеры, cargo-deny |
| F3 Каталог | 🟡 домен-валидация готова; внешние интеграции — блок B5 |
| F7 Панели | 🟡 каркасы готовы; контракты F1/F2 в openapi.yaml |
| F8/F9 | не начаты |

**Весь бэкенд F0→F1→F2→F4→F5→F6→F8 пройден в Test Mode.** Осталось: прод-интеграции (ADR-004, B5), UI (Flutter + панели), F9 (нагрузка/пилот).

## 7. Текущие блокеры

| # | Блокер | Что блокирует | Временный режим |
|---|---|---|---|
| ~~Б1 cargo локально~~ | — | — | **снят**: toolchain найден в `~/.cargo/bin` |
| Б2 Pen-дизайн недоступен | точную реализацию UI | работаем от текстовых описаний + HTML-прототипы |
| Б3 Q11–Q16 без ответов | платежи/выплаты/возвраты | платежи не включаются; параметры — конфиги |
| ~~Б4 нет Git~~ | — | — | **снят**: репозиторий инициализирован |
| Б5 Лицензии каталога | F3-интеграции | валидация+кэш готовы, внешние вызовы не пишутся |
| Б6 Volume-маунт colima | Docker-сборки | tar-пайп; сборка на хосте не нужна |

## 8. Параллельный трек: UI-прототипы (не ждут бэкенд)

HTML-прототипы в фирменном стиле (чёрный/неон, токены в CSS-переменных), служат заданием дизайнеру и основой React-панелей. Данные — моки поверх планируемого REST-контракта.

| Дата | Артефакт | Что покрывает |
|---|---|---|
| 2026-10-13 | `apps/web-dj/index.html` | Консоль диджея (dizayn-zadanie §1.2–1.8): очередь, сортировка, пауза, «сейчас играет», Принять/Отклонить/Сыграна, шаблоны причин, ISRC, статистика, состояния offline/переподключение |

Токены и план прототипов: `docs/ui/README.md`.

## 9. Следующий маленький этап

**Дальше (выбор владельца):**
1. **Хвосты панелей** — inviteDj (email-инвайт), createClub (верификация), listGuests (антифрод-скоринг);
2. **Транспорт WS в приложениях** — web_socket_channel в pubspec + connectSocket (контракт и редьюсеры готовы и протестированы);
3. **Прод-интеграции** — Stripe Connect (ждёт ADR-004/Q11–Q15), каталог (B5 лицензии);
4. **F9** — k6-нагрузка, soak, пилот, сторы.

Flutter SDK на хосте не установлен: `flutter test` для mobile-guest не прогнан (виджет-тесты написаны; прогон — при установке SDK). Панели собраны и проверены.

Джобы (авто-закрытие, TTL-интенты, retention) — методы JobsRepo; прод-cron-триггеры — воркер-бинарник `workers` при переходе к деплою. WS-соединения (gateway-ws) и NATS-релье outbox — с инфраструктурой F9/деплоя.

---

## Журнал проверок

| Дата | Команда | Результат |
|---|---|---|
| 2026-10-12 | `ls -la`, `git rev-parse`, `docker --version`, `rustc/cargo --version` | 6 документов; не git-репо; Docker 29.3.0; rustc/cargo «отсутствуют» (не в PATH) |
| 2026-10-13 | F0A-проверки в Docker | compose ✅, миграция ✅, build ✅, healthz/readyz ✅, 12 тестов ✅, clippy/fmt ✅ |
| 2026-10-13 | `git init` + baseline | ✅ коммит `6dd94f7` |
| 2026-10-13 | `cargo test --workspace` на хосте (toolchain в `~/.cargo/bin`) | ✅ 12 passed (F0A-набор) |
| 2026-10-13 | F1: `cargo test --workspace` | ✅ **39 passed / 0 failed** (включая e2e RBAC-матрицу) |
| 2026-10-13 | F1: `cargo clippy -D warnings` / `fmt --check` | ✅ 0 ошибок / чисто |
| 2026-10-13 | F1: `git commit` | ✅ `0bfc694` |
| 2026-10-13 | F2: миграции 0002+0003 применены в compose-БД | ✅ 13 таблиц |
| 2026-10-13 | F2: `cargo test --workspace` | ✅ **46 passed / 0 failed** (e2e F2: QR-флоу полный) |
| 2026-10-13 | F2: OpenAPI 0.2.0 Redocly lint | ✅ valid |
| 2026-10-13 | F2: `git commit` | ✅ `458db86` |
| 2026-10-13 | F4: миграция 0004 применена | ✅ 17 таблиц |
| 2026-10-13 | F4: `cargo test --workspace` | ✅ **48 passed / 0 failed** (e2e F4: полный request-флоу) |
| 2026-10-13 | F4: OpenAPI 0.3.0 Redocly lint | ✅ valid |
| 2026-10-13 | F4: `git commit` | ✅ `d752a29` |
| 2026-10-13 | F5: миграция 0005 применена | ✅ 21 таблица |
| 2026-10-13 | F5: `cargo test --workspace` | ✅ **61 passed / 0 failed** (chaos: двойной вебхук, cancel-гонка, инварианты) |
| 2026-10-13 | F5: OpenAPI 0.4.0 Redocly lint | ✅ valid |
| 2026-10-13 | F5: `git commit` | ✅ `7e93aa0` |
| 2026-10-13 | F6: миграция 0006 применена | ✅ 24 таблицы |
| 2026-10-13 | F6: `cargo test --workspace` | ✅ **67 passed / 0 failed** (e2e F6: цикл выплат+сверка) |
| 2026-10-13 | F6: OpenAPI 0.5.0 Redocly lint | ✅ valid |
| 2026-10-13 | F6: `git commit` | ✅ `bf1d10d` |
| 2026-10-13 | F8: миграция 0007 применена | ✅ 25 таблиц |
| 2026-10-13 | F8: `cargo test --workspace` | ✅ **75 passed / 0 failed** (e2e F8: антифрод+джобы+GDPR+crypto) |
| 2026-10-13 | F8: `git commit` | ✅ `cca8357` |
| 2026-10-13 | UI: Flutter guest app | ✅ коммит `d55ccc1` (структура+экраны+тесты) |
| 2026-10-13 | UI: панели — `npm install` + `tsc --noEmit` | ✅ 0 ошибок в обоих app |
| 2026-10-13 | UI: панели — `vite build` | ✅ обе собираются (187/197 КБ) |
| 2026-10-13 | UI: панели подключены к контрактам | ✅ коммит `1d22ddb` |
| 2026-10-13 | Панельные эндпоинты: e2e | ✅ settings/pricing/блокировка/RBAC |
| 2026-10-13 | Панельные эндпоинты: клиенты live | ✅ tsc+vite зелёные |
| 2026-10-13 | Панельные эндпоинты: коммит | ✅ `d71fe2d` |
| 2026-10-13 | DJ-приложение (Flutter, §1) | ✅ коммит `925ec20` |
| 2026-10-13 | gateway-ws MVP: e2e | ✅ DJ+гость, билеты, изоляция |
| 2026-10-13 | gateway-ws MVP: коммит | ✅ `115b5b9` |
| 2026-10-13 | WS-live в приложениях | ✅ коммит `b429073` |
