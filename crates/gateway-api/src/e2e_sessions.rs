//! E2e-тест F2: сессии, QR, join гостя.
//!
//! Критерий приёмки фазы: скан тест-QR → session info (клуб, DJ,
//! цена); чужой QR / чужая сессия отклонены; повторный join —
//! re-grant, не дубль.
//!
//! Требует живой Postgres (compose). При недоступной БД — пропуск.

use axum::http::StatusCode;
use dropit_infra_db::SessionRepo;
use serde_json::json;

#[tokio::test]
async fn e2e_session_flow() {
    let repo = match crate::e2e::test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e F2 пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let env = crate::e2e::app_env(Some(repo.clone()), Some(sessions.clone())).await;

    // --- Фикстуры: клуб, DJ, сессия, QR --------------------------
    let pool = repo.pool();
    let _ = &env;
    let club_id = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO club (id, name, slug, email, password_hash) VALUES ($1, 'E2E Club', 'e2e-club', 'e2e@club.test', 'no-hash')")
        .bind(club_id)
        .execute(&pool)
        .await
        .unwrap();
    let dj_id = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO dj (id, club_id, email, password_hash, display_name) VALUES ($1, $2, 'e2e@dj.test', 'no-hash', 'DJ Test')")
        .bind(dj_id)
        .bind(club_id)
        .execute(&pool)
        .await
        .unwrap();
    let session = sessions
        .create_session(club_id, Some(dj_id), None, "Europe/Madrid")
        .await
        .unwrap();

    // QR: генерируем через репо (как сделает роут клуба).
    let qr_plain = "e2e".repeat(22); // 66 hex-символов > 64 (128 бит)
    let qr_hash = crate::sessions_routes::hash_qr_for_test(&qr_plain);
    sessions
        .issue_qr_token(
            session.id,
            &qr_hash,
            chrono::Utc::now() + chrono::Duration::hours(6),
        )
        .await
        .unwrap();

    // Гость: device-token.
    let device_id = uuid::Uuid::now_v7();
    repo.create_guest_device(device_id, "ios", Some("e2e"))
        .await
        .unwrap();

    // --- Скан QR → session info (критерий приёмки) ---------------
    let (s, body) = crate::e2e::call(
        &env,
        "POST",
        "/v1/sessions/join",
        Some(json!({ "qr_token": qr_plain })),
        Some(&device_id.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "join: {body}");
    assert_eq!(body["club_name"], "E2E Club");
    assert_eq!(body["dj_display_name"], "DJ Test");
    assert_eq!(body["status"], "created");
    assert_eq!(body["timezone"], "Europe/Madrid");
    let grant_id = body["grant_id"].as_str().unwrap().to_string();
    assert!(!grant_id.is_empty());

    // --- Повторный join = re-grant, не дубль ----------------------
    let (s, body2) = crate::e2e::call(
        &env,
        "POST",
        "/v1/sessions/join",
        Some(json!({ "qr_token": qr_plain })),
        Some(&device_id.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "re-join: {body2}");
    assert_eq!(
        body2["grant_id"], grant_id,
        "same grant row (UNIQUE re-grant)"
    );

    // --- Чужой QR (неизвестный хеш) → 404 -------------------------
    let (s, body) = crate::e2e::call(
        &env,
        "POST",
        "/v1/sessions/join",
        Some(json!({ "qr_token": "ff".repeat(32) })),
        Some(&device_id.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "unknown qr: {body}");
    assert_eq!(body["code"], "not_found");

    // --- Мусорный формат токена → 400 ------------------------------
    let (s, _) = crate::e2e::call(
        &env,
        "POST",
        "/v1/sessions/join",
        Some(json!({ "qr_token": "short" })),
        Some(&device_id.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "bad format");

    // --- Неизвестное устройство (чужой device-token) → 401 ---------
    let (s, _) = crate::e2e::call(
        &env,
        "POST",
        "/v1/sessions/join",
        Some(json!({ "qr_token": qr_plain })),
        Some(&uuid::Uuid::now_v7().to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "unknown device");

    // --- Ротация QR: старый токен умирает --------------------------
    let qr2_plain = "ab".repeat(33);
    sessions
        .issue_qr_token(
            session.id,
            &crate::sessions_routes::hash_qr_for_test(&qr2_plain),
            chrono::Utc::now() + chrono::Duration::hours(6),
        )
        .await
        .unwrap();
    // Старый токен больше не проходит.
    let (s, _) = crate::e2e::call(
        &env,
        "POST",
        "/v1/sessions/join",
        Some(json!({ "qr_token": qr_plain })),
        Some(&device_id.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "rotated qr must fail");
    // Новый — проходит.
    let (s, _) = crate::e2e::call(
        &env,
        "POST",
        "/v1/sessions/join",
        Some(json!({ "qr_token": qr2_plain })),
        Some(&device_id.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "new qr works");

    // --- Закрытая сессия: join отклонён ----------------------------
    sessions
        .apply_transition(session.id, dropit_domain::SessionTransition::Open)
        .await
        .unwrap();
    sessions
        .apply_transition(session.id, dropit_domain::SessionTransition::Close)
        .await
        .unwrap();
    let (s, body) = crate::e2e::call(
        &env,
        "POST",
        "/v1/sessions/join",
        Some(json!({ "qr_token": qr2_plain })),
        Some(&device_id.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "closed session join: {body}");
    assert_eq!(body["code"], "session_closed");

    // --- Чужой session_id: гранта нет → основа F4-гейта -----------
    let grant = sessions
        .find_live_grant(uuid::Uuid::now_v7(), device_id)
        .await
        .unwrap();
    assert!(grant.is_none(), "no grant for foreign session");
    let grant = sessions
        .find_live_grant(session.id, device_id)
        .await
        .unwrap();
    assert!(grant.is_some(), "live grant for own session");
}
