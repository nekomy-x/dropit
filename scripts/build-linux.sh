#!/usr/bin/env bash
# Сборка Linux-артефактов DROP IT на macOS (aarch64) для VPS 150.241.68.57.
#
# Почему так:
#  * Rust-таргет x86_64-unknown-linux-musl живёт в отдельном RUSTUP_HOME
#    (в ~/.rustup добавить таргет нельзя — каталог только для чтения
#    в песочнице агента);
#  * линкер/cc — zig (musl + glibc из коробки, собирает C-часть ring);
#  * cargo-zigbuild пишет кэш в ~/Library/Caches, поэтому HOME подменяется
#    на записываемый каталог, а CARGO_HOME/rustup/cargo-bin задаются явно.
#
# Артефакты: build/
#   dropit-api, dropit-workers      (статические musl-бинарники)
#   web/club/…, web/admin/…, web/dj/index.html
#   migrations/*.sql
#
# Использование:
#   scripts/build-linux.sh            # бинарники + веб
#   scripts/build-linux.sh --bin-only # только бинарники
#   scripts/build-linux.sh --web-only # только веб-бандлы
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STORAGE="${DROPIT_TOOLCHAIN_DIR:-/Volumes/Хранилище/Dev/dropit_ascii}"
ZIG_DIR="$STORAGE/zig/zig-aarch64-macos-0.17.0"
RUSTUP_WRITABLE="$STORAGE/rustup"
SANDBOX_HOME="$STORAGE/home"
SYSTEM_CARGO_BIN="/Users/nekomy/.cargo/bin"

TARGET="x86_64-unknown-linux-musl"
OUT="$REPO_ROOT/build"

mode="${1:-all}"

export CARGO_HOME="$REPO_ROOT/.cargo-home"
export RUSTUP_HOME="$RUSTUP_WRITABLE"
export HOME="$SANDBOX_HOME"
export PATH="$ZIG_DIR:$CARGO_HOME/bin:$SYSTEM_CARGO_BIN:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin"
mkdir -p "$HOME/Library/Caches" "$HOME/.cache" "$OUT"

# Быстрая релизная сборка: LTO в профиле репозитория включён, для стенда
# важнее время сборки, чем последние 5 % перфоманса.
export CARGO_PROFILE_RELEASE_LTO="${CARGO_PROFILE_RELEASE_LTO:-false}"
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-16}"

build_bins() {
  echo "==> cargo zigbuild ($TARGET) — dropit-api, dropit-workers"
  ( cd "$REPO_ROOT" && cargo zigbuild --release --target "$TARGET" \
      -p dropit-gateway-api -p dropit-workers )
  install -m 0755 "$REPO_ROOT/target/$TARGET/release/dropit-api"     "$OUT/dropit-api"
  install -m 0755 "$REPO_ROOT/target/$TARGET/release/dropit-workers" "$OUT/dropit-workers"
  # seed_demo — демо-данные стенда (клуб/DJ/админ + треки); запускается на VPS.
  install -m 0755 "$REPO_ROOT/target/$TARGET/release/seed_demo"     "$OUT/seed_demo"
  file "$OUT/dropit-api" "$OUT/dropit-workers" "$OUT/seed_demo"
  ls -lh "$OUT/dropit-api" "$OUT/dropit-workers" "$OUT/seed_demo"
}

build_migrations() {
  echo "==> migrations"
  rm -rf "$OUT/migrations"
  mkdir -p "$OUT/migrations"
  cp "$REPO_ROOT"/crates/infra-db/migrations/*.sql "$OUT/migrations/"
  ls "$OUT/migrations" | tail -3
}

build_web() {
  echo "==> web-club"
  ( cd "$REPO_ROOT/apps/web-club"  && npm run --silent build )
  echo "==> web-admin"
  ( cd "$REPO_ROOT/apps/web-admin" && npm run --silent build )

  rm -rf "$OUT/web"
  mkdir -p "$OUT/web/club" "$OUT/web/admin" "$OUT/web/dj"
  cp -R "$REPO_ROOT/apps/web-club/dist/."  "$OUT/web/club/"
  cp -R "$REPO_ROOT/apps/web-admin/dist/." "$OUT/web/admin/"
  cp "$REPO_ROOT/apps/web-dj/index.html"   "$OUT/web/dj/index.html"
  du -sh "$OUT/web/club" "$OUT/web/admin" "$OUT/web/dj"
}

case "$mode" in
  --bin-only) build_bins; build_migrations ;;
  --web-only) build_web ;;
  all)        build_bins; build_migrations; build_web ;;
  *) echo "unknown mode: $mode" >&2; exit 2 ;;
esac

echo "==> готово: $OUT"
