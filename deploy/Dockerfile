# DROP IT runtime image — сборка обоих бинарников воркспейса.
#
# Один Dockerfile на оба бинарника: выбирается через ARG BIN
# (по умолчанию `dropit-api`). Образ воркера:
#   docker build --build-arg BIN=dropit-workers -t dropit-workers .
# Образ API:
#   docker build -t dropit-api .   # или --build-arg BIN=dropit-api
#
# Согласуется с layout репозитория: корневой `Cargo.toml` (workspace),
# `Cargo.lock`, `crates/`. Пакет `deny.toml` в сборке не участвует.
# Многоэтапная: builder на rust:1-slim, runtime на debian:bookworm-slim.
# В runtime добавлен `curl` — нужен healthcheck API на /healthz.

ARG BIN=dropit-api
FROM rust:1-slim AS builder
ARG BIN
WORKDIR /build
# Сначала манифесты и лок — кешируем разрешение зависимостей.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# Билдим выбранный бинарник release. Путь к bin-таргету разрешает воркспейс.
RUN cargo build --release --bin ${BIN}

FROM debian:bookworm-slim AS runtime
ARG BIN
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/${BIN} /usr/local/bin/dropit-bin
ENV DROPIT_BIND=0.0.0.0:8080
# /healthz имеет смысл только для API; для воркера порт не открывается.
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/dropit-bin"]
