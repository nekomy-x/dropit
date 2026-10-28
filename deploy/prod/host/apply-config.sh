#!/usr/bin/env bash
# deploy/prod/host/apply-config.sh — install nginx site + systemd units.
#
# Runs ON the target host as root. Invoked by provision.sh and deploy.sh from
# the uploaded bundle (default /root/dropit-deploy).
#
# Idempotent: each file is compared by sha256 and only replaced when its
# content actually differs, so re-running produces no churn and no reload.
# Safety: the current nginx config is backed up before being replaced, and if
# `nginx -t` rejects the candidate the backup is restored and the script fails
# — a bad config can never take the site down.
#
# Usage: apply-config.sh [--nginx] [--units] [--all] [--dry-run]
# Prints one machine-readable line per managed file: "<label> <state>".

set -euo pipefail

BUNDLE=${DROPIT_BUNDLE:-/root/dropit-deploy}
STATE_DIR=$BUNDLE/state
BACKUP_DIR=$BUNDLE/backup
NGINX_AVAIL=/etc/nginx/sites-available/dropit.conf
NGINX_ENABLED=/etc/nginx/sites-enabled/dropit.conf
NGINX_DEFAULT=/etc/nginx/sites-enabled/default
UNIT_DIR=/etc/systemd/system
UNITS="dropit-api.service dropit-workers.service"

DO_NGINX=0
DO_UNITS=0
DRY_RUN=0

for arg in "$@"; do
  case $arg in
    --nginx)   DO_NGINX=1 ;;
    --units)   DO_UNITS=1 ;;
    --all)     DO_NGINX=1; DO_UNITS=1 ;;
    --dry-run) DRY_RUN=1 ;;
    *) echo "apply-config.sh: unknown argument '$arg'" >&2; exit 2 ;;
  esac
done
[ "$DO_NGINX$DO_UNITS" != "00" ] || { DO_NGINX=1; DO_UNITS=1; }

log() { printf '  [config] %s\n' "$*"; }

sha() {
  if [ -f "$1" ]; then sha256sum "$1" | awk '{print $1}'; else printf 'missing'; fi
}

# install_if_changed <src> <dest> <mode> <owner:group> <label>
# Echoes "<label> changed|unchanged|created".
install_if_changed() {
  local src=$1 dest=$2 mode=$3 owner=$4 label=$5
  [ -f "$src" ] || { echo "$label missing-source"; return 0; }
  if [ "$(sha "$src")" = "$(sha "$dest")" ]; then
    echo "$label unchanged"
    return 0
  fi
  local state=changed
  [ -f "$dest" ] || state=created
  if [ "$DRY_RUN" = 1 ]; then
    echo "$label $state(dry-run)"
    return 0
  fi
  mkdir -p "$(dirname "$dest")"
  if [ -f "$dest" ]; then
    mkdir -p "$BACKUP_DIR"
    cp -a "$dest" "$BACKUP_DIR/$(basename "$dest").bak"
  fi
  install -m "$mode" -o "${owner%%:*}" -g "${owner##*:}" "$src" "$dest"
  echo "$label $state"
}

mkdir -p "$STATE_DIR" "$BACKUP_DIR"

# --- nginx -----------------------------------------------------------------
if [ "$DO_NGINX" = 1 ]; then
  state=$(install_if_changed "$BUNDLE/nginx/dropit.conf" "$NGINX_AVAIL" 644 root:root nginx-site)

  # The distro default site also claims default_server; two default_server
  # blocks on :80 make nginx refuse to start. Our site replaces it.
  default_removed=unchanged
  if [ -L "$NGINX_DEFAULT" ] || [ -f "$NGINX_DEFAULT" ]; then
    if [ "$DRY_RUN" = 1 ]; then
      default_removed='removed(dry-run)'
    else
      rm -f "$NGINX_DEFAULT"
      default_removed=removed
    fi
  fi

  link_state=unchanged
  if [ ! -L "$NGINX_ENABLED" ]; then
    if [ "$DRY_RUN" = 1 ]; then
      link_state='linked(dry-run)'
    else
      ln -sfn "$NGINX_AVAIL" "$NGINX_ENABLED"
      link_state=linked
    fi
  fi

  echo "$state"
  log "default site: $default_removed, sites-enabled link: $link_state"

  if [ "$DRY_RUN" = 1 ]; then
    echo "nginx reload skipped(dry-run)"
  elif [ "$state" = unchanged ] && [ "$default_removed" = unchanged ] && [ "$link_state" = unchanged ]; then
    echo "nginx reload skipped(no changes)"
  else
    if nginx -t >"$STATE_DIR/nginx-test.log" 2>&1; then
      systemctl reload nginx
      echo "nginx reloaded"
      log "config test ok, reloaded"
    else
      log "nginx -t FAILED — restoring previous config"
      sed -n '1,40p' "$STATE_DIR/nginx-test.log" >&2 || true
      if [ -f "$BACKUP_DIR/dropit.conf.bak" ]; then
        install -m 644 -o root -g root "$BACKUP_DIR/dropit.conf.bak" "$NGINX_AVAIL"
        if nginx -t >/dev/null 2>&1; then
          systemctl reload nginx || true
          log "previous config restored and reloaded"
        else
          log "previous config also fails nginx -t — manual attention required"
        fi
      else
        rm -f "$NGINX_ENABLED"
      fi
      echo "nginx broken"
      exit 1
    fi
  fi
fi

# --- systemd units ---------------------------------------------------------
if [ "$DO_UNITS" = 1 ]; then
  unit_changed=0
  for u in $UNITS; do
    out=$(install_if_changed "$BUNDLE/systemd/$u" "$UNIT_DIR/$u" 644 root:root "unit:$u")
    echo "$out"
    case $out in
      *' changed'*|*' created'*) unit_changed=1 ;;
    esac
  done

  if [ "$DRY_RUN" = 1 ]; then
    echo "systemd daemon-reload skipped(dry-run)"
  elif [ "$unit_changed" = 1 ]; then
    systemctl daemon-reload
    # `enable` is idempotent and keeps the units in place across reboots.
    systemctl enable $UNITS >/dev/null 2>&1 || true
    echo "systemd daemon-reload done"
    log "units installed, daemon-reload + enable done"
  else
    # Still make sure they are enabled — a fresh host may have the unit
    # files from a previous provision but not the wants-symlinks.
    systemctl enable $UNITS >/dev/null 2>&1 || true
    echo "systemd daemon-reload skipped(no changes)"
  fi
fi
