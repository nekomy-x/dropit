#!/usr/bin/env bash
# deploy/prod/lib.sh — shared helpers for provision.sh / deploy.sh.
#
# Sourced, never executed directly. Kept compatible with bash 3.2 (the default
# /bin/bash on macOS), so: no associative arrays, no ${var,,}, no mapfile.
#
# Password auth: this box has no SSH key installed, and `sshpass` is not
# available everywhere. Instead the root password is exported as DROPIT_SSH_PASS
# and an askpass helper answers ssh's prompt. The password never appears in
# argv (never `ssh -o ...` and never passed to the remote side) and never
# lands in a file on the target.

set -euo pipefail

DC_PROD_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
DC_REPO_ROOT=$(cd -- "$DC_PROD_DIR/../.." && pwd)

if [ -t 1 ]; then
  DC_C_BLUE=$'\033[1;34m'
  DC_C_YEL=$'\033[1;33m'
  DC_C_RED=$'\033[1;31m'
  DC_C_OFF=$'\033[0m'
else
  DC_C_BLUE=''
  DC_C_YEL=''
  DC_C_RED=''
  DC_C_OFF=''
fi

dc_log()  { printf '%s==>%s %s\n' "$DC_C_BLUE" "$DC_C_OFF" "$*"; }
dc_info() { printf '    %s\n' "$*"; }
dc_step() { printf '\n%s==> %s%s\n' "$DC_C_BLUE" "$*" "$DC_C_OFF"; }
dc_warn() { printf '%s!!%s  %s\n' "$DC_C_YEL" "$DC_C_OFF" "$*" >&2; }
dc_err()  { printf '%sxx%s  %s\n' "$DC_C_RED" "$DC_C_OFF" "$*" >&2; }
dc_die()  { dc_err "$*"; exit 1; }

# --- environment -----------------------------------------------------------

# Source deploy/prod/.env.local if present. It is a plain KEY=value bash file.
dc_load_local_env() {
  local f=${1:-$DC_PROD_DIR/.env.local}
  if [ -f "$f" ]; then
    set -a
    # shellcheck disable=SC1090
    . "$f"
    set +a
    DC_LOCAL_ENV_FILE=$f
  fi
}

dc_require_local_env() {
  [ -n "${DC_LOCAL_ENV_FILE:-}" ] || dc_die "deploy/prod/.env.local not found. Copy deploy/prod/.env.example to deploy/prod/.env.local and fill it in."
}

dc_target_host() {
  printf '%s' "${PROD_HOST:-root@150.241.68.57}"
}

# --- ssh / rsync -----------------------------------------------------------

dc_require_tools() {
  local missing=''
  for t in ssh rsync "$@"; do
    command -v "$t" >/dev/null 2>&1 || missing="$missing $t"
  done
  [ -z "$missing" ] || dc_die "missing required tool(s):$missing"
}

# Path to an askpass helper that prints $DROPIT_SSH_PASS. Reuses the harness
# helper when present, otherwise creates one under a 0700 temp dir.
dc_askpass_path() {
  if [ -n "${SSH_ASKPASS:-}" ] && [ -x "$SSH_ASKPASS" ]; then
    printf '%s' "$SSH_ASKPASS"
    return 0
  fi
  local candidate="${TMPDIR:-/tmp}/dsh-ssh/askpass.sh"
  if [ -x "$candidate" ]; then
    printf '%s' "$candidate"
    return 0
  fi
  local dir="${TMPDIR:-/tmp}/dropit-ssh"
  mkdir -p "$dir"
  chmod 700 "$dir"
  candidate="$dir/askpass.sh"
  if [ ! -x "$candidate" ]; then
    printf '#!/bin/sh\nprintf "%%s\\n" "$DROPIT_SSH_PASS"\n' > "$candidate"
    chmod 700 "$candidate"
  fi
  printf '%s' "$candidate"
}

dc_ssh_opts() {
  local opts
  # This host's key was handed over out-of-band with the password, so there is
  # no trusted fingerprint to pin. Keep the workstation's ~/.ssh/known_hosts
  # untouched and rely on the password for auth. Override with
  # DEPLOY_SSH_HOSTKEY=yes + DEPLOY_SSH_KNOWN_HOSTS=~/.ssh/known_hosts once a
  # fingerprint has been verified.
  opts="-o StrictHostKeyChecking=${DEPLOY_SSH_HOSTKEY:-no}"
  opts="$opts -o UserKnownHostsFile=${DEPLOY_SSH_KNOWN_HOSTS:-/dev/null}"
  opts="$opts -o LogLevel=ERROR"
  opts="$opts -o ConnectTimeout=20 -o ServerAliveInterval=15 -o ServerAliveCountMax=4"
  if [ -n "${DEPLOY_SSH_EXTRA_OPTS:-}" ]; then
    opts="$opts $DEPLOY_SSH_EXTRA_OPTS"
  fi
  printf '%s' "$opts"
}

# dc_ssh <target> <remote command...>
dc_ssh() {
  local target=$1
  shift
  # shellcheck disable=SC2046
  if [ -n "${DROPIT_SSH_PASS:-}" ]; then
    SSH_ASKPASS=$(dc_askpass_path) SSH_ASKPASS_REQUIRE=force \
      ssh $(dc_ssh_opts) "$target" "$@" < /dev/null
  else
    ssh $(dc_ssh_opts) "$target" "$@" < /dev/null
  fi
}

# dc_ssh_stdin <target> <remote command>
# Same as dc_ssh but leaves stdin attached to the caller, for piping a payload
# (e.g. the secrets file) into the remote command.
dc_ssh_stdin() {
  local target=$1
  shift
  # shellcheck disable=SC2046
  if [ -n "${DROPIT_SSH_PASS:-}" ]; then
    SSH_ASKPASS=$(dc_askpass_path) SSH_ASKPASS_REQUIRE=force \
      ssh $(dc_ssh_opts) "$target" "$@"
  else
    ssh $(dc_ssh_opts) "$target" "$@"
  fi
}

# dc_ssh_script <target> <local script path> [args...]
# Feeds the script to `bash -s` on the target so it never has to be copied
# there first (and cannot be half-written by an interrupted transfer).
dc_ssh_script() {
  local target=$1
  local script=$2
  shift 2
  local remote_cmd='bash -s --'
  local a
  for a in "$@"; do
    remote_cmd="$remote_cmd $(printf '%q' "$a")"
  done
  [ -f "$script" ] || dc_die "script not found: $script"
  # shellcheck disable=SC2046
  if [ -n "${DROPIT_SSH_PASS:-}" ]; then
    SSH_ASKPASS=$(dc_askpass_path) SSH_ASKPASS_REQUIRE=force \
      ssh $(dc_ssh_opts) "$target" "$remote_cmd" < "$script"
  else
    ssh $(dc_ssh_opts) "$target" "$remote_cmd" < "$script"
  fi
}

# dc_rsync <target> <remote path> <rsync args...>
# rsync's stdout is left to the caller (used for --itemize-changes change
# detection), stderr passes through.
dc_rsync() {
  local target=$1
  local remote_path=$2
  shift 2
  local rsh="ssh $(dc_ssh_opts) -o BatchMode=no"
  if [ -n "${DROPIT_SSH_PASS:-}" ]; then
    SSH_ASKPASS=$(dc_askpass_path) SSH_ASKPASS_REQUIRE=force \
      rsync -e "$rsh" "$@" "$target:$remote_path"
  else
    rsync -e "$rsh" "$@" "$target:$remote_path"
  fi
}

# --- misc ------------------------------------------------------------------

dc_gen_alnum() {
  local n=${1:-40}
  dd if=/dev/urandom bs=512 count=1 2>/dev/null | LC_ALL=C tr -dc 'A-Za-z0-9' | cut -c1-"$n"
}

dc_gen_hex() {
  openssl rand -hex "${1:-32}"
}

# Report how many migration files were handed over, so a silent empty
# directory does not turn into a silently empty database.
dc_count_migrations() {
  local dir=$1
  [ -d "$dir" ] || { printf '0'; return 0; }
  find "$dir" -maxdepth 1 -name '*.sql' -type f | wc -l | tr -d ' '
}
