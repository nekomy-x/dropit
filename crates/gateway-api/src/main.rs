//! DROP IT API Gateway (F0A: health + формат ошибок).
//!
//! Реальные роуты появляются в F1+ (identity) — здесь только каркас,
//! health-check для Compose/CI и единый envelope ошибок.

use axum::{http::StatusCode, response::IntoResponse, Json, Router};
use serde_json::json;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let app = Router::new()
        .route("/healthz", axum::routing::get(healthz))
        .route("/readyz", axum::routing::get(readyz));

    let addr: SocketAddr = std::env::var("DROPIT_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()?;
    tracing::info!("DROP IT API listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Liveness: процесс жив. Никаких проверок зависимостей.
async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

/// Readiness: сейчас version-only; в F0B добавит ping Postgres/Redis.
async fn readyz() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") })),
    )
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received, draining connections");
}
