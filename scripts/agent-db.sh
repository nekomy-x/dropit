#!/usr/bin/env bash
# Start/stop/status the local Postgres cluster used by e2e tests.
#
#   scripts/agent-db.sh start     # initdb (once) + pg_ctl start on :55433
#   scripts/agent-db.sh stop
#   scripts/agent-db.sh status
#
# The cluster is disposable: it lives in .pgdata/ (gitignored) and holds no
# important data. Tests TRUNCATE their fixtures themselves.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DATA="$REPO/.pgdata"
PORT="${DROPIT_PGPORT:-55433}"
PGBIN="/opt/homebrew/opt/postgresql@16/bin"
LOG="$DATA/server.log"

start() {
    if [ ! -f "$DATA/PG_VERSION" ]; then
        mkdir -p "$DATA"
        LC_ALL=C LANG=C "$PGBIN/initdb" -D "$DATA" -U dropit --auth=trust -E UTF8 --locale=C >/dev/null
    fi
    if "$PGBIN/pg_ctl" -D "$DATA" status >/dev/null 2>&1; then
        echo "postgres already running on :$PORT"
    else
        LC_ALL=C LANG=C "$PGBIN/pg_ctl" -D "$DATA" -o "-p $PORT -k /tmp -h 127.0.0.1" -l "$LOG" start
    fi
    "$PGBIN/psql" -h /tmp -p "$PORT" -U dropit -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname='dropit'" \
        | grep -q 1 || "$PGBIN/createdb" -h /tmp -p "$PORT" -U dropit dropit
    echo "DATABASE_URL=postgres://dropit@127.0.0.1:$PORT/dropit"
}

case "${1:-status}" in
    start) start ;;
    stop) "$PGBIN/pg_ctl" -D "$DATA" stop -m fast ;;
    status) "$PGBIN/pg_ctl" -D "$DATA" status ;;
    *) echo "usage: $0 start|stop|status" >&2; exit 2 ;;
esac
