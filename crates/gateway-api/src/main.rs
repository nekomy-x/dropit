//! DROP IT API Gateway (F1: identity + RBAC + health).
//!
//! F0A: каркас + health. F1: device-token, логины, refresh, RBAC.

mod auth;
#[cfg(test)]
mod e2e;
#[cfg(test)]
mod e2e_sessions;
mod rbac;
mod routes;
mod sessions_routes;

use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let keys = auth::JwtKeys::from_secret(
        std::env::var("DROPIT_JWT_SECRET")
            .unwrap_or_else(|_| "dev-only-secret-change-me-32-bytes!!".into())
            .as_bytes(),
    );

    let env = routes::ApiEnv {
        keys,
        repo: None, // подключается при DROPIT_DATABASE_URL (main-рантайм)
        sessions: None,
    };
    let app = routes::router(env);

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
