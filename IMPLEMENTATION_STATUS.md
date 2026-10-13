# DROP IT — Статус реализации

**Обновлено:** 2026-10-13
**Этап:** F0A + F0B + F1 + F2 завершены (код+тесты); F3-домен и F7-каркасы созданы параллельно

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

### F7 — каркасы панелей (параллельный трек, субагент)

`apps/web-club/` и `apps/web-admin/` (React 18 + TS + Vite + Tailwind, 45 файлов): типизированные API-клиенты контракта Части IV (CSRF, httpOnly-cookie, Idempotency-Key), страницы панелей, валидация PlatformSettings (бипсисы 0–10000, мин<макс). npm не запускался (нет сети), сборка — при первом запуске разработчиком.

## 3. Проверено (реальные результаты, F1-прогон)

| Проверка | Результат |
|---|---|
| `cargo fmt --all -- --check` (хост) | ✅ чисто |
| `cargo clippy --workspace --all-targets -- -D warnings` (хост) | ✅ 0 ошибок |
| `cargo test --workspace` (хост, БД из compose) | ✅ **46 passed / 0 failed** (8 common + 28 domain + 10 gateway, включая e2e F1 и F2) |
| OpenAPI 3.1 (Redocly lint) | ✅ valid (0 errors) |
| Git-история | ✅ 4 коммита: F0A → F0B+F1+F3+F7 → docs → F2 |

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
| F3 Каталог | 🟡 домен-валидация готова; внешние интеграции — блок B5 |
| F4 Запросы+WS | ⏳ следующая: request-lifecycle, WS-ticket, очередь DJ |
| F5/F6 Деньги | не начаты (ждут Q11–Q15) |
| F7 Панели | 🟡 каркасы готовы; контракты F1/F2 в openapi.yaml |
| F8/F9 | не начаты |

**Критический путь: F4 → F5.**

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

**F4**: миграция `0004_requests.sql` (request, track_cache-ссылки, счётчики), домен-переходы запросов DJ (accept/reject/played уже есть в domain), POST /v1/requests с грант-гейтом (живой session_guest + status == active), пауза приёма, WS-ticket (одноразовый, TTL 30 с) и очередь DJ. После — F5 (Stripe, требует Q11–Q15).

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
