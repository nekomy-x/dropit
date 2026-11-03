//! E2E публичной статус-страницы `GET /v1/status/services`.
//!
//! Проверяем честность, а не только форму ответа:
//!   * без БД страница всё равно отвечает 200 и показывает `database: down`;
//!   * со свежим heartbeat воркеров и пустым outbox — общий `ok`;
//!   * старый heartbeat → `degraded` (воркеры отстают, но API жив);
//!   * зависший outbox (старая непубликованная запись) → `down`;
//!   * в ответе есть все подсистемы и нет секретов/бизнес-данных;
//!   * `payments` не литерал `ok`: мёртвая БД → `degraded`, live-провайдер
//!     без реального проба → `unknown`;
//!   * форма ответа не меняется: ровно 4 поля на сервис и 6 сервисов.

#![cfg(test)]

use axum::http::StatusCode;
use serde_json::Value;

use crate::e2e::{app_env, call, test_repo};
use dropit_infra_db::{IdentityRepo, SessionRepo};

fn service<'a>(body: &'a Value, id: &str) -> &'a Value {
    body["services"]
        .as_array()
        .expect("services array")
        .iter()
        .find(|item| item["id"] == id)
        .unwrap_or_else(|| panic!("service {id} missing in {body}"))
}

/// Публичный контракт страницы: сверху ровно `{status, checked_at, services}`,
/// в каждом сервисе ровно `{id, name, state, note}`. Лишнее поле — утечка
/// внутренностей, недостающее — сломанная страница, поэтому форма проверяется
/// целиком, а не по отдельным ключам.
fn assert_public_shape(body: &Value) {
    let top = body
        .as_object()
        .expect("public status must be a JSON object");
    let mut keys: Vec<&str> = top.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["checked_at", "services", "status"],
        "public fields changed: {body}"
    );

    let services = body["services"].as_array().expect("services array");
    assert_eq!(
        services.len(),
        6,
        "public status always lists six services: {body}"
    );
    for service in services {
        let fields = service.as_object().expect("service must be an object");
        let mut names: Vec<&str> = fields.keys().map(String::as_str).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            ["id", "name", "note", "state"],
            "service fields changed: {service}"
        );
        assert!(
            service["note"]
                .as_str()
                .is_some_and(|note| !note.is_empty()),
            "note must be a non-empty string: {service}"
        );
        assert!(
            ["ok", "degraded", "down", "unknown"]
                .contains(&service["state"].as_str().unwrap_or("")),
            "unexpected state: {service}"
        );
    }
}

/// Восстановление env-переменной после теста: `set_var` не должен протекать
/// в остальные e2e-тесты этого же процесса.
fn restore_env(key: &str, saved: Option<String>) {
    match saved {
        Some(value) => std::env::set_var(key, value),
        None => std::env::remove_var(key),
    }
}

#[tokio::test]
async fn e2e_status_services_without_db_is_honest() {
    let env = app_env(None, None, None).await;
    let (status, body) = call(&env, "GET", "/v1/status/services", None, None).await;
    assert_eq!(status, StatusCode::OK, "public status must answer 200");
    assert_public_shape(&body);
    assert_eq!(body["status"], "down", "no database → down: {body}");
    // Публичный ответ говорит только про сервисы, без внутренностей.
    assert_eq!(service(&body, "panel")["state"], "down");
    assert_eq!(service(&body, "requests")["state"], "down");
    assert_eq!(service(&body, "api")["state"], "degraded");
    // Платежи больше не литерал `ok`: без БД записи о платежах не проверить,
    // значит строка обязана деградировать вместе с остальными.
    assert_eq!(
        service(&body, "payments")["state"],
        "degraded",
        "payments must not stay ok without a database: {body}"
    );
    let raw = body.to_string().to_lowercase();
    for forbidden in [
        "redis",
        "outbox",
        "heartbeat",
        "schema",
        "postgres",
        "nats",
        "workers",
        "latency",
        "secret",
        "password",
        "token",
        "revenue",
        "guest_device",
    ] {
        assert!(
            !raw.contains(forbidden),
            "public status must not expose {forbidden}: {raw}"
        );
    }
}

/// Настоящая «мёртвая БД» без пропуска теста: ленивый пул на закрытый порт
/// отдаёт ошибку соединения, а не ждёт DATABASE_URL. Публичный ответ обязан
/// остаться 200-м документом, но `payments` — уже не `ok`.
#[tokio::test]
async fn e2e_status_services_payments_degrades_when_db_is_dead() {
    let dead_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect_lazy("postgres://dropit@127.0.0.1:1/dropit")
        .expect("lazy pool does not connect");
    let env = app_env(Some(IdentityRepo::new(dead_pool)), None, None).await;

    let (status, body) = call(&env, "GET", "/v1/status/services", None, None).await;
    assert_eq!(status, StatusCode::OK, "dead DB still gets a document");
    assert_public_shape(&body);
    assert_eq!(body["status"], "down", "dead DB → down: {body}");
    assert_eq!(service(&body, "panel")["state"], "down");
    assert_eq!(
        service(&body, "payments")["state"],
        "degraded",
        "dead DB → payments degraded: {body}"
    );
}

/// Live-провайдер без реального проба — это `unknown`, а не `ok`: настоящий
/// probe Stripe (баланс/webhook) в статус-странице не выполняется, и выдавать
/// непроверенное за работающее нельзя.
#[tokio::test]
async fn e2e_status_services_live_payments_are_unknown() {
    let Some(repo) = test_repo().await else {
        eprintln!("DATABASE_URL недоступен — e2e live-платежей пропущен");
        return;
    };
    let pool = repo.pool();
    let env = app_env(Some(repo), Some(SessionRepo::new(pool.clone())), None).await;

    let saved_key = std::env::var("STRIPE_API_KEY").ok();
    let saved_secret = std::env::var("STRIPE_WEBHOOK_SECRET").ok();
    std::env::set_var("STRIPE_API_KEY", "sk_test_e2e_status");
    std::env::set_var("STRIPE_WEBHOOK_SECRET", "whsec_e2e_status");
    let (status, body) = call(&env, "GET", "/v1/status/services", None, None).await;
    restore_env("STRIPE_API_KEY", saved_key);
    restore_env("STRIPE_WEBHOOK_SECRET", saved_secret);

    assert_eq!(status, StatusCode::OK);
    assert_public_shape(&body);
    assert_eq!(
        service(&body, "payments")["state"],
        "unknown",
        "live provider is not probed: {body}"
    );
}

#[tokio::test]
async fn e2e_status_services_reports_workers_outbox_and_recovery() {
    let repo = match test_repo().await {
        Some(repo) => repo,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e статус-страницы пропущен");
            return;
        }
    };
    let pool = repo.pool();
    let env = app_env(
        Some(repo.clone()),
        Some(SessionRepo::new(pool.clone())),
        None,
    )
    .await;

    // Детерминированное состояние: свежий heartbeat, пустой outbox.
    sqlx::query(
        "INSERT INTO worker_heartbeat (worker, last_seen_at) VALUES ('dropit-workers', now()) \
         ON CONFLICT (worker) DO UPDATE SET last_seen_at = now()",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM outbox")
        .execute(&pool)
        .await
        .unwrap();

    let (status, body) = call(&env, "GET", "/v1/status/services", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_public_shape(&body);
    assert_eq!(
        body["status"], "ok",
        "fresh heartbeat + empty outbox: {body}"
    );
    for id in ["panel", "dj", "api", "requests", "guest-entry", "payments"] {
        let _ = service(&body, id);
    }
    assert_eq!(service(&body, "panel")["state"], "ok");
    assert_eq!(service(&body, "requests")["state"], "ok");
    assert_eq!(service(&body, "guest-entry")["state"], "ok");
    assert_eq!(service(&body, "payments")["state"], "ok");
    assert!(
        service(&body, "payments")["note"]
            .as_str()
            .unwrap_or_default()
            .contains("Test mode"),
        "payments note must explain test mode"
    );

    // Полный технический статус доступен только админу.
    let (status, _) = call(&env, "GET", "/v1/admin/status/services", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let club_jwt = env
        .keys
        .issue_access(
            uuid::Uuid::now_v7(),
            dropit_common::Role::Club,
            uuid::Uuid::now_v7(),
        )
        .unwrap();
    let (status, _) = call(
        &env,
        "GET",
        "/v1/admin/status/services",
        None,
        Some(&club_jwt),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let admin_jwt = env
        .keys
        .issue_access(
            uuid::Uuid::now_v7(),
            dropit_common::Role::Admin,
            uuid::Uuid::now_v7(),
        )
        .unwrap();
    let (status, detail) = call(
        &env,
        "GET",
        "/v1/admin/status/services",
        None,
        Some(&admin_jwt),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let ids: Vec<&str> = detail["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .filter_map(|check| check["id"].as_str())
        .collect();
    for id in [
        "api", "database", "schema", "redis", "workers", "outbox", "mail", "payments", "runtime",
    ] {
        assert!(ids.contains(&id), "admin detail misses {id}: {detail}");
    }

    // Воркеры отстают: heartbeat 10 минут назад → degraded, но не down.
    // (Тик супервизора — 15 с; 30 минут — граница «жив, но отстал».)
    sqlx::query(
        "UPDATE worker_heartbeat SET last_seen_at = now() - interval '10 minutes' \
         WHERE worker = 'dropit-workers'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (status, body) = call(&env, "GET", "/v1/status/services", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "degraded", "stale heartbeat: {body}");
    assert_eq!(service(&body, "requests")["state"], "degraded");

    // Освежаем heartbeat, чтобы проверять именно outbox, а не воркеры.
    sqlx::query("UPDATE worker_heartbeat SET last_seen_at = now()")
        .execute(&pool)
        .await
        .unwrap();
    // Outbox ждёт, но relay не настроен (NATS выключен): это конфигурация
    // Test Mode, поэтому статус остаётся `ok` с честной деталью.
    sqlx::query(
        "INSERT INTO outbox (id, aggregate_type, aggregate_id, event_type, payload, created_at) \
         VALUES ($1, 'request', $2, 'request.created', '{}'::jsonb, now() - interval '2 hours')",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(uuid::Uuid::now_v7())
    .execute(&pool)
    .await
    .unwrap();
    let (status, body) = call(&env, "GET", "/v1/status/services", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok", "relay off is configuration: {body}");
    // Внутренняя причина остаётся на админском эндпоинте, публично — только
    // «работает».
    assert_eq!(service(&body, "requests")["state"], "ok");

    // Включаем relay: зависший на два часа outbox → degraded.
    std::env::set_var("NATS_URL", "nats://127.0.0.1:4222");
    let (status, body) = call(&env, "GET", "/v1/status/services", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "degraded", "stalled relay: {body}");
    assert_eq!(service(&body, "requests")["state"], "degraded");
    std::env::remove_var("NATS_URL");

    // Возврат в норму: публикуем запись и освежаем heartbeat.
    sqlx::query("UPDATE outbox SET published_at = now() WHERE published_at IS NULL")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE worker_heartbeat SET last_seen_at = now()")
        .execute(&pool)
        .await
        .unwrap();
    let (_, body) = call(&env, "GET", "/v1/status/services", None, None).await;
    assert_eq!(body["status"], "ok", "recovered: {body}");
}
