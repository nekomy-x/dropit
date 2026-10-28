#!/usr/bin/env bash
# deploy/prod/host/migrate.sh — apply SQL migrations with psql.
#
# Runs ON the target host as root. This is the ONLY migration mechanism on
# PROD: the binaries cannot run them, because crates/infra-db/src/migrations.rs
# uses env!("CARGO_MANIFEST_DIR")/migrations at compile time, which does not
# exist on the server — DROPIT_RUN_MIGRATIONS=true there panics instead of
# migrating. (See deploy/servers/prod.md.)
#
# Behaviour:
#   * files are applied one per transaction, in C-collation filename order;
#   * every applied file is recorded in `schema_migrations(filename PK)`, so
#     re-running is a no-op for files already applied;
#   * psql runs with ON_ERROR_STOP=1 — a failing migration aborts that file,
#     rolls back its transaction, and stops the run;
#   * a short lock_timeout keeps the deploy from queueing behind long queries
#     on this single-vCPU box;
#   * a changed checksum on an already-applied file is reported as drift
#     (warn by default, fatal with --strict-drift).
#
# Auth: connects as the `dropit` DB role over the local unix socket, where
# pg_hba maps the `dropit` OS user to the `dropit` role via peer auth. No
# password is passed on a command line or written to disk by this script.
#
# Usage:
#   migrate.sh [--dir DIR] [--db NAME] [--role ROLE]
#              [--dry-run] [--status] [--strict-drift] [--quiet]

set -euo pipefail

MIG_DIR=${DROPIT_MIGRATIONS_DIR:-/srv/dropit/data/migrations}
DB_NAME=${DROPIT_DB_NAME:-dropit}
DB_ROLE=${DROPIT_DB_ROLE:-dropit}
PG_SOCKET_DIR=${DROPIT_PG_SOCKET_DIR:-/var/run/postgresql}
PG_PORT=${DROPIT_PG_PORT:-5432}

DRY_RUN=0
STATUS_ONLY=0
STRICT_DRIFT=0
QUIET=0

while [ $# -gt 0 ]; do
  case $1 in
    --dir)   MIG_DIR=$2; shift 2 ;;
    --db)    DB_NAME=$2; shift 2 ;;
    --role)  DB_ROLE=$2; shift 2 ;;
    --dry-run) DRY_RUN=1; shift ;;
    --status)  STATUS_ONLY=1; shift ;;
    --strict-drift) STRICT_DRIFT=1; shift ;;
    --quiet) QUIET=1; shift ;;
    -h|--help) sed -n '2,40p' "$0"; exit 0 ;;
    *) echo "migrate.sh: unknown argument '$1'" >&2; exit 2 ;;
  esac
done

PSQL_BASE=(psql -X -q -h "$PG_SOCKET_DIR" -p "$PG_PORT" -d "$DB_NAME")
# Keep a misbehaving migration from blocking the API for minutes.
export PGOPTIONS="${PGOPTIONS:--c lock_timeout=10s -c statement_timeout=300s}"

say()  { [ "$QUIET" = 1 ] || printf '  [migrate] %s\n' "$*"; }
warn() { printf '  [migrate] WARNING: %s\n' "$*" >&2; }
die()  { printf '  [migrate] ERROR: %s\n' "$*" >&2; exit 1; }

# --- sanity ----------------------------------------------------------------
[ -d "$MIG_DIR" ] || die "migrations directory not found: $MIG_DIR"
command -v runuser >/dev/null 2>&1 || die "runuser not found"
id -u "$DB_ROLE" >/dev/null 2>&1 || die "OS user '$DB_ROLE' does not exist (run provision.sh first)"

# Fail fast with an actionable message instead of a psql stack trace.
if ! runuser -u "$DB_ROLE" -- "${PSQL_BASE[@]}" -tAc 'SELECT 1' >/dev/null 2>&1; then
  die "cannot connect to database '$DB_NAME' as '$DB_ROLE' over $PG_SOCKET_DIR (peer auth). Is the cluster running and the role created?"
fi

sql_lit() { printf "'%s'" "$(printf '%s' "$1" | sed "s/'/''/g")"; }
psql_q()  { runuser -u "$DB_ROLE" -- "${PSQL_BASE[@]}" -tA -c "$1"; }

# --- bookkeeping table -----------------------------------------------------
runuser -u "$DB_ROLE" -- "${PSQL_BASE[@]}" -v ON_ERROR_STOP=1 >/dev/null <<'SQL'
CREATE TABLE IF NOT EXISTS schema_migrations (
    filename    text        PRIMARY KEY,
    checksum    text        NOT NULL,
    applied_at  timestamptz NOT NULL DEFAULT now(),
    source      text
);
COMMENT ON TABLE schema_migrations IS
    'Applied SQL migrations, maintained by deploy/prod/host/migrate.sh. Filename-ordered, one transaction each.';
SQL

# --- collect the migration list -------------------------------------------
FILES=()
while IFS= read -r f; do
  FILES+=("$f")
done < <(find "$MIG_DIR" -maxdepth 1 -type f -name '*.sql' | LC_ALL=C sort)

TOTAL=${#FILES[@]}
[ "$TOTAL" -gt 0 ] || die "no *.sql files in $MIG_DIR — nothing to apply (refusing to pretend success)"

# bash 3.2 has no associative arrays; use a temp lookup file.
APPLIED_LIST=$(mktemp)
trap 'rm -f "$APPLIED_LIST"' EXIT
psql_q "SELECT filename || '|' || checksum FROM schema_migrations" > "$APPLIED_LIST"

lookup_applied() {
  # prints "<checksum>" for an applied file, empty otherwise
  awk -F'|' -v k="$1" '$1 == k { print $2; exit }' "$APPLIED_LIST"
}

say "database=$DB_NAME role=$DB_ROLE dir=$MIG_DIR files=$TOTAL"

# --- status mode -----------------------------------------------------------
if [ "$STATUS_ONLY" = 1 ]; then
  applied_n=0
  pending_n=0
  for f in "${FILES[@]}"; do
    name=$(basename "$f")
    if [ -n "$(lookup_applied "$name")" ]; then
      applied_n=$((applied_n + 1))
      printf '  applied  %s\n' "$name"
    else
      pending_n=$((pending_n + 1))
      printf '  PENDING  %s\n' "$name"
    fi
  done
  printf '  [migrate] applied=%s pending=%s\n' "$applied_n" "$pending_n"
  exit 0
fi

# --- apply -----------------------------------------------------------------
applied=0
skipped=0
drift=0
pending_names=''

for f in "${FILES[@]}"; do
  name=$(basename "$f")
  sum=$(sha256sum "$f" | awk '{print $1}')
  prev=$(lookup_applied "$name")

  if [ -n "$prev" ]; then
    skipped=$((skipped + 1))
    if [ "$prev" != "$sum" ]; then
      drift=$((drift + 1))
      warn "$name was already applied but its checksum changed (db=$prev build=$sum)."
      if [ "$STRICT_DRIFT" = 1 ]; then
        die "migration drift detected and --strict-drift is set"
      fi
      warn "  leaving it in place — write a new migration instead of editing an applied one"
    fi
    continue
  fi

  pending_names="$pending_names $name"

  if [ "$DRY_RUN" = 1 ]; then
    say "would apply $name"
    continue
  fi

  say "applying $name"
  # One transaction per file: the DDL and its bookkeeping row commit together,
  # so an interrupted run can never mark a half-applied file as done.
  {
    printf 'BEGIN;\n'
    cat "$f"
    printf '\nINSERT INTO schema_migrations (filename, checksum, source)\n'
    printf 'VALUES (%s, %s, %s)\n' "$(sql_lit "$name")" "$(sql_lit "$sum")" "$(sql_lit 'deploy.sh')"
    printf 'ON CONFLICT (filename) DO NOTHING;\n'
    printf 'COMMIT;\n'
  } | runuser -u "$DB_ROLE" -- "${PSQL_BASE[@]}" -v ON_ERROR_STOP=1 >/dev/null \
    || die "migration $name failed (transaction rolled back); fix the SQL and re-run"

  applied=$((applied + 1))
done

if [ "$DRY_RUN" = 1 ]; then
  printf '  [migrate] applied=0 skipped=%s drift=%s pending:%s\n' "$skipped" "$drift" "${pending_names:- none}"
else
  printf '  [migrate] applied=%s skipped=%s drift=%s total=%s\n' "$applied" "$skipped" "$drift" "$TOTAL"
fi

[ "$drift" = 0 ] || printf '  [migrate] checksum drift on %s file(s) — review before the next release\n' "$drift"
exit 0
