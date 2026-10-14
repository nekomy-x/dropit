//! DROP IT API Gateway (F1: identity + RBAC + health).
//!
//! F0A: каркас + health. F1: device-token, логины, refresh, RBAC.

mod auth;
#[cfg(test)]
mod e2e;
#[cfg(test)]
mod e2e_f8;
#[cfg(test)]
mod e2e_panels;
#[cfg(test)]
mod e2e_payments;
#[cfg(test)]
mod e2e_payouts;
#[cfg(test)]
mod e2e_requests;
#[cfg(test)]
mod e2e_sessions;
#[cfg(test)]
mod e2e_ws;
mod panel_routes;
mod payments;
mod payments_routes;
mod payouts_routes;
mod rbac;
mod requests_routes;
mod routes;
mod sessions_routes;
mod ws;

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

    // БД (Test Mode: DATABASE_URL или DROPIT_DATABASE_URL). Без неё
    // сервер стартует, но data-роуты отвечают 503 (README: как
    // поднять compose + seed).
    let db_url = std::env::var("DATABASE_URL")
        .or_else(|_| std::env::var("DROPIT_DATABASE_URL"))
        .ok();
    let (repo, sessions, requests, billing, payouts, jobs) = if let Some(url) = db_url {
        match dropit_infra_db::connect(&url).await {
            Ok(pool) => {
                tracing::info!("database connected (pool ready)");
                let pool = std::sync::Arc::new(pool);
                (
                    Some(std::sync::Arc::new(dropit_infra_db::IdentityRepo::new(
                        pool.as_ref().clone(),
                    ))),
                    Some(std::sync::Arc::new(dropit_infra_db::SessionRepo::new(
                        pool.as_ref().clone(),
                    ))),
                    Some(std::sync::Arc::new(dropit_infra_db::RequestRepo::new(
                        pool.as_ref().clone(),
                    ))),
                    Some(std::sync::Arc::new(dropit_infra_db::BillingRepo::new(
                        pool.as_ref().clone(),
                    ))),
                    Some(std::sync::Arc::new(dropit_infra_db::PayoutRepo::new(
                        pool.as_ref().clone(),
                    ))),
                    Some(std::sync::Arc::new(dropit_infra_db::JobsRepo::new(
                        pool.as_ref().clone(),
                    ))),
                )
            }
            Err(e) => {
                tracing::warn!("database unreachable ({e}) — data routes will 503");
                (None, None, None, None, None, None)
            }
        }
    } else {
        tracing::warn!("DATABASE_URL not set — data routes will 503");
        (None, None, None, None, None, None)
    };

    let env = routes::ApiEnv {
        keys,
        repo,
        sessions,
        requests,
        billing,
        payouts,
        jobs,
        ws: Some(std::sync::Arc::new(crate::ws::WsHub::new())),
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
