# DROP IT — платформа заказа музыки в ночных клубах

Гость сканирует QR сессии → платит за запрос трека → DJ решает, играть ли.
Клуб и DJ получают доли; платформа — комиссию.

**Состояние: Test Mode (мок-Stripe, без внешних интеграций).**
Полная документация — `IMPLEMENTATION_STATUS.md`; план — `tehnicheskaya-karta.md`.

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

**80 тестов / clippy 0 / OpenAPI 0.7.0.** Прод-внешние интеграции
(Stripe Connect, музыкальный каталог) — ждут решений владельца
(Q11–Q15, B5): см. `IMPLEMENTATION_STATUS.md`.

## Не прод

- Stripe — мок (реальные деньги НЕ проводятся);
- каталог — 5 демо-треков (лицензии B5);
- секреты — dev-значения по умолчанию.
