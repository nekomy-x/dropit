//! E2e-тест роутов панелей (F7-бэкенд).
//!
//! Критерии:
//!   * platform-settings roundtrip: PATCH значений → GET возвращает;
//!     NULL-семантика (clear → NULL = «не задано»);
//!   * pricing: история Q6 — новая строка закрывает старую (valid_to);
//!   * admin dashboard/now, clubs (блокировка + аудит);
//!   * club dashboard/profile/earnings/djs;
//!   * dj shift/stats;
//!   * payments + splits;
//!   * RBAC: клуб не может в админ-роуты.

#![cfg(test)]

use axum::http::StatusCode;
use serde_json::json;

use crate::e2e::{app_env, call, test_repo};
use dropit_infra_db::SessionRepo;

#[tokio::test]
async fn e2e_panel_routes() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e панелей пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let env = app_env(Some(repo.clone()), Some(sessions.clone()), None).await;
    let pool = repo.pool();

    // --- Токены ------------------------------------------------------
    let admin_id = uuid::Uuid::now_v7();
    let club_id = uuid::Uuid::now_v7();
    let dj_id = uuid::Uuid::now_v7();
    let admin = env
        .keys
        .issue_access(admin_id, dropit_common::Role::Admin, uuid::Uuid::now_v7())
        .unwrap();
    let club = env
        .keys
        .issue_access(club_id, dropit_common::Role::Club, uuid::Uuid::now_v7())
        .unwrap();
    let dj = env
        .keys
        .issue_access(dj_id, dropit_common::Role::Dj, uuid::Uuid::now_v7())
        .unwrap();

    sqlx::query("INSERT INTO club (id, name, slug, email, password_hash) VALUES ($1, 'Panel Club', 'panel-club', 'panel@club.test', 'x')")
        .bind(club_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO dj (id, club_id, email, password_hash, display_name) VALUES ($1, $2, 'panel@dj.test', 'x', 'DJ Panel')")
        .bind(dj_id)
        .bind(club_id)
        .execute(&pool)
        .await
        .unwrap();

    // --- platform-settings: NULL-семантика + roundtrip --------------
    let (s, body) = call(
        &env,
        "GET",
        "/v1/admin/platform-settings",
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "settings get: {body}");
    assert_eq!(body["commission_mode"], "unset"); // стартовое значение

    // PATCH: задаём (валидные диапазоны; значения выбирает владелец — Q11).
    let (s, body) = call(
        &env,
        "PATCH",
        "/v1/admin/platform-settings",
        Some(json!({
            "commission_mode": "percent",
            "commission_percent_bp": 3000,
            "min_price_minor": 100,
            "max_price_minor": 1000
        })),
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "settings patch: {body}");
    assert_eq!(body["commission_mode"], "percent");
    assert_eq!(body["commission_percent_bp"], 3000);
    assert_eq!(body["min_price_minor"], 100);

    // Некорректный диапазон отклоняется.
    let (s, _) = call(
        &env,
        "PATCH",
        "/v1/admin/platform-settings",
        Some(json!({ "commission_percent_bp": 99999 })),
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "bp out of range");

    // clear → NULL («не задано»).
    let (s, body) = call(
        &env,
        "PATCH",
        "/v1/admin/platform-settings",
        Some(json!({ "clear_commission_percent_bp": true })),
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        body["commission_percent_bp"].is_null(),
        "cleared to NULL: {body}"
    );

    // Клуб НЕ может в настройки платформы.
    let (s, _) = call(
        &env,
        "GET",
        "/v1/admin/platform-settings",
        None,
        Some(&club),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "club forbidden on admin route");

    // --- pricing: история Q6 ------------------------------------------
    let (s, body) = call(
        &env,
        "POST",
        "/v1/club/pricing",
        Some(json!({ "price_minor": 300 })),
        Some(&club),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "pricing create: {body}");
    let (s, _) = call(
        &env,
        "POST",
        "/v1/club/pricing",
        Some(json!({ "price_minor": 500, "dj_share_bp": 6000 })),
        Some(&club),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);

    let (s, body) = call(&env, "GET", "/v1/club/pricing", None, Some(&club)).await;
    assert_eq!(s, StatusCode::OK);
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "pricing history: {body}");
    // Q6: старая закрыта (valid_to NOT NULL), новая действует.
    // Порядок при равных микросекундах не гарантирован — ищем
    // по значению политики.
    let old = items
        .iter()
        .find(|i| i["price_minor"] == 300)
        .expect("old 300 policy");
    let new = items
        .iter()
        .find(|i| i["price_minor"] == 500)
        .expect("new 500 policy");
    assert!(!old["valid_to"].is_null(), "old policy closed (Q6)");
    assert!(new["valid_to"].is_null(), "new policy active");

    // Некорректная цена отклоняется.
    let (s, _) = call(
        &env,
        "POST",
        "/v1/club/pricing",
        Some(json!({ "price_minor": 0 })),
        Some(&club),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);

    // --- клуб: dashboard/profile/djs -----------------------------------
    let (s, body) = call(&env, "GET", "/v1/club/dashboard", None, Some(&club)).await;
    assert_eq!(s, StatusCode::OK, "club dashboard: {body}");
    assert_eq!(body["requests_today"], 0);

    let (s, body) = call(&env, "GET", "/v1/club/profile", None, Some(&club)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["name"], "Panel Club");

    let (s, body) = call(
        &env,
        "PATCH",
        "/v1/club/profile",
        Some(json!({ "address": "Calle Nueva 1", "phone": "+34 600 000 000" })),
        Some(&club),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["address"], "Calle Nueva 1");

    let (s, body) = call(&env, "GET", "/v1/club/djs", None, Some(&club)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["items"].as_array().map(Vec::len), Some(1));

    let (s, body) = call(
        &env,
        "GET",
        "/v1/club/earnings?period=week",
        None,
        Some(&club),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "earnings: {body}");
    assert_eq!(body["period"], "week");

    let (s, body) = call(&env, "GET", "/v1/club/payouts", None, Some(&club)).await;
    assert_eq!(s, StatusCode::OK, "club payouts: {body}");

    // --- админ: dashboard, clubs, блокировка + аудит -------------------
    let (s, body) = call(&env, "GET", "/v1/admin/dashboard/now", None, Some(&admin)).await;
    assert_eq!(s, StatusCode::OK, "admin dashboard: {body}");
    assert!(body["active_sessions"].is_i64());

    let (s, body) = call(&env, "GET", "/v1/admin/clubs", None, Some(&admin)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["items"].as_array().map(Vec::len), Some(1));

    // Блокировка клуба (статус, не delete) + запись в audit_log.
    let (s, body) = call(
        &env,
        "PATCH",
        &format!("/v1/admin/clubs/{club_id}"),
        Some(json!({ "status": "blocked" })),
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "club block: {body}");
    assert_eq!(body["status"], "blocked");

    let (s, body) = call(&env, "GET", "/v1/admin/audit-log", None, Some(&admin)).await;
    assert_eq!(s, StatusCode::OK);
    let events = body["items"].as_array().unwrap();
    assert!(
        events.iter().any(|e| e["action"] == "club.status_change"),
        "audit entry written: {events:?}"
    );

    // --- DJ: shift + stats ----------------------------------------------
    // Сессия для DJ.
    let session = sessions
        .create_session(club_id, Some(dj_id), None, "Europe/Madrid")
        .await
        .unwrap();
    let (s, body) = call(&env, "GET", "/v1/dj/shift", None, Some(&dj)).await;
    assert_eq!(s, StatusCode::OK, "dj shift: {body}");
    assert_eq!(
        body["session"]["id"].as_str().unwrap(),
        session.id.to_string()
    );

    let (s, body) = call(&env, "GET", "/v1/dj/stats/today", None, Some(&dj)).await;
    assert_eq!(s, StatusCode::OK, "dj stats: {body}");
    assert_eq!(body["requests_today"], 0);

    // --- платежи + сплиты (админ) ----------------------------------------
    let (s, body) = call(&env, "GET", "/v1/admin/payments", None, Some(&admin)).await;
    assert_eq!(s, StatusCode::OK, "admin payments: {body}");
    // Сплиты пустого платежа — пустой список (404 если платежа нет).
    let fake = uuid::Uuid::now_v7();
    let (s, _) = call(
        &env,
        "GET",
        &format!("/v1/admin/payments/{fake}/splits"),
        None,
        Some(&admin),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "empty splits ok");

    // --- RBAC: гость/клуб/DJ в админ-роуты → 403 ------------------------
    for token in [&club, &dj] {
        let (s, _) = call(&env, "GET", "/v1/admin/clubs", None, Some(token)).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
    }
    let (s, _) = call(&env, "GET", "/v1/club/dashboard", None, Some(&dj)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "dj forbidden on club route");
}
