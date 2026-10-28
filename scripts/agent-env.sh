#!/usr/bin/env bash
# Shared dev environment for agents working in this checkout.
#
#   source scripts/agent-env.sh
#
# The repo ships its own Rust toolchain and cargo home; the local Postgres
# cluster lives in .pgdata on port 55433 (started by scripts/agent-db.sh).
# Nothing here touches the network or a shared server.

DROPIT_REPO="/Volumes/Хранилище/Dev/products/dropit"

export CARGO_HOME="$DROPIT_REPO/.cargo-home"
export RUSTUP_HOME="$DROPIT_REPO/rustup"
export PATH="$DROPIT_REPO/rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"

export PGBIN="/opt/homebrew/opt/postgresql@16/bin"
export DROPIT_PGPORT="${DROPIT_PGPORT:-55433}"

export DATABASE_URL="postgres://dropit@127.0.0.1:${DROPIT_PGPORT}/dropit"
export RUST_LOG="${RUST_LOG:-warn}"
