#!/usr/bin/env bash
# deploy/prod/host/provision-host.sh — idempotent system setup for the PROD host.
#
# Runs ON the target host as root, fed in over stdin by deploy/prod/provision.sh
#   bash -s -- [--with-redis|--no-redis] [--skip-ufw] [--skip-swap]
#
# Expects the config bundle at $DROPIT_BUNDLE (default /root/dropit-deploy)
# with nginx/, systemd/, host/ subdirectories, and a 0600 secrets.env holding
#   DROPIT_DB_PASSWORD=...
# (written by provision.sh over the ssh channel; shredded here afterwards).
#
# Everything is guarded so re-running is safe:
#   * packages: apt only if something is missing;
#   * user/dirs: created only when absent;
#   * postgres: role/db created if missing, password always re-synced so the
#     DB and /srv/dropit/etc/dropit.env cannot drift;
#   * config files: delegated to apply-config.sh, which compares checksums;
#   * ufw: rules added before enabling, 22 always allowed first.
#
# Deliberately NOT touched: /etc/ssh/sshd_config (see deploy/servers/prod.md).

set -euo pipefail

DROPIT_BUNDLE=${DROPIT_BUNDLE:-/root/dropit-deploy}
SECRETS_FILE=$DROPIT_BUNDLE/secrets.env
APP_ROOT=/srv/dropit
APP_USER=dropit
APP_GROUP=dropit

WITH_REDIS=auto
SKIP_UFW=0
SKIP_SWAP=0
SYNC_ENV=${DROPIT_ENV_FORCE:-0}

for arg in "$@"; do
  case $arg in
    --with-redis) WITH_REDIS=yes ;;
    --no-redis)   WITH_REDIS=no ;;
    --skip-ufw)   SKIP_UFW=1 ;;
    --skip-swap)  SKIP_SWAP=1 ;;
    --sync-env)   SYNC_ENV=1 ;;
    *) echo "provision-host.sh: unknown argument '$arg'" >&2; exit 2 ;;
  esac
done

say()  { printf '\n=== %s\n' "$*"; }
info() { printf '    %s\n' "$*"; }
warn() { printf '  !! %s\n' "$*" >&2; }
die()  { printf '  xx %s\n' "$*" >&2; exit 1; }

[ "$(id -u)" = 0 ] || die "must run as root"
[ -d "$DROPIT_BUNDLE" ] || die "bundle not found: $DROPIT_BUNDLE"

cleanup() {
  if [ -f "$SECRETS_FILE" ]; then
    shred -u "$SECRETS_FILE" 2>/dev/null || rm -f "$SECRETS_FILE"
  fi
}
trap cleanup EXIT

# --- 0. secrets ------------------------------------------------------------
say "secrets"
[ -f "$SECRETS_FILE" ] || die "missing $SECRETS_FILE (provision.sh should have uploaded it)"
# shellcheck disable=SC1090
. "$SECRETS_FILE"
[ -n "${DROPIT_DB_PASSWORD:-}" ] || die "DROPIT_DB_PASSWORD is empty in $SECRETS_FILE"
info "secrets loaded (db password ${#DROPIT_DB_PASSWORD} chars); file will be shredded"

# --- 1. swap ---------------------------------------------------------------
# 956 MB RAM with Postgres + 2 Rust daemons: a swapfile turns a rare memory
# spike into latency instead of an OOM kill. Not a substitute for tuning.
if [ "$SKIP_SWAP" = 1 ]; then
  info "swap: skipped (--skip-swap)"
else
  say "swap"
  if swapon --show=NAME --noheadings 2>/dev/null | grep -qx '/swapfile'; then
    info "swapfile already active"
  elif [ -f /swapfile ]; then
    chmod 600 /swapfile
    swapon /swapfile
    info "activated existing /swapfile"
  else
    info "creating 1 GiB /swapfile"
    if ! fallocate -l 1G /swapfile 2>/dev/null; then
      dd if=/dev/zero of=/swapfile bs=1M count=1024 status=none
    fi
    chmod 600 /swapfile
    mkswap /swapfile >/dev/null
    swapon /swapfile
    info "created and activated"
  fi
  grep -qE '^/swapfile[[:space:]]' /etc/fstab || echo '/swapfile none swap sw 0 0' >> /etc/fstab
  cat >/etc/sysctl.d/99-dropit-memory.conf <<'EOF'
# DROP IT PROD (956 MB): prefer keeping page cache and Postgres resident,
# but still swap out genuinely cold pages under pressure.
vm.swappiness = 20
vm.vfs_cache_pressure = 50
EOF
  sysctl --system >/dev/null 2>&1 || warn "sysctl --system failed (non-fatal)"
  info "swappiness=20 persisted in /etc/sysctl.d/99-dropit-memory.conf"
fi

# --- 2. user + tree --------------------------------------------------------
say "service user and /srv/dropit tree"
if id -u "$APP_USER" >/dev/null 2>&1; then
  info "user $APP_USER exists"
else
  useradd --system --home-dir "$APP_ROOT" --shell /usr/sbin/nologin \
          --user-group --comment 'DROP IT service account' "$APP_USER"
  info "created system user $APP_USER"
fi

install -d -m 755 -o root      -g root      "$APP_ROOT"
install -d -m 755 -o root      -g "$APP_GROUP" "$APP_ROOT/bin"
install -d -m 755 -o root      -g "$APP_GROUP" "$APP_ROOT/web"
install -d -m 755 -o root      -g "$APP_GROUP" "$APP_ROOT/web/club"
install -d -m 755 -o root      -g "$APP_GROUP" "$APP_ROOT/web/admin"
install -d -m 755 -o root      -g "$APP_GROUP" "$APP_ROOT/web/dj-mock"
install -d -m 750 -o root      -g "$APP_GROUP" "$APP_ROOT/etc"
install -d -m 750 -o "$APP_USER" -g "$APP_GROUP" "$APP_ROOT/data"
install -d -m 750 -o "$APP_USER" -g "$APP_GROUP" "$APP_ROOT/data/migrations"
install -d -m 750 -o "$APP_USER" -g "$APP_GROUP" "$APP_ROOT/data/backup"
info "directories ready under $APP_ROOT"

# Placeholder shells so nginx serves something meaningful before the first
# deploy.sh run. deploy.sh rsyncs with --delete, so these disappear on deploy.
write_placeholder() {
  local dir=$1 label=$2
  if [ -f "$dir/index.html" ]; then
    info "$label: index.html already present, leaving it alone"
    return 0
  fi
  cat >"$dir/index.html" <<EOF
<!doctype html>
<html lang="en">
<meta charset="utf-8">
<meta name="robots" content="noindex">
<title>DROP IT — $label (not deployed yet)</title>
<style>
  body { font: 16px/1.5 ui-monospace, SFMono-Regular, Menlo, monospace;
         margin: 0; min-height: 100vh; display: grid; place-items: center;
         background: #0b0b0c; color: #e8e8ea; }
  main { max-width: 34rem; padding: 2rem; }
  h1 { font-size: 1.1rem; letter-spacing: .14em; text-transform: uppercase; margin: 0 0 1rem; }
  p  { margin: .35rem 0; color: #a8a8b0; }
  code { color: #e8e8ea; }
</style>
<main>
  <h1>DROP IT — $label</h1>
  <p>PROD host is provisioned, the application bundle has not been deployed yet.</p>
  <p>Run <code>deploy/prod/deploy.sh</code> from the repository.</p>
</main>
</html>
EOF
  chmod 644 "$dir/index.html"
  chown root:"$APP_GROUP" "$dir/index.html"
  info "$label: placeholder index.html written"
}
write_placeholder "$APP_ROOT/web/club"  "club panel"
write_placeholder "$APP_ROOT/web/admin" "admin panel"
write_placeholder "$APP_ROOT/web/dj-mock"    "DJ console"

# --- 3. packages -----------------------------------------------------------
say "packages"
PKGS="nginx postgresql postgresql-client curl rsync ufw ca-certificates"
command -v chronyc >/dev/null 2>&1 || PKGS="$PKGS chrony"
command -v jq     >/dev/null 2>&1 || PKGS="$PKGS jq"
if [ "$WITH_REDIS" = yes ]; then
  PKGS="$PKGS redis-server"
elif [ "$WITH_REDIS" = auto ] && ! dpkg -s redis-server >/dev/null 2>&1; then
  PKGS="$PKGS redis-server"
fi

missing=''
for p in $PKGS; do
  dpkg -s "$p" >/dev/null 2>&1 || missing="$missing $p"
done

if [ -z "$missing" ]; then
  info "all packages already installed"
else
  info "installing:$missing"
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -qq
  # shellcheck disable=SC2086
  apt-get install -y --no-install-recommends $missing
fi

systemctl enable --now postgresql >/dev/null 2>&1 || warn "could not enable postgresql"
systemctl enable --now nginx      >/dev/null 2>&1 || warn "could not enable nginx"
if [ "$WITH_REDIS" != no ] && dpkg -s redis-server >/dev/null 2>&1; then
  systemctl enable --now redis-server >/dev/null 2>&1 || warn "could not enable redis-server"
fi

# --- 4. postgres -----------------------------------------------------------
say "postgresql"
PGVER=$(pg_lsclusters -h 2>/dev/null | awk 'NR==1 {print $1}')
[ -n "$PGVER" ] || die "no PostgreSQL cluster found"
PGCLUSTER=$(pg_lsclusters -h | awk 'NR==1 {print $2}')
PGCONF_DIR=/etc/postgresql/$PGVER/$PGCLUSTER
[ -d "$PGCONF_DIR" ] || die "postgres config dir not found: $PGCONF_DIR"
info "cluster $PGVER/$PGCLUSTER at $PGCONF_DIR"

# Tuning for 956 MB total. shared_buffers 128 MB + max_connections 40 is the
# budget the task fixes; everything else stays proportional so the box keeps
# page cache for nginx/web assets and never swaps in steady state.
mkdir -p "$PGCONF_DIR/conf.d"
cat >"$PGCONF_DIR/conf.d/99-dropit.conf" <<'EOF'
# DROP IT PROD — generated by deploy/prod/host/provision-host.sh.
# Sized for a 956 MB / 1 vCPU VPS shared with nginx and two Rust daemons.
listen_addresses = 'localhost'      # never expose 5432 publicly
port = 5432
max_connections = 40                # API pool + workers + psql headroom
shared_buffers = 128MB
effective_cache_size = 320MB
work_mem = 3MB                      # x40 conns worst case = 120MB, still safe
maintenance_work_mem = 32MB
wal_buffers = 4MB
min_wal_size = 48MB
max_wal_size = 384MB
checkpoint_completion_target = 0.9
default_statistics_target = 100
random_page_cost = 1.1              # SSD-backed VPS
effective_io_concurrency = 100
max_worker_processes = 4
max_parallel_workers = 2
max_parallel_workers_per_gather = 1 # a single vCPU: parallel query hurts
huge_pages = off
log_min_duration_statement = 1000   # log slow queries (>1s)
log_checkpoints = on
log_lock_waits = on
log_temp_files = 0
log_line_prefix = '%m [%p] %q%u@%d '
EOF

changed=0
TUNING_SHA_FILE=/var/lib/dropit-pg-tuning.sha
new_sha=$(sha256sum "$PGCONF_DIR/conf.d/99-dropit.conf" | awk '{print $1}')
old_sha=$(cat "$TUNING_SHA_FILE" 2>/dev/null || true)
if [ "$new_sha" != "$old_sha" ]; then
  changed=1
fi
info "tuning written to $PGCONF_DIR/conf.d/99-dropit.conf"

if ! pg_isready -q 2>/dev/null; then
  changed=1
fi
if [ "$changed" = 1 ]; then
  info "restarting postgresql to apply tuning"
  systemctl restart postgresql
  for _ in $(seq 1 30); do
    if pg_isready -q 2>/dev/null; then break; fi
    sleep 1
  done
  printf '%s\n' "$new_sha" > "$TUNING_SHA_FILE"
  chmod 600 "$TUNING_SHA_FILE"
fi
pg_isready || die "postgres did not come back up"
info "postgres ready"

# Role + database. Password is always re-applied so that
# /srv/dropit/etc/dropit.env and the role cannot drift apart after a rotation.
runuser -u postgres -- psql -X -q -v ON_ERROR_STOP=1 -v pw="$DROPIT_DB_PASSWORD" <<'SQL'
SELECT format('CREATE ROLE %I LOGIN PASSWORD %L', 'dropit', :'pw')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'dropit')
\gexec
ALTER ROLE dropit WITH LOGIN PASSWORD :'pw';
SQL

runuser -u postgres -- psql -X -q -v ON_ERROR_STOP=1 <<'SQL'
SELECT 'CREATE DATABASE dropit OWNER dropit ENCODING ''UTF8'' TEMPLATE template0'
WHERE NOT EXISTS (SELECT 1 FROM pg_database WHERE datname = 'dropit')
\gexec
SQL

# Extensions need superuser, so they are created here rather than by the
# migrations (which run as `dropit`). The migrations' own
# `CREATE EXTENSION IF NOT EXISTS` is then a no-op notice for that role.
runuser -u postgres -- psql -X -q -v ON_ERROR_STOP=1 -d dropit <<'SQL'
CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS citext;
ALTER SCHEMA public OWNER TO dropit;
GRANT ALL ON SCHEMA public TO dropit;
ALTER DATABASE dropit SET timezone TO 'UTC';
SQL

info "role 'dropit' and database 'dropit' ready (extensions pgcrypto, citext)"

# Prove the app's own path works: role over TCP with scram, as it will run.
if PGPASSWORD="$DROPIT_DB_PASSWORD" psql -X -q -tA -h 127.0.0.1 -p 5432 \
     -U dropit -d dropit -c 'SELECT 1' >/dev/null 2>&1; then
  info "verified: dropit role reaches the DB over 127.0.0.1 (scram)"
else
  die "cannot connect as dropit over 127.0.0.1 — check pg_hba.conf"
fi

# --- 5. redis (optional) ---------------------------------------------------
say "redis"
if [ "$WITH_REDIS" = no ]; then
  info "redis disabled by request — REDIS_URL will be left empty"
elif dpkg -s redis-server >/dev/null 2>&1; then
  mkdir -p /etc/systemd/system/redis-server.service.d
  cat >/etc/systemd/system/redis-server.service.d/dropit-memory.conf <<'EOF'
# DROP IT PROD: bound redis so it cannot crowd out Postgres on a 956 MB box.
[Service]
MemoryAccounting=yes
MemoryMax=96M
EOF
  systemctl daemon-reload
  systemctl restart redis-server 2>/dev/null || systemctl restart redis 2>/dev/null || warn "redis restart failed"
  # Keep redis on loopback only; bind is the distro default but assert it.
  if grep -qE '^[[:space:]]*bind[[:space:]]' /etc/redis/redis.conf 2>/dev/null; then
    info "redis bind directive present in /etc/redis/redis.conf (left as is)"
  fi
  if redis-cli -h 127.0.0.1 ping 2>/dev/null | grep -q PONG; then
    info "redis responding on 127.0.0.1:6379 (MemoryMax=96M)"
  else
    warn "redis not responding; leaving REDIS_URL disabled"
    WITH_REDIS=no
  fi
else
  warn "redis-server not installed; REDIS_URL will be left empty"
  WITH_REDIS=no
fi

# --- 6. env file -----------------------------------------------------------
say "runtime environment"
ENV_DST=$APP_ROOT/etc/dropit.env
ENV_TEMPLATE=$DROPIT_BUNDLE/systemd/dropit.env.example
[ -f "$ENV_TEMPLATE" ] || die "missing env template: $ENV_TEMPLATE"

render_env() {
  local out=$1
  # Substitute the three secrets. `|` delimiter: secrets are alphanumeric.
  sed -e "s|__DB_PASSWORD__|$DROPIT_DB_PASSWORD|g" \
      -e "s|__JWT_SECRET__|${DROPIT_JWT_SECRET:-}|g" \
      -e "s|__TOTP_KEY__|${DROPIT_TOTP_KEY:-}|g" \
      "$ENV_TEMPLATE" > "$out"
  if [ "$WITH_REDIS" = no ]; then
    sed -i 's|^REDIS_URL=.*|REDIS_URL=|' "$out"
  fi
  # Only real assignments matter; the template's comments mention the token
  # names themselves and must not trip this guard.
  if grep -vE '^[[:space:]]*#' "$out" | grep -qE '__[A-Z_]+__'; then
    die "unsubstituted placeholder left in rendered env: $(grep -vE '^[[:space:]]*#' "$out" | grep -oE '__[A-Z_]+__' | sort -u | tr '\n' ' ')"
  fi
}

if [ -f "$ENV_DST" ] && [ "$SYNC_ENV" != 1 ]; then
  info "$ENV_DST already exists — keeping it (--sync-env to overwrite)"
else
  tmp=$(mktemp)
  render_env "$tmp"
  install -m 600 -o "$APP_USER" -g "$APP_GROUP" "$tmp" "$ENV_DST"
  rm -f "$tmp"
  info "rendered $ENV_DST (600 $APP_USER:$APP_GROUP)"
fi

# --- 7. nginx + units ------------------------------------------------------
say "nginx site and systemd units"
bash "$DROPIT_BUNDLE/host/apply-config.sh" --all

# --- 8. ufw ----------------------------------------------------------------
say "firewall"
if [ "$SKIP_UFW" = 1 ]; then
  info "ufw: skipped (--skip-ufw)"
else
  # Order matters: 22 is opened BEFORE enabling, otherwise the enable would
  # cut the SSH session that is running this very script.
  ufw --force default deny incoming  >/dev/null
  ufw --force default allow outgoing >/dev/null
  ufw allow 22/tcp  >/dev/null
  ufw allow 80/tcp  >/dev/null
  ufw allow 443/tcp >/dev/null
  info "rules: 22/tcp, 80/tcp, 443/tcp allowed; incoming denied by default"
  if ufw status 2>/dev/null | grep -q '^Status: active'; then
    info "ufw already active"
  else
    ufw --force enable >/dev/null
    info "ufw enabled"
  fi
  ufw status verbose | sed 's/^/    /'
fi

# --- 9. summary ------------------------------------------------------------
say "host summary"
info "OS:       $(. /etc/os-release && printf '%s' "$PRETTY_NAME") ($(uname -m))"
info "user:     $APP_USER ($(id -u "$APP_USER"))"
info "root:     $APP_ROOT"
info "env:      $ENV_DST ($(stat -c '%a %U:%G' "$ENV_DST"))"
info "nginx:    $(nginx -v 2>&1)"
info "postgres: $(runuser -u postgres -- psql -X -tA -c 'SHOW server_version' | head -1)"
info "redis:    $( [ "$WITH_REDIS" = no ] && echo 'disabled (in-memory fallback)' || echo "$(redis-cli -h 127.0.0.1 ping 2>/dev/null || echo down)" )"
info "swap:     $(swapon --show=NAME,SIZE --noheadings 2>/dev/null | tr '\n' ' ' || echo none)"
info "firewall: $(ufw status 2>/dev/null | head -1)"
info "units:    $(systemctl list-unit-files 'dropit-*' --no-legend 2>/dev/null | awk '{printf "%s(%s) ", $1, $2}')"

say "done"
info "next: deploy/prod/deploy.sh (from the repository) ships the binaries,"
info "      applies migrations and starts the services."
