#!/usr/bin/env bash
# deploy/prod/deploy.sh — ship the built application to the PROD host.
#
#   ./deploy/prod/deploy.sh                  # full deploy
#   ./deploy/prod/deploy.sh --dry-run        # show what would change, touch nothing
#   ./deploy/prod/deploy.sh --skip-migrate   # binaries+web only (no psql step)
#   ./deploy/prod/deploy.sh --no-restart     # upload and install, leave services running
#   ./deploy/prod/deploy.sh --verify-only    # health checks against the live host
#   ./deploy/prod/deploy.sh --build-dir DIR  # non-default artifact location
#
# Input: a local `build/` directory (gitignored) produced by the Lead's
# cross-compile step — see deploy/servers/prod.md, "Build":
#
#   build/dropit-api            static x86_64-musl binary
#   build/dropit-workers        static x86_64-musl binary
#   build/web/club/...          Vite dist of apps/web-club   (base '/')
#   build/web/admin/...         Vite dist of apps/web-admin  (base '/admin/')
#   build/web/dj/index.html     static DJ console prototype (served at /dj-mock/)
#   build/migrations/*.sql      crates/infra-db/migrations
#
# Flow: preflight -> upload artifacts -> install config -> migrate (psql) ->
#       restart -> health check. Idempotent: re-running with the same build
#       changes nothing and restarts nothing (change detection by checksum).
#
# Migrations are applied with psql by host/migrate.sh, NOT by the binaries:
# crates/infra-db bakes env!("CARGO_MANIFEST_DIR")/migrations at compile time,
# so DROPIT_RUN_MIGRATIONS=true panics on the host. It stays false in
# /srv/dropit/etc/dropit.env.
#
# Secrets: deploy/prod/.env.local (gitignored). Nothing secret is ever placed
# in this repo or in a git-tracked file.

set -euo pipefail

# shellcheck source=./lib.sh
. "$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

BUILD_DIR=$DC_REPO_ROOT/build
DRY_RUN=0
SKIP_MIGRATE=0
NO_RESTART=0
VERIFY_ONLY=0
SKIP_CONFIG=0

usage() { sed -n '2,30p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0; }

while [ $# -gt 0 ]; do
  case $1 in
    --build-dir)    BUILD_DIR=$2; shift 2 ;;
    --host)         PROD_HOST=$2; shift 2 ;;
    --dry-run)      DRY_RUN=1; shift ;;
    --skip-migrate) SKIP_MIGRATE=1; shift ;;
    --skip-config)  SKIP_CONFIG=1; shift ;;
    --no-restart)   NO_RESTART=1; shift ;;
    --verify-only)  VERIFY_ONLY=1; shift ;;
    -h|--help)      usage ;;
    *) dc_die "deploy.sh: unknown argument '$1' (try --help)" ;;
  esac
done

dc_require_tools ssh rsync
dc_load_local_env
TARGET=$(dc_target_host)

# --- health checks ---------------------------------------------------------
# Run against the host itself (loopback) so a local firewall or DNS problem on
# the workstation cannot mask a healthy deployment.
health_check() {
  local host=$1
  dc_step "health check"
  # dc_ssh_stdin, not dc_ssh: dc_ssh closes stdin (< /dev/null) and the
  # heredoc script below would never reach the remote bash.
  dc_ssh_stdin "$host" 'bash -s' <<'REMOTE'
set -u
fail=0
checks=0
ok()  { printf '  ok    %s\n' "$*"; checks=$((checks + 1)); }
bad() { printf '  FAIL  %s\n' "$*"; fail=$((fail + 1)); checks=$((checks + 1)); }
skip(){ printf '  skip  %s\n' "$*"; }

# 1. local API directly on the loopback port
code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 http://127.0.0.1:8080/healthz 2>/dev/null); code=${code:-000}
if [ "$code" = 200 ]; then ok "api :8080/healthz -> 200"; else bad "api :8080/healthz -> $code"; fi

code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 http://127.0.0.1:8080/readyz 2>/dev/null); code=${code:-000}
if [ "$code" = 200 ]; then ok "api :8080/readyz -> 200"; else bad "api :8080/readyz -> $code"; fi

# 2. same endpoints through nginx (the public path)
code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 http://127.0.0.1/healthz 2>/dev/null); code=${code:-000}
if [ "$code" = 200 ]; then ok "nginx /healthz -> 200"; else bad "nginx /healthz -> $code"; fi

# The document the public /status/ page renders. Unlike /readyz (process-level),
# this route reads the database, so it is the deploy check that reflects the
# state a visitor actually sees.
body=$(curl -s --max-time 5 http://127.0.0.1/v1/status/services 2>/dev/null || true)
# Field order in the JSON is not guaranteed, so test the two keys independently.
if printf '%s' "$body" | grep -q '"status"' && printf '%s' "$body" | grep -q '"services"'; then
  ok "/v1/status/services -> status document"
else
  bad "/v1/status/services: no status document (API/DB?)"
fi

# 3. the three front-ends
for path in / /admin/ /dj /dj/login /dj-mock/ /status/; do
  code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 "http://127.0.0.1$path" 2>/dev/null); code=${code:-000}
  case $code in
    200) ok "nginx $path -> 200" ;;
    *)   bad "nginx $path -> $code" ;;
  esac
done

# 4. an unauthenticated API route must answer JSON, not an nginx error page
body=$(curl -s --max-time 5 http://127.0.0.1/v1/auth/club/login -X POST \
        -H 'Content-Type: application/json' -d '{}' 2>/dev/null || true)
case $body in
  *'"code"'*|*'"error"'*|*'"message"'*) ok "/v1/ proxied to the API (JSON error envelope)" ;;
  '')  bad "/v1/ returned nothing" ;;
  *)   printf '  note  /v1/ answered: %s\n' "$(printf '%s' "$body" | tr -s '\n' ' ' | cut -c1-120)" ;;
esac

# 5. services are up
for u in dropit-api dropit-workers; do
  if systemctl is-active --quiet "$u"; then
    ok "$u active"
  else
    state=$(systemctl is-active "$u" 2>/dev/null); state=${state:-unknown}
    bad "$u is $state"
    printf '        journal: journalctl -u %s -n 20 --no-pager\n' "$u"
  fi
done

# 6. postgres still reachable as the app role
if runuser -u dropit -- psql -X -q -h /var/run/postgresql -d dropit -tAc 'SELECT 1' >/dev/null 2>&1; then
  ok "postgres reachable as dropit"
else
  bad "postgres not reachable as dropit"
fi

# A script that never ran (empty stdin, wrong shell) must not look like success.
if [ "$checks" -lt 8 ]; then
  printf '  FAIL  remote check script ran only %s checks — expected >= 8\n' "$checks"
  fail=$((fail + 1))
fi

exit $fail
REMOTE
}

# --- preflight -------------------------------------------------------------
preflight() {
  local dir=$1
  dc_step "preflight"
  local errs=0

  for f in dropit-api dropit-workers; do
    if [ ! -f "$dir/$f" ]; then
      dc_err "missing $dir/$f"
      errs=$((errs + 1))
    elif [ ! -s "$dir/$f" ]; then
      dc_err "$dir/$f is empty"
      errs=$((errs + 1))
    fi
  done

  for d in web/club web/admin; do
    if [ ! -f "$dir/$d/index.html" ]; then
      dc_err "missing $dir/$d/index.html (Vite dist not built?)"
      errs=$((errs + 1))
    fi
  done

  if [ ! -f "$dir/web/status/index.html" ]; then
    dc_err "missing $dir/web/status/index.html (public status page)"
    errs=$((errs + 1))
  fi
  if [ ! -f "$dir/web/dj/index.html" ]; then
    dc_err "missing $dir/web/dj/index.html"
    errs=$((errs + 1))
  fi

  mig_n=$(dc_count_migrations "$dir/migrations")
  if [ "$mig_n" = 0 ]; then
    dc_warn "no migrations in $dir/migrations — the database will stay empty"
  fi

  if [ "$errs" -gt 0 ]; then
    dc_die "build artifacts incomplete ($errs problem(s)). Build first: see deploy/servers/prod.md"
  fi

  # Architecture guard: the host is x86_64, so a Mach-O binary would fail
  # at exec time with a confusing error. Catch it here instead.
  for f in dropit-api dropit-workers; do
    if command -v file >/dev/null 2>&1; then
      desc=$(file -b "$dir/$f")
      case $desc in
        *Mach-O*) dc_die "$f is a macOS binary ($desc). Cross-compile for x86_64-musl (see deploy/servers/prod.md)" ;;
        *ELF*64*) : ;;
        *) dc_warn "$f: unexpected file type: $desc" ;;
      esac
    fi
  done

  dc_info "artifacts: 2 binaries, 3 web bundles, $mig_n migration(s)"
  dc_info "target:    $TARGET"
}

# --- upload ----------------------------------------------------------------
upload_artifacts() {
  local host=$1 dir=$2
  dc_step "uploading artifacts"

  # Binaries: install to a staging name first so a transfer interrupted
  # mid-file cannot leave a truncated executable that the unit would exec.
  dc_ssh "$host" 'mkdir -p /srv/dropit/bin /srv/dropit/web /srv/dropit/data/migrations' >/dev/null

  # Note: permissions are applied on the host, not via rsync --chmod. macOS
  # ships openrsync (protocol 29), which rejects --chmod, and the modes here
  # must be exact (a 0644 binary will not exec, a 0777 env file is a finding).
  for f in dropit-api dropit-workers; do
    dc_log "  $f -> /srv/dropit/bin/$f.new"
    dc_rsync "$host" /srv/dropit/bin/ -t "$dir/$f" >/dev/null
    dc_ssh "$host" "mv -f /srv/dropit/bin/$f /srv/dropit/bin/$f.new" >/dev/null
  done

  # club and admin keep their names. The static DJ prototype is served under
  # /dj-mock/ because /dj belongs to the club SPA's own router (live DJ
  # console, /dj/login, /dj/invite/:token) -- see nginx/dropit.conf.
  dc_log "  web bundles -> /srv/dropit/web/{club,admin,dj-mock}"
  dc_rsync "$host" /srv/dropit/web/ -a --delete --exclude 'dj/' "$dir/web/" >/dev/null
  dc_rsync "$host" /srv/dropit/web/dj-mock/ -a --delete "$dir/web/dj/" >/dev/null
  # Drop a stale web/dj from an older deploy: /dj must never be served statically.
  dc_ssh "$host" 'set -e
    rm -rf /srv/dropit/web/dj
    find /srv/dropit/web -type d -exec chmod 755 {} +
    find /srv/dropit/web -type f -exec chmod 644 {} +
    chown -R root:dropit /srv/dropit/web' >/dev/null

  dc_log "  migrations -> /srv/dropit/data/migrations/"
  if [ "$(dc_count_migrations "$dir/migrations")" != 0 ]; then
    dc_rsync "$host" /srv/dropit/data/migrations/ -a --delete "$dir/migrations/" >/dev/null
    # Owned by the service user: migrate.sh connects over the unix socket as it.
    dc_ssh "$host" 'set -e
      chmod 750 /srv/dropit/data/migrations
      find /srv/dropit/data/migrations -type f -exec chmod 640 {} +
      chown -R dropit:dropit /srv/dropit/data/migrations' >/dev/null
  fi

  # Promote the staged binaries. Doing both here (not per-file) keeps the
  # window where api and workers are on different versions to microseconds.
  dc_ssh "$host" 'set -e; for f in dropit-api dropit-workers; do
      chown root:dropit /srv/dropit/bin/$f.new;
      chmod 755 /srv/dropit/bin/$f.new;
      mv -f /srv/dropit/bin/$f.new /srv/dropit/bin/$f;
    done; chown -R root:dropit /srv/dropit/web; ls -l /srv/dropit/bin' >/dev/null
  dc_info "binaries promoted to /srv/dropit/bin/{dropit-api,dropit-workers}"
}

# --- config ----------------------------------------------------------------
install_config() {
  local host=$1
  dc_step "installing nginx site + systemd units"

  # The config bundle is the same one provision.sh uploads: single source of
  # truth for the repo -> host direction. apply-config.sh compares checksums
  # and only reloads when something actually changed.
  local tmp
  tmp=$(mktemp -d "${TMPDIR:-/tmp}/dropit-cfg.XXXXXX")
  mkdir -p "$tmp/nginx" "$tmp/systemd" "$tmp/host"
  cp "$DC_PROD_DIR/nginx/dropit.conf"            "$tmp/nginx/"
  cp "$DC_PROD_DIR/systemd/dropit-api.service"   "$tmp/systemd/"
  cp "$DC_PROD_DIR/systemd/dropit-workers.service" "$tmp/systemd/"
  cp "$DC_PROD_DIR/systemd/dropit.env.example"   "$tmp/systemd/"
  cp "$DC_PROD_DIR/host/apply-config.sh"         "$tmp/host/"
  cp "$DC_PROD_DIR/host/migrate.sh"              "$tmp/host/"
  cp "$DC_PROD_DIR/host/bootstrap-test-club.py"   "$tmp/host/"
  chmod 755 "$tmp/host/"*.sh
  dc_rsync "$host" /root/dropit-deploy/ -a --delete \
    --exclude 'backup/' --exclude 'state/' --exclude 'secrets.env' "$tmp/" >/dev/null
  rm -rf "$tmp"

  dc_ssh "$host" 'chmod 755 /root/dropit-deploy/host/*.sh && bash /root/dropit-deploy/host/apply-config.sh --all' | sed 's/^/    /'
}

# --- migrations ------------------------------------------------------------
run_migrations() {
  local host=$1
  dc_step "applying migrations (psql)"
  dc_ssh "$host" 'bash /root/dropit-deploy/host/migrate.sh' | sed 's/^/    /'
}

# --- restart ---------------------------------------------------------------
restart_services() {
  local host=$1
  dc_step "restarting services"
  # `restart` (not `reload`): the unit runs a plain binary, and a fresh process
  # is what actually picks up a new executable.
  dc_ssh "$host" 'systemctl restart dropit-api.service dropit-workers.service' >/dev/null
  dc_info "restarted dropit-api, dropit-workers"

  # Give the API a moment to bind before health-checking.
  dc_ssh "$host" 'for i in $(seq 1 20); do
      if curl -sf -o /dev/null --max-time 2 http://127.0.0.1:8080/healthz; then exit 0; fi
      sleep 1
    done
    echo "api did not become healthy in 20s" >&2
    systemctl status dropit-api.service --no-pager -n 30 >&2 || true
    exit 1' >/dev/null
}

# --- main ------------------------------------------------------------------
if [ "$VERIFY_ONLY" = 1 ]; then
  health_check "$TARGET" && dc_log "all health checks passed" || dc_die "health checks failed"
  exit 0
fi

preflight "$BUILD_DIR"

if [ "$DRY_RUN" = 1 ]; then
  dc_step "dry run"
  dc_info "would upload:  $BUILD_DIR/{dropit-api,dropit-workers} -> /srv/dropit/bin/"
  dc_info "would upload:  $BUILD_DIR/web/{club,admin,dj,status} -> /srv/dropit/web/"
  dc_info "would upload:  $BUILD_DIR/migrations/*.sql -> /srv/dropit/data/migrations/"
  dc_info "would install: nginx site + systemd units"
  if [ "$SKIP_MIGRATE" = 1 ]; then
    dc_info "would skip:    migrations (--skip-migrate)"
  else
    dc_info "would apply:   migrations via psql"
  fi
  if [ "$NO_RESTART" = 1 ]; then
    dc_info "would skip:    service restart (--no-restart)"
  else
    dc_info "would restart: dropit-api, dropit-workers"
  fi
  dc_ssh "$TARGET" 'bash /root/dropit-deploy/host/migrate.sh --dry-run --status' 2>/dev/null \
    | sed 's/^/    /' || dc_info "(migration status unavailable — host bundle not uploaded yet)"
  exit 0
fi

dc_step "deploying to $TARGET"
dc_info "build dir: $BUILD_DIR"

upload_artifacts "$TARGET" "$BUILD_DIR"

if [ "$SKIP_CONFIG" = 0 ]; then
  install_config "$TARGET"
fi

if [ "$SKIP_MIGRATE" = 0 ]; then
  run_migrations "$TARGET"
else
  dc_info "migrations skipped (--skip-migrate)"
fi

if [ "$NO_RESTART" = 0 ]; then
  restart_services "$TARGET"
else
  dc_info "restart skipped (--no-restart)"
fi

if [ "$NO_RESTART" = 0 ]; then
  health_check "$TARGET" && dc_log "deploy complete — all health checks passed" \
    || dc_die "deploy finished but health checks failed; see journalctl -u dropit-api"
else
  dc_info "skipping health check because services were not restarted"
fi
