# DROP IT — платформа заказа музыки в ночных клубах

Гость сканирует QR сессии → платит за запрос трека → DJ решает, играть ли.
Клуб и DJ получают доли; платформа — комиссию.

**Состояние: бэкенд доведён до плана.** Внешние интеграции (Stripe, push,
каталог, NATS, Redis) написаны и включаются env-переменными; без них —
mock/noop (Test Mode). Полный статус — `IMPLEMENTATION_STATUS.md`;
план доводки — `docs/BACKEND_COMPLETION_PLAN.md`; контракт —
`docs/openapi.yaml` (0.7.0); техкарта — `tehnicheskaya-karta.md`.

---

## Бэкенд

Rust workspace: `common`, `domain`, `infra-db`, `infra-redis`, `infra-nats`,
`infra-stripe`, `infra-catalog`, `infra-push`, `workers` (бинарник
`dropit-workers`), `gateway-api` (бинарник `dropit-api`, axum 0.7). 65
HTTP-путей, 10 миграций, 403 теста.

### Запуск

Локальный стенд (Docker, приближён к проду — Postgres, Redis, NATS JetStream,
API, workers):

```sh
docker compose -f deploy/docker-compose.yml up -d
# воркер применяет миграции при старте; healthcheck API — http://localhost:8080/healthz
```

На хосте (dev):

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_HOME="$(pwd)/.cargo-home"     # без неё cargo падает (Operation not permitted)
export DATABASE_URL="postgres://dropit:dropit_local@localhost:5432/dropit"
export REDIS_URL="redis://localhost:6379"
export NATS_URL="nats://localhost:4222"

cargo run -p dropit-gateway-api --bin seed_demo     # демо-данные (клуб/DJ/админ+TOTP)
cargo run -p dropit-gateway-api --bin dropit-api    # API: 0.0.0.0:8080
cargo run -p dropit-workers -- run                  # супервизор фоновых задач
cargo run -p dropit-workers -- --once outbox        # один прогон задачи (CI/крон)
```

### Тесты

```sh
# unit-тесты крейта (без БД):
cargo test -p dropit-infra-stripe

# БД-зависимые e2e — ТОЛЬКО через лок-скрипт (общая БД, параллельные агенты):
./scripts/db-test.sh cargo test -p dropit-gateway-api e2e_payments -- --test-threads=1

cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

### Переменные окружения

| Переменная | Дефолт / деградация |
|---|---|
| `DATABASE_URL` | нет → data-роуты 503, БД-задачи skip |
| `REDIS_URL` | нет → WS-тикеты и pub/sub in-memory (один узел) |
| `NATS_URL` | нет → NoopBus, outbox-реле skip |
| `STRIPE_API_KEY` / `STRIPE_WEBHOOK_SECRET` | пусто → MockStripe |
| `FCM_*` / `APNS_*` | нет → NoopPush |
| `TRUST_PROXY` | `1` (прод за nginx) — доверять `X-Forwarded-For`/`X-Real-IP` для IP-лимитов; `0` — игнорировать |
| `DROPIT_JWT_SECRET`, `DROPIT_TOTP_KEY`, `DROPIT_BIND`, `DROPIT_RUN_MIGRATIONS`, `RUST_LOG` | см. `deploy/.env.example` |

Подробнее: `IMPLEMENTATION_STATUS.md` (раздел «Переменные окружения» и
«Внешние сервисы и деградация»), `docs/BACKEND_COMPLETION_PLAN.md`,
`docs/openapi.yaml`.

---

## Потрогать за 5 минут (Test Mode)

### 1. База (однократно)

```sh
docker compose up -d        # Postgres 16 (deploy-postgres-1)
# миграции:
for f in crates/infra-db/migrations/*.sql; do
  docker cp "$f" deploy-postgres-1:/tmp/m.sql
  docker exec deploy-postgres-1 psql -U dropit -d dropit -v ON_ERROR_STOP=1 -f /tmp/m.sql
done
```

### 2. Демо-данные

```sh
export DATABASE_URL="postgres://dropit:dropit_local@localhost:5432/dropit"
cargo run -p dropit-gateway-api --bin seed_demo
```

Создаёт клуб/DJ/админа и 5 треков:
- клуб `demo@club.test` / `demo-club-pass`
- DJ `demo@dj.test` / `demo-dj-pass`
- админ `demo@admin.test` / `demo-admin-pass` + TOTP-ключ
  `JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP` (введите в Google Authenticator)

### 3. Сервер

```sh
cargo run -p dropit-gateway-api --bin dropit-api   # 127.0.0.1:8080
```

### 4. Живой флоу (curl)

```sh
B=http://127.0.0.1:8080
# логины
DJ=$(curl -s -X POST $B/v1/auth/dj/login -H "Content-Type: application/json" \
  -d '{"email":"demo@dj.test","password":"demo-dj-pass"}' | python3 -c "import sys,json;print(json.load(sys.stdin)['access_token'])")
GUEST=$(curl -s -X POST $B/v1/guest/device -H "Content-Type: application/json" \
  -d '{"platform":"ios"}' | python3 -c "import sys,json;print(json.load(sys.stdin)['device_token'])")

# сессия + QR (клубом)
CLUB=$(curl -s -X POST $B/v1/auth/club/login -H "Content-Type: application/json" \
  -d '{"email":"demo@club.test","password":"demo-club-pass"}' | python3 -c "import sys,json;print(json.load(sys.stdin)['access_token'])")
DJ_ID=$(docker exec deploy-postgres-1 psql -U dropit -d dropit -tAc "SELECT id FROM dj WHERE email='demo@dj.test'")
S=$(curl -s -X POST $B/v1/club/sessions -H "Content-Type: application/json" -H "Authorization: Bearer $CLUB" \
  -d "{\"dj_id\":\"$DJ_ID\",\"timezone\":\"Europe/Madrid\"}" | python3 -c "import sys,json;print(json.load(sys.stdin)['id'])")
QR=$(curl -s -X POST $B/v1/club/sessions/$S/qr/generate -H "Authorization: Bearer $CLUB" \
  | python3 -c "import sys,json;print(json.load(sys.stdin)['qr_token'])")

# гость: join по QR → открытие смены DJ → запрос
curl -s -X POST $B/v1/sessions/join -H "Content-Type: application/json" -H "Authorization: Bearer $GUEST" -d "{\"qr_token\":\"$QR\"}"
curl -s -X POST $B/v1/dj/sessions/$S/open -H "Authorization: Bearer $DJ"
TRACK=$(docker exec deploy-postgres-1 psql -U dropit -d dropit -tAc "SELECT id FROM track_cache LIMIT 1")
R=$(curl -s -X POST $B/v1/requests -H "Content-Type: application/json" -H "Authorization: Bearer $GUEST" \
  -H "Idempotency-Key: smoke1" -d "{\"track_id\":\"$TRACK\"}")

# DJ: очередь → принять
curl -s "$B/v1/dj/queue?sort=new" -H "Authorization: Bearer $DJ"
curl -s -X POST $B/v1/dj/requests/$(echo $R | python3 -c "import sys,json;print(json.load(sys.stdin)['request_id'])")/accept \
  -H "Authorization: Bearer $DJ"
```

### 5. Панели (собранные)

```sh
cd apps/web-club  && npm install && npm run dev   # панель клуба
cd apps/web-admin && npm install && npm run dev   # панель админа
```

### Приложения (Flutter)

`apps/mobile-guest/`, `apps/mobile-dj/` — каркасы с темой и WS-live
(см. README внутри; `flutter run` при установленном SDK).

---

## Что работает (Test Mode)

| Модуль | Статус |
|---|---|
| Identity (JWT, 2FA, RBAC, refresh-ротация) | ✅ |
| Сессии, QR (IANA-зоны, авто-закрытие) | ✅ |
| Запросы треков, очередь DJ, пауза приёма | ✅ |
| Платежи (МОК-провайдер, вебхуки, идемпотентность, ledger) | ✅ |
| Выплаты (порог, финальные, on_hold+инциденты, сверка) | ✅ |
| Антифрод, GDPR-джобы, TOTP-шифрование | ✅ |
| WS-live (очередь DJ, статусы гостя) | ✅ |
| Панельные эндпоинты (dashboard, тарифы, настройки) | ✅ |
| Панели клуба/админа (React) | ✅ live |
| Приложения гость/DJ (Flutter) | ✅ каркасы |

**403 теста / clippy 0 / OpenAPI 0.7.0.** Прод-внешние интеграции
(Stripe Connect live, лицензии каталога, push-ключи) — ждут решений
владельца (Q11–Q15, B5): см. `IMPLEMENTATION_STATUS.md`.

## Не прод (по умолчанию)

- Stripe — MockStripe (live за `STRIPE_API_KEY`, но схема Connect открыта — реальные деньги НЕ проводятся);
- каталог — `HttpCatalog` пишет в iTunes/Deezer, но live-использование в прод ждёт лицензии B5; в e2e — `MockCatalog`;
- NATS/Redis/push — написаны, без env-переменных работают на noop/in-memory;
- секреты — dev-значения по умолчанию.
