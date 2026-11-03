#!/usr/bin/env bash
# Developer API v1 — смоук на живом бинарнике (ADR-010).
#
# Проверяет не юнит-логику, а собранную систему целиком:
#   панельный JWT → создание ключа → Developer API по ключу →
#   скоупы (403 достаёт только своих) → фильтрация OpenAPI →
#   отзыв (401) → транспорт X-API-Key → неизвестный ключ.
#
# Требования: собранный `target/debug/dropit-api`, локальная Postgres
# с применёнными миграциями, `jq`, `curl`.
#
#   source scripts/agent-env.sh
#   ./scripts/developer-api-smoke.sh
#
# Переменные: DROPIT_SMOKE_PORT (18080), DROPIT_SMOKE_SEED=0 — не сидить
# демо-данные, DROPIT_SMOKE_KEEP=1 — не гасить сервер после прогона.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${DROPIT_SMOKE_PORT:-18080}"
BASE="http://127.0.0.1:${PORT}"
API_BIN="${REPO}/target/debug/dropit-api"
LOG="$(mktemp -t dropit-smoke-api.XXXXXX.log)"
SERVER_PID=""

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

cleanup() {
    if [[ -n "${SERVER_PID}" && "${DROPIT_SMOKE_KEEP:-0}" != "1" ]]; then
        kill "${SERVER_PID}" 2>/dev/null || true
        wait "${SERVER_PID}" 2>/dev/null || true
    fi
}
trap cleanup EXIT

http() { # method path token data -> body (последняя строка — код в HTTP_CODE)
    local method="$1" path="$2" token="${3:-}" data="${4:-}"
    local args=(-sS -X "${method}" -o /tmp/dropit-smoke-body.json -w '%{http_code}')
    [[ -n "${token}" ]] && args+=(-H "Authorization: Bearer ${token}")
    [[ -n "${data}" ]] && args+=(-H 'Content-Type: application/json' -d "${data}")
    HTTP_CODE="$(curl "${args[@]}" "${BASE}${path}")"
    BODY="$(cat /tmp/dropit-smoke-body.json)"
}

expect_code() { # expected label
    local expected="$1" label="$2"
    [[ "${HTTP_CODE}" == "${expected}" ]] || fail "${label}: ожидался ${expected}, получен ${HTTP_CODE}: ${BODY}"
    echo "  ok  ${label} (${HTTP_CODE})"
}

[[ -x "${API_BIN}" ]] || fail "нет бинарника ${API_BIN} (cargo build -p dropit-gateway-api --bin dropit-api)"
[[ -n "${DATABASE_URL:-}" ]] || fail "DATABASE_URL не задан (source scripts/agent-env.sh)"

# --- Демо-данные ---------------------------------------------------------
if [[ "${DROPIT_SMOKE_SEED:-1}" == "1" ]]; then
    ( cd "${REPO}" && cargo run -q -p dropit-gateway-api --bin seed_demo >/dev/null )
fi

# --- Сервер --------------------------------------------------------------
DROPIT_OPEN_SIGNUP=1 DROPIT_BIND="127.0.0.1:${PORT}" \
    DROPIT_JWT_SECRET="smoke-secret-key-32-bytes-xxxxx!!" \
    "${API_BIN}" >"${LOG}" 2>&1 &
SERVER_PID=$!
for _ in $(seq 1 100); do
    if curl -fsS "${BASE}/healthz" >/dev/null 2>&1; then break; fi
    sleep 0.1
done
curl -fsS "${BASE}/healthz" >/dev/null || { tail -30 "${LOG}"; fail "API не поднялся на ${BASE}"; }

# --- Панельный вход демо-клубом ------------------------------------------
http POST /v1/auth/club/login "" '{"email":"demo@club.test","password":"demo-club-pass"}'
expect_code 200 "логин демо-клуба"
CLUB_TOKEN="$(jq -r '.access_token // empty' <<<"${BODY}")"
[[ -n "${CLUB_TOKEN}" ]] || fail "логин не вернул access_token: ${BODY}"

# --- Создание ключа с полным набором скоупов ------------------------------
http POST /v1/club/api-keys "${CLUB_TOKEN}" '{
  "name": "smoke-full",
  "scopes": ["docs:read","profile:read","sessions:read","sessions:write","qr:write","requests:read","requests:write","catalog:read","stats:read"],
  "environment": "test",
  "expires_in_days": 1,
  "rate_limit_per_min": 120
}'
expect_code 201 "создание ключа"
FULL_KEY="$(jq -r '.key // empty' <<<"${BODY}")"
KEY_PREFIX="$(jq -r '.prefix // empty' <<<"${BODY}")"
[[ "${FULL_KEY}" == dit_test_* ]] || fail "ключ не соответствует формату dit_test_…: ${FULL_KEY}"
grep -q '"secret_hash"' <<<"${BODY}" && fail "ответ содержит secret_hash"
LIST_BODY="$(curl -sS -H "Authorization: Bearer ${CLUB_TOKEN}" "${BASE}/v1/club/api-keys")"
grep -q "${FULL_KEY}" <<<"${LIST_BODY}" && fail "список ключей раскрывает plaintext-ключ"
grep -qi '"secret_hash"' <<<"${LIST_BODY}" && fail "список ключей раскрывает secret_hash"

# --- Developer API: идентичность, данные ---------------------------------
http GET /v1/developer/me "${FULL_KEY}"
expect_code 200 "/me"
[[ "$(jq -r '.prefix' <<<"${BODY}")" == "${KEY_PREFIX}" ]] || fail "/me вернул чужой prefix"

http GET /v1/developer/profile "${FULL_KEY}"
expect_code 200 "/profile"
http GET /v1/developer/sessions "${FULL_KEY}"
expect_code 200 "/sessions"
http GET /v1/developer/stats/overview "${FULL_KEY}"
expect_code 200 "/stats/overview"
http GET /v1/developer/docs "${FULL_KEY}"
expect_code 200 "/docs"
[[ "$(jq '.sections | length' <<<"${BODY}")" -gt 0 ]] || fail "/docs вернул пустой список секций"

# --- Фильтрация OpenAPI по скоупам ---------------------------------------
http GET /v1/developer/openapi.json "${FULL_KEY}"
expect_code 200 "/openapi.json"
jq -e '.paths | has("/v1/developer/profile")' <<<"${BODY}" >/dev/null \
    || fail "полный ключ не видит /profile в своём OpenAPI"

# --- Ключ только с документацией: никаких данных -------------------------
http POST /v1/club/api-keys "${CLUB_TOKEN}" '{"name":"smoke-docs","scopes":["docs:read"]}'
expect_code 201 "создание docs-only ключа"
DOCS_KEY="$(jq -r '.key' <<<"${BODY}")"
DOCS_ID="$(jq -r '.id' <<<"${BODY}")"

http GET /v1/developer/docs "${DOCS_KEY}"
expect_code 200 "docs-only: /docs"
http GET /v1/developer/profile "${DOCS_KEY}"
expect_code 403 "docs-only: /profile → 403"
[[ "$(jq -r '.code' <<<"${BODY}")" == "insufficient_scope" ]] || fail "ожидался код insufficient_scope"
http GET /v1/developer/stats/overview "${DOCS_KEY}"
expect_code 403 "docs-only: /stats → 403"

http GET /v1/developer/openapi.json "${DOCS_KEY}"
expect_code 200 "docs-only: /openapi.json"
jq -e '.paths | has("/v1/developer/profile") | not' <<<"${BODY}" >/dev/null \
    || fail "docs-only ключ получил в OpenAPI чужие маршруты"

# --- Транспорт X-API-Key -------------------------------------------------
HTTP_CODE="$(curl -sS -o /tmp/dropit-smoke-body.json -w '%{http_code}' \
    -H "X-API-Key: ${DOCS_KEY}" "${BASE}/v1/developer/me")"
BODY="$(cat /tmp/dropit-smoke-body.json)"
expect_code 200 "X-API-Key транспорт"

# --- Отзыв и неизвестный ключ -------------------------------------------
http DELETE "/v1/club/api-keys/${DOCS_ID}" "${CLUB_TOKEN}"
expect_code 204 "отзыв ключа"
http GET /v1/developer/me "${DOCS_KEY}"
expect_code 401 "отозванный ключ → 401"
[[ "$(jq -r '.code' <<<"${BODY}")" == "api_key_revoked" ]] || fail "ожидался код api_key_revoked"

http GET /v1/developer/me "dit_test_aaaaaaaaaa_$(printf 'a%.0s' {1..64})"
expect_code 401 "неизвестный ключ → 401"
[[ "$(jq -r '.code' <<<"${BODY}")" == "invalid_api_key" ]] || fail "ожидался код invalid_api_key"

echo
echo "SMOKE GREEN: Developer API v1 + scoped keys работают на живом бинарнике."