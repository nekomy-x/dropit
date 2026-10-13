# DROP IT

Платформа заказа музыки в ночных клубах: гость сканирует QR сессии, находит трек и платит за отправку запроса диджею. Решение о воспроизведении принимает диджей.

## Статус

**Этап:** F0A (фундамент). См. `IMPLEMENTATION_STATUS.md` — что сделано, что проверено, что блокируется.

**Важно:** Rust toolchain может отсутствовать в среде — статус сборки/тестов честно отражён в `IMPLEMENTATION_STATUS.md`, а не предполагается.

## Структура

```
Cargo.toml               # Rust workspace
crates/
  common/                # деньги (minor units), ошибки, идентификаторы
  domain/                # чистые домены: state machines сессий/запросов
  infra-db/              # sqlx, миграции
  gateway-api/           # REST-бинарник (Axum)
docs/
  adr/                   # ADR-001..006 (004 — открыт: Stripe схема)
  openapi.yaml           # контракт API (растёт по фазам)
deploy/
  docker-compose.yml     # local: Postgres + Redis (NATS — с F5)
  Dockerfile
  .env.example           # БЕЗ реальных секретов
.github/workflows/ci.yml # fmt / clippy / test / миграции / openapi lint
```

## Локальный запуск

```bash
# инфраструктура
docker compose -f deploy/docker-compose.yml up -d

# приложение (нужен Rust toolchain)
cp deploy/.env.example deploy/.env
export DATABASE_URL=postgres://dropit:dropit_local@localhost:5432/dropit
cargo run -p dropit-gateway-api

# проверки
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Правила проекта

1. Суммы — только целые минорные единицы (ADR-003). Float в деньгах запрещён.
2. Бизнес-параметры (комиссия, пределы цен, доли) — конфигурация; значения задаёт владелец.
3. Stripe — не подключён; production-платежи запрещены (ADR-004 открыт).
4. Секреты только в `.env`/секретном хранилище — никогда в репозитории.
5. Mock/stub явно помечаются в коде и документации.
