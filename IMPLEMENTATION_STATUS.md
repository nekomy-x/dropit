# DROP IT — Статус реализации

**Обновлено:** 2026-10-13
**Этап:** F0A завершён (проверено сборкой в Docker); параллельный трек UI запущен (см. §8)

---

## 1. Что найдено в проекте (Этап 0 — ЗАВЕРШЁН)

| Факт | Статус |
|---|---|
| Проектные документы | 6 шт., все прочитаны: `trebovania.md`, `otvet-zakazchiku.md`, `koncepciya.md`, `arhitekturnoe-reshenie.md`, `dizayn-zadanie.md`, `tehnicheskaya-karta.md` |
| Исходный код, сборочная конфигурация | До F0A отсутствовали; **созданы в F0A** |
| Git-репозиторий | **Нет** — рекомендован `git init` |
| Docker (локальный) | Docker CLI 29.3.0; демон поднимался через `colima start` (vz), после сбоя `colima stop -f` + `colima start` → работает |
| Rust toolchain локально | `rustc`/`cargo` **отсутствуют** — все проверки выполнялись в контейнере `rust:1-slim` (Docker) |
| Дизайн Pen | **Независимо не проверен**: инструмент отклонил URL (hostname → непубличный IP). Утверждения о макете взяты из документов |
| Папка `del/` | Пуста |

## 2. Что сделано в F0A

**Созданные файлы:**

| Файл | Назначение |
|---|---|
| `Cargo.toml` | Rust workspace (common, domain, infra-db, gateway-api) |
| `crates/common/` | деньги (MinorUnits, Money — minor units, no float), единый ErrorEnvelope |
| `crates/domain/` | стейт-машины session/request (no-op семантика дублей, invalid transitions) |
| `crates/infra-db/` | sqlx-каркас, миграции |
| `crates/infra-db/migrations/0001_init.sql` | platform_settings (значения NULL = не заданы владельцем), audit_log (append-only) |
| `crates/gateway-api/` | Axum REST: /healthz, /readyz, graceful shutdown |
| `docs/adr/ADR-001..006` | архитектурные решения (004 — открыт: Stripe) |
| `docs/openapi.yaml` | заготовка контракта (health + ErrorEnvelope) |
| `deploy/docker-compose.yml` | local: Postgres 16 + Redis 7 (NATS — с F5) |
| `deploy/Dockerfile` | двухэтапная сборка (rust:1-slim → debian-slim) |
| `deploy/.env.example` | переменные окружения без секретов |
| `.github/workflows/ci.yml` | fmt / clippy / test / миграции / openapi lint / docker build |
| `.gitignore`, `README.md` | инфраструктура репозитория |

**Проверено (реальные результаты):**

| Проверка | Команда | Результат |
|---|---|---|
| Compose-валидация | `docker compose -f deploy/docker-compose.yml config` | ✅ валидна |
| Инфраструктура | `docker compose up -d` → `ps` | ✅ Postgres + Redis **healthy** |
| Миграция | `docker cp` + `psql -f 0001_init.sql` | ✅ таблицы `platform_settings`, `audit_log` созданы; `commission_mode = 'unset'`, NULL-параметры |
| Сборка | `docker build -f deploy/Dockerfile` | ✅ образ собран (код компилируется) |
| API runtime | `docker run` + `curl /healthz`, `/readyz` | ✅ `{"status":"ok"}`, `{"status":"ok","version":"0.1.0"}` |
| **Юнит-тесты** | `cargo test --workspace` (в контейнере rust:1-slim) | ✅ **12 passed / 0 failed** (5 money + 7 state machines) |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` (в контейнере) | ✅ чисто (только future-incompat note от sqlx v0.7.4 — известное) |
| Fmt | `cargo fmt --all -- --check` (в контейнере) | ✅ чисто (после прогона `cargo fmt` и копирования обратно) |

**Важные детали реализации:**

- Docker-демон поднимался через colima; первый `colima start` упал («attach disk in use»), вылечено `colima stop -f` + `colima start`.
- Volume-маунт `/Volumes` в colima работает некорректно (файлы не видны контейнеру) → исходники передавались через `tar` по stdin.
- `Cargo.lock` не создаётся локально (нет cargo) — Dockerfile не копирует его; в CI lock будет сгенерирован на месте.
- Первая сборка упала: `getrandom 0.4.3` требует edition2024 → базовый образ поднят с `rust:1.83` на `rust:1` (1.8x+); вторая — `anyhow` отсутствовал в gateway-api (добавлен).

## 3. Что является mock/stub или отсутствует

- Stripe: **не подключён** (ни Test, ни Live) — ADR-004 открыт.
- NATS: не поднимается (вводится в F5).
- Каталог (iTunes/Deezer/MusicBrainz): **не подключён** — до проверки лицензий (B5).
- Бизнес-значения (комиссия, мин/макс цены, доли): **не заданы** — в `platform_settings` NULL, задаёт владелец.
- Веб-панели, Flutter: не начаты (F7/этап UI).
- Реальных БД-репозиториев нет — только каркас `infra-db` (модели данных создаются в F1–F2).

## 4. Открытые вопросы владельцу (Q11–Q16 из техкарты)

| # | Вопрос | Что блокирует | Дедлайн |
|---|---|---|---|
| Q11 | Финальная комиссия DROP IT (фикс/процент, per-club) | прод-значения конфига | до F5-prod |
| Q12 | Возврат при отклонении DJ (политика) | тексты UI условий оплаты | до дизайна F4 |
| Q13 | Страна/страны пилота | Stripe-методы, VAT, юрист | до F5 |
| Q14 | Юрлицо для Stripe/сторов | онбординг Connect | до F9 |
| Q15 | Схема Stripe-сплитов + float (ADR-004) | реализация F5 | до F5, согласовать с бухгалтером |
| Q16 | Политика невостребованных денег | воркеры финальных выплат | до F6 |

Дефолты Q1–Q10 (30%, 1–10 €, 50/50, автовозврат expired и т.д.) — рабочие, но **не утверждённые**: значения остаются конфигами, не кодом.

## 5. План F0A–F9

| Фаза | Содержимое | Критерий приёмки | Зависимости |
|---|---|---|---|
| **F0A** ✅ | Workspace, ADR, Compose, CI, миграционный скелет, OpenAPI-заготовка | сборка+тесты+fmt+clippy зелёные (в Docker) | — |
| **F0B** | Локальный rustup + прямой прогон `cargo` (без Docker-контейнера) | `cargo test` зелёный на хосте; `git init` | rustup |
| **F1** | Идентичность: device-token гостя, логины DJ/клуб/админ, JWT+refresh, 2FA (TOTP), RBAC | e2e-тест: 4 роли, wrong-role → 403 | F0B |
| **F2** | Сессии, QR (128-бит, hash, ротация), join гостя, грант session_guest, авто-закрытие по IANA-TZ | скан тест-QR → session info; чужой session_id отклонён | F1 |
| **F3** | Каталог: клиентский поиск + серверная whitelist-валидация, кэш, ISRC | поиск работает, кэш-hit < 20 мс | F2, **B5** |
| **F4** | Запросы: state machine, WS ticket/ACL, очередь DJ, пауза, REST-fallback, гонки | запрос у DJ < 500 мс; пауза блокирует POST | F2 |
| **F5** | Stripe **Test Mode**: PaymentIntent, webhook-intake, outbox, ledger, идемпотентность, chaos | деньги не теряются во всех сценариях отказов | F0–F4, **Q11/Q12/Q13/Q15** |
| **F6** | Выплаты, пороги, transfer-вебхуки, reconciliation, инварианты, partition-manager | сверка сошлась копейка-в-копейку | F5, ADR-004 |
| **F7** | Панели клуба и админа (React+TS) | клуб создаёт сессию и QR без разработчика | контракты + дизайн (B6) |
| **F8** | Антифрод, аудит, 2FA-принуждение, GDPR-джобы, restore-drill | чек-лист безопасности VII на 100 % | F1–F6 |
| **F9** | k6 нагрузка, пилот, сторы | SLO 24 ч soak; сторы приняли | F5–F8, Q14, Q13 |

**Критический путь: F0B → F1 → F2 → F4 → F5.**

## 6. Текущие блокеры

| # | Блокер | Что блокирует | Временный режим |
|---|---|---|---|
| Б1 | Локальный `cargo`/`rustc` отсутствуют | прямые прогоны на хосте | проверки через `rust:1-slim` контейнер (Docker) — работает |
| Б2 | Pen-дизайн недоступен | точную реализацию UI | работаем от текстовых описаний в `dizayn-zadanie.md` |
| Б3 | Q11–Q16 без ответов | платежи/выплаты/возвраты | платежи не включаются; параметры — конфиги |
| Б4 | Нет Git-репозитория | история коммитов, CI-запуск | файлы готовы; рекомендован `git init` |
| Б5 | Лицензии каталога не проверены | F3 | интеграция не пишется |
| Б6 | Volume-маунт `/Volumes` в colima глючит | удобство локальной работы | обходной путь: tar-пайп (работает) |

## 7. Следующий маленький этап

**F0B → F1**: инициализировать Git-репозиторий (`git init` + первый коммит), при возможности установить rustup для прямых прогонов; затем начать F1 (идентичность: device-token, логины, RBAC) — миграции `0002_identity.sql` и е2е-тест ролей.

---

## 8. Параллельный трек: UI-прототипы (не ждут бэкенд)

Пока бэкенд-этапы F1–F4 не начаты, ведётся независимый трек фронтенда — HTML-прототипы в
фирменном стиле (чёрный/неон, токены в CSS-переменных), которые служат и заданием дизайнеру,
и основой для будущих React-панелей. Данные — моки поверх планируемого REST-контракта.

| Дата | Артефакт | Что покрывает |
|---|---|---|
| 2026-10-13 | `apps/web-dj/index.html` | Консоль диджея (dizayn-zadanie §1.2–1.8): очередь с живыми запросами, сортировка новые/по-запросам, пауза приёма, «сейчас играет», карточка с Принять/Отклонить/Сыграна, шаблоны причин отклонения, ISRC-копирование, статистика вечера (доля DJ), состояния: пустая очередь, офлайн/переподключение, входящие запросы в реальном времени (эмуляция WS) |

Токены и план прототипов: `docs/ui/README.md`. Следующие: панель клуба (QR-генератор),
экран условий оплаты гостя (блокирует F5 по §4.2 концепции).

---

## Журнал проверок

| Дата | Команда | Результат |
|---|---|---|
| 2026-10-12 | `ls -la`, `git rev-parse`, `docker --version`, `rustc/cargo --version` | 6 документов; не git-репо; Docker 29.3.0; rustc/cargo отсутствуют |
| 2026-10-13 | `colima start` (после `stop -f`) | Docker-демон поднялся (29.2.1) |
| 2026-10-13 | `docker compose config` | ✅ валидна |
| 2026-10-13 | `docker compose up -d && ps` | ✅ Postgres 16 + Redis 7, статус healthy |
| 2026-10-13 | `psql -f 0001_init.sql` (docker exec) | ✅ миграция применена; platform_settings: unset/NULL |
| 2026-10-13 | `docker build -f deploy/Dockerfile` | ✅ образ собран (rust:1-slim; 2 итерации правок) |
| 2026-10-13 | `curl /healthz, /readyz` | ✅ оба OK |
| 2026-10-13 | `cargo test --workspace` (контейнер) | ✅ 12 passed / 0 failed |
| 2026-10-13 | `cargo clippy -D warnings` (контейнер) | ✅ чисто |
| 2026-10-13 | `cargo fmt --check` (контейнер) | ✅ чисто (после авто-формата) |
