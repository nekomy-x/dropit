# DROP IT API runtime image.
# Многоэтапная сборка: в среде без rustc/cargo образ служит также
# единственным способом реально проверить компиляцию (CI/локально).

FROM rust:1-slim AS builder
WORKDIR /build
COPY Cargo.toml ./
COPY crates ./crates
RUN cargo build --release --bin dropit-api

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/dropit-api /usr/local/bin/dropit-api
ENV DROPIT_BIND=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/dropit-api"]
