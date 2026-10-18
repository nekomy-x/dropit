#!/usr/bin/env bash
# Сериализует БД-зависимые прогоны тестов между параллельными агентами.
#
# Все e2e-тесты используют одну локальную Postgres и чистят фикстуры
# (TRUNCATE), поэтому одновременный запуск двух прогонов даёт ложные падения.
# Скрипт берёт межпроцессный лок через mkdir и держит его до конца команды.
#
# Использование:
#   scripts/db-test.sh cargo test -p dropit-gateway-api e2e_panels -- --test-threads=1
set -euo pipefail

LOCK_DIR="${TMPDIR:-/tmp}/dropit-db-test.lock"

for _ in $(seq 1 900); do
    if mkdir "$LOCK_DIR" 2>/dev/null; then
        trap 'rm -rf "$LOCK_DIR"' EXIT INT TERM
        "$@"
        exit $?
    fi
    sleep 1
done

echo "db-test: не дождался лока $LOCK_DIR" >&2
exit 1
