#!/usr/bin/env bash
# deploy/prod/provision.sh — bring the PROD host from bare Ubuntu to
# "ready for the application bundle".
#
# Runs on YOUR machine (the repository checkout); every system change happens
# on the target over ssh. Use deploy.sh afterwards to ship binaries + web
# bundles and start the services.
#
#   ./deploy/prod/provision.sh                 # full, idempotent
#   ./deploy/prod/provision.sh --verify-only    # re-run acceptance checks only
#   ./deploy/prod/provision.sh --no-redis       # leave REDIS_URL empty
#   ./deploy/prod/provision.sh --sync-env       # re-render /srv/dropit/etc/dropit.env
#
# What it does:
#   1. packages (nginx, postgresql, redis, curl, rsync, ufw, chrony)
#   2. swap + vm.swappiness (956 MB box)
#   3. system user `dropit`, /srv/dropit/{bin,web,etc,data} tree
#   4. PostgreSQL: localhost-only, tuned for 956 MB, role+db `dropit`,
#      extensions pgcrypto/citext, password synced with the env file
#   5. nginx site + systemd units installed (enable, not start)
#   6. /srv/dropit/etc/dropit.env rendered 0600 dropit:dropit
#   7. ufw: 22/80/443 allowed, then enabled
#   8. acceptance checks
#
# Secrets: read from deploy/prod/.env.local (gitignored). Missing values are
# generated here and written back. The root SSH password is used only through
# an askpass helper; it is never written to the host.
#
# It does NOT deploy application binaries and does NOT run migrations.

set -euo pipefail

# shellcheck source=./lib.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

VERIFY_ONLY=0
REDIS_MODE=auto
SKIP_UFW=0
SKIP_SWAP=0
SYNC_ENV=0

usage() {
  sed -n '2,32p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
  exit 0
}

while [ $# -gt 0 ]; do
  case $1 in
    --host)        PROD_HOST=$2; shift 2 ;;
    --with-redis)  REDIS_MODE=yes; shift ;;
    --no-redis)    REDIS_MODE=no; shift ;;
    --skip-ufw)    SKIP_UFW=1; shift ;;
    --skip-swap)   SKIP_SWAP=1; shift ;;
    --sync-env)    SYNC_ENV=1; shift ;;
    --verify-only) VERIFY_ONLY=1; shift ;;
    -h|--help)     usage ;;
    *) dc_die "provision.sh: unknown argument '$1' (try --help)" ;;
  esac
done

dc_require_tools ssh rsync openssl
dc_load_local_env
TARGET=$(dc_target_host)

# --- secrets ---------------------------------------------------------------
# .env.local is the only place the root password and generated secrets live.
# Anything missing is generated once and persisted so re-runs are stable.
prepare_local_env() {
  local file=${DC_LOCAL_ENV_FILE:-${DROPIT_DEPLOY_ENV:-$DC_PROD_DIR/.env.local}}

  if [ ! -f "$file" ]; then
    if [ -f "$DC_PROD_DIR/.env.example" ]; then
      cp "$DC_PROD_DIR/.env.example" "$file"
      chmod 600 "$file"
      dc_info "created deploy/prod/.env.local from .env.example"
    else
      dc_die "deploy/prod/.env.example is missing"
    fi
  fi
  chmod 600 "$file"

  local missing=''
  local var
  for var in DROPIT_DB_PASSWORD DROPIT_JWT_SECRET DROPIT_TOTP_KEY; do
    eval "local cur=\${$var:-}"
    if [ -z "$cur" ]; then
      missing="$missing $var"
    fi
  done

  if [ -n "$missing" ]; then
    dc_log "generating missing secrets:$missing"
    for var in $missing; do
      local val
      case $var in
        DROPIT_DB_PASSWORD) val=$(dc_gen_alnum 40) ;;
        DROPIT_JWT_SECRET)  val=$(dc_gen_hex 48) ;;
        DROPIT_TOTP_KEY)    val=$(dc_gen_hex 32) ;;
      esac
      env_local_set "$var" "$val" "$file"
      eval "export $var=\"\$val\""
    done
    dc_info "written to deploy/prod/.env.local (chmod 600, gitignored)"
  fi

  # Re-read so exported values always match the file.
  dc_load_local_env "$file"
}

# Idempotently set KEY=value in a KEY=value file (works on macOS awk).
env_local_set() {
  local key=$1 val=$2 file=$3
  awk -v k="$key" -v v="$val" '
    BEGIN { done = 0 }
    index($0, k "=") == 1 { print k "=" v; done = 1; next }
    { print }
    END { if (!done) print k "=" v }
  ' "$file" > "$file.tmp.$$"
  mv "$file.tmp.$$" "$file"
  chmod 600 "$file"
}

if [ "$VERIFY_ONLY" = 0 ]; then
  prepare_local_env
  [ -n "${DROPIT_DB_PASSWORD:-}" ] || dc_die "DROPIT_DB_PASSWORD is empty after generation"
  [ -n "${DROPIT_JWT_SECRET:-}" ]  || dc_die "DROPIT_JWT_SECRET is empty after generation"
  [ -n "${DROPIT_TOTP_KEY:-}" ]    || dc_die "DROPIT_TOTP_KEY is empty after generation"

  if [ -z "${DROPIT_SSH_PASS:-}" ]; then
    dc_warn "DROPIT_SSH_PASS is empty in .env.local — relying on an SSH key / ssh-agent"
  fi
fi

# --- bundle upload ---------------------------------------------------------
# Only configuration travels in the bundle. Binaries and web assets are
# deploy.sh's job; migrations are uploaded there too.
upload_bundle() {
  local host=$1
  local tmp
  tmp=$(mktemp -d "${TMPDIR:-/tmp}/dropit-bundle.XXXXXX")

  mkdir -p "$tmp/nginx" "$tmp/systemd" "$tmp/host"
  cp "$DC_PROD_DIR/nginx/dropit.conf"                        "$tmp/nginx/"
  cp "$DC_PROD_DIR/systemd/dropit-api.service"               "$tmp/systemd/"
  cp "$DC_PROD_DIR/systemd/dropit-workers.service"           "$tmp/systemd/"
  cp "$DC_PROD_DIR/systemd/dropit.env.example"               "$tmp/systemd/"
  cp "$DC_PROD_DIR/host/apply-config.sh"                     "$tmp/host/"
  cp "$DC_PROD_DIR/host/migrate.sh"                          "$tmp/host/"
  cp "$DC_PROD_DIR/host/bootstrap-test-club.py"               "$tmp/host/"
  cp "$DC_PROD_DIR/host/provision-host.sh"                   "$tmp/host/"
  chmod 755 "$tmp/host/"*.sh

  dc_log "uploading config bundle to $host:/root/dropit-deploy"
  dc_ssh "$host" 'mkdir -p /root/dropit-deploy' >/dev/null
  dc_rsync "$host" /root/dropit-deploy/ -a --delete \
    --exclude 'backup/' --exclude 'state/' --exclude 'secrets.env' \
    "$tmp/" >/dev/null
  dc_ssh "$host" 'chmod 755 /root/dropit-deploy/host/*.sh' >/dev/null
  rm -rf "$tmp"
  dc_info "bundle: nginx/dropit.conf, systemd/*.service, systemd/dropit.env.example, host/*.sh"
}

# Secrets go over the ssh channel on stdin, never in argv, and are shredded on
# the host by provision-host.sh once the env file is rendered.
upload_secrets() {
  local host=$1
  local tmp
  tmp=$(mktemp "${TMPDIR:-/tmp}/dropit-secrets.XXXXXX")
  chmod 600 "$tmp"
  # Single-quoted values: the alphabets we generate are shell-safe.
  {
    printf "DROPIT_DB_PASSWORD='%s'\n" "$DROPIT_DB_PASSWORD"
    printf "DROPIT_JWT_SECRET='%s'\n"  "$DROPIT_JWT_SECRET"
    printf "DROPIT_TOTP_KEY='%s'\n"    "$DROPIT_TOTP_KEY"
  } > "$tmp"

  dc_log "transferring secrets to $host (stdin, 0600)"
  dc_ssh_stdin "$host" 'umask 077; cat > /root/dropit-deploy/secrets.env' < "$tmp"
  rm -f "$tmp"
  dc_info "secrets.env staged; it is shredded at the end of provisioning"
}

# --- acceptance checks -----------------------------------------------------
remote_verify() {
  local host=$1
  dc_step "acceptance checks"
  # dc_ssh_stdin (not dc_ssh) so the heredoc reaches the remote bash.
  dc_ssh_stdin "$host" 'bash -s' <<'REMOTE'
set -u
fail=0
checks=0
ok()   { printf '  ok    %s\n' "$*"; checks=$((checks + 1)); }
bad()  { printf '  FAIL  %s\n' "$*"; fail=$((fail + 1)); checks=$((checks + 1)); }

if nginx -t 2>/dev/null; then ok "nginx -t"; else bad "nginx -t"; fi

for u in dropit-api.service dropit-workers.service; do
  if [ -f "/etc/systemd/system/$u" ]; then ok "unit installed: $u"; else bad "unit missing: $u"; fi
  if systemctl is-enabled "$u" >/dev/null 2>&1; then ok "unit enabled: $u"; else bad "unit not enabled: $u"; fi
done

if [ -f /srv/dropit/etc/dropit.env ]; then
  perm=$(stat -c '%a %U:%G' /srv/dropit/etc/dropit.env)
  [ "$perm" = "600 dropit:dropit" ] && ok "env file $perm" || bad "env file perms: $perm"
else
  bad "/srv/dropit/etc/dropit.env missing"
fi

for d in bin web etc data web/club web/admin web/dj-mock data/migrations; do
  [ -d "/srv/dropit/$d" ] && ok "dir /srv/dropit/$d" || bad "dir missing: /srv/dropit/$d"
done

if id -u dropit >/dev/null 2>&1; then ok "user dropit exists"; else bad "user dropit missing"; fi

if runuser -u dropit -- psql -X -q -h /var/run/postgresql -d dropit -tAc 'SELECT 1' >/dev/null 2>&1; then
  ok "postgres: dropit role reaches db dropit (peer)"
else
  bad "postgres: dropit role cannot reach db dropit"
fi

for ext in pgcrypto citext; do
  n=$(runuser -u postgres -- psql -X -tA -d dropit -c "SELECT count(*) FROM pg_extension WHERE extname='$ext'" 2>/dev/null)
  [ "$n" = 1 ] && ok "extension $ext" || bad "extension $ext missing"
done

code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 http://127.0.0.1/ || echo 000)
case $code in
  200|301|302|404) ok "nginx answers on :80 (HTTP $code)" ;;
  000) bad "nginx not answering on :80 (connection refused)" ;;
  *)  bad "nginx answered :80 with HTTP $code" ;;
esac

if ufw status 2>/dev/null | grep -q '^Status: active'; then
  rule22=$(ufw status | grep -cE '^22/tcp' || true)
  [ "$rule22" -ge 1 ] && ok "ufw active, 22/tcp allowed" || bad "ufw active but 22/tcp rule missing"
else
  bad "ufw is not active"
fi

code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 http://127.0.0.1/healthz || echo 000)
if [ "$code" = "200" ]; then
  ok "api /healthz through nginx (HTTP 200)"
else
  printf '  note  api /healthz not serving yet (HTTP %s) — expected until deploy.sh runs\n' "$code"
fi

if [ "$checks" -lt 10 ]; then
  printf '  FAIL  remote check script ran only %s checks — expected >= 10\n' "$checks"
  fail=$((fail + 1))
fi

exit $fail
REMOTE
}

# --- main ------------------------------------------------------------------
if [ "$VERIFY_ONLY" = 1 ]; then
  dc_step "verify only"
  remote_verify "$TARGET" && dc_log "all checks passed" || dc_die "some checks failed (see above)"
  exit 0
fi

dc_step "provisioning $TARGET"
dc_info "redis mode: $REDIS_MODE (auto = install/keep if the package works)"

upload_bundle "$TARGET"
upload_secrets "$TARGET"

prov_args=''
case $REDIS_MODE in
  yes) prov_args='--with-redis' ;;
  no)  prov_args='--no-redis' ;;
esac
if [ "$SKIP_UFW" = 1 ]; then
  prov_args="$prov_args --skip-ufw"
fi
if [ "$SKIP_SWAP" = 1 ]; then
  prov_args="$prov_args --skip-swap"
fi
if [ "$SYNC_ENV" = 1 ]; then
  prov_args="$prov_args --sync-env"
fi

dc_log "running host provisioning (this installs packages and restarts services)"
# shellcheck disable=SC2086
dc_ssh_script "$TARGET" "$DC_PROD_DIR/host/provision-host.sh" $prov_args

remote_verify "$TARGET" && dc_log "all checks passed" || dc_die "provisioning finished but some checks failed (see above)"

dc_step "done"
dc_info "host is ready for the application bundle"
dc_info "next: ./deploy/prod/deploy.sh   (rsync binaries + web, migrate, restart, health check)"
