//! E2e-тест F8: антифрод, джобы, шифрование.
//!
//! Критерии:
//!   * антифрод-лимит: 21-й запрос устройства → 429 (лимит 20);
//!   * TTL-воркер: intent старше 15 мин → cancelling;
//!   * авто-закрытие: достигнут плановый час + неактивность → closed,
//!     pending-запросы погашены в expired;
//!   * GDPR: анонимизация — финансы НЕ удалены, гранты ревок,
//!     платформа затёрта; retention-sweep по last_seen;
//!   * TOTP: шифрование roundtrip через crypto-модуль.

use axum::http::StatusCode;
use serde_json::json;

use crate::e2e::{app_env, test_repo};
use dropit_infra_db::{BillingRepo, JobsRepo, RequestRepo, SessionRepo};

#[tokio::test]
async fn e2e_f8_hardening() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e F8 пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let requests = RequestRepo::new(repo.pool());
    let billing = BillingRepo::new(repo.pool());
    let jobs = JobsRepo::new(repo.pool());
    let env = app_env(
        Some(repo.clone()),
        Some(sessions.clone()),
        Some(requests.clone()),
    )
    .await;

    let pool = repo.pool();

    // --- Фикстуры ---------------------------------------------------
    let club_id = uuid::Uuid::now_v7();
    let dj_id = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO club (id, name, slug, email, password_hash) VALUES ($1, 'F8', 'f8', 'f8@club.test', 'x')")
        .bind(club_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO dj (id, club_id, email, password_hash, display_name) VALUES ($1, $2, 'f8@dj.test', 'x', 'DJ')")
        .bind(dj_id)
        .bind(club_id)
        .execute(&pool)
        .await
        .unwrap();
    let session = sessions
        .create_session(club_id, Some(dj_id), None, "Europe/Madrid")
        .await
        .unwrap();
    sessions
        .apply_transition(session.id, dropit_domain::SessionTransition::Open)
        .await
        .unwrap();
    let guest_id = uuid::Uuid::now_v7();
    repo.create_guest_device(guest_id, "ios", None)
        .await
        .unwrap();
    sessions
        .grant_session_guest(session.id, guest_id)
        .await
        .unwrap();
    let (track_id,): (uuid::Uuid,) =
        sqlx::query_as("INSERT INTO track_cache (id, source, source_id, canonical_key, title, artist) VALUES ($1, 'itunes', 'f8-tr', 'itunes:f8-tr', 'T', 'A') RETURNING id")
            .bind(uuid::Uuid::now_v7())
            .fetch_one(&pool)
            .await
            .unwrap();

    // --- Антифрод: 20 запросов ок, 21-й → 429 ------------------------
    let mut last_ok = None;
    for i in 0..21i32 {
        let (s, _) = call_with_key(
            &env,
            "POST",
            "/v1/requests",
            Some(json!({ "track_id": track_id })),
            Some(&guest_id.to_string()),
            &format!("f8-idem-{i}"),
        )
        .await;
        if i < 20 {
            assert_eq!(s, StatusCode::CREATED, "request #{i} must pass");
            last_ok = Some(i);
        } else {
            assert_eq!(
                s,
                StatusCode::TOO_MANY_REQUESTS,
                "request #21 (index {i}) must be 429"
            );
        }
    }
    assert_eq!(last_ok, Some(19));

    // --- TTL-интенты: старый intent → cancelling ---------------------
    let request = requests
        .create_request(session.id, guest_id, track_id, None, "f8-ttl-req")
        .await
        .unwrap()
        .expect("created");
    let payment = billing
        .create_payment(
            request.id,
            500,
            "f8-ttl-key",
            "pi_mock_f8",
            &dropit_infra_db::CommissionParams {
                commission_bp: 3000,
                dj_share_bp: 5000,
                currency: "EUR".into(),
            },
        )
        .await
        .unwrap();
    // Отправляем интент в прошлое (симуляция 15 мин).
    sqlx::query("UPDATE payment SET created_at = now() - interval '16 minutes' WHERE id = $1")
        .bind(payment.id)
        .execute(&pool)
        .await
        .unwrap();
    let expired = jobs.expire_stale_intents().await.unwrap();
    assert_eq!(expired, vec![payment.id], "stale intent → cancelling");
    let p = billing.find_payment(payment.id).await.unwrap().unwrap();
    assert_eq!(p.status, "cancelling");

    // --- Авто-закрытие сессии (07:00 + неактивность) -----------------
    // QR с плановым закрытием в прошлом + запрос давний → закрыть.
    let past = chrono::Utc::now() - chrono::Duration::hours(3);
    sessions
        .issue_qr_token(session.id, &"f8hash".repeat(8), past)
        .await
        .unwrap();
    sqlx::query("UPDATE request SET created_at = now() - interval '3 hours' WHERE session_id = $1")
        .bind(session.id)
        .execute(&pool)
        .await
        .unwrap();
    let closed: Vec<dropit_infra_db::ClosedSession> = jobs.auto_close_sessions().await.unwrap();
    assert!(
        closed.iter().any(|c| c.session_id == session.id),
        "session auto-closed: {closed:?}"
    );
    let s = sessions.find_session(session.id).await.unwrap().unwrap();
    assert_eq!(s.status, "closed");
    // Pending-запросы погашены.
    let (pending,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM request WHERE session_id = $1 AND status = 'pending'")
            .bind(session.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pending, 0, "pending requests expired after close");

    // --- GDPR: анонимизация -------------------------------------------
    let anon_guest = uuid::Uuid::now_v7();
    repo.create_guest_device(anon_guest, "android", Some("9.9"))
        .await
        .unwrap();
    // Финансовая запись гостя (создаётся через репо — остаётся).
    let session2 = sessions
        .create_session(club_id, Some(dj_id), None, "Europe/Madrid")
        .await
        .unwrap();
    sessions
        .apply_transition(session2.id, dropit_domain::SessionTransition::Open)
        .await
        .unwrap();
    sessions
        .grant_session_guest(session2.id, anon_guest)
        .await
        .unwrap();
    let fin_request = requests
        .create_request(session2.id, anon_guest, track_id, None, "f8-gdpr")
        .await
        .unwrap()
        .expect("created");
    let fin_payment = billing
        .create_payment(
            fin_request.id,
            500,
            "f8-gdpr-key",
            "pi_gdpr",
            &dropit_infra_db::CommissionParams {
                commission_bp: 3000,
                dj_share_bp: 5000,
                currency: "EUR".into(),
            },
        )
        .await
        .unwrap();

    // Экспорт DSR до анонимизации.
    let export = jobs.export_guest_data(anon_guest).await.unwrap();
    assert_eq!(export["requests"].as_array().map(Vec::len), Some(1));
    assert_eq!(export["payments"].as_array().map(Vec::len), Some(1));

    // Анонимизация: финансы НЕ удалены, устройство затёрто, грант ревок.
    let n = jobs.anonymize_guest_device(anon_guest, None).await.unwrap();
    assert_eq!(n, 1);
    let (platform, app): (String, Option<String>) =
        sqlx::query_as("SELECT platform, app_version FROM guest_device WHERE id = $1")
            .bind(anon_guest)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(platform, "anonymized");
    assert!(app.is_none());
    let (grant_alive,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM session_guest WHERE guest_device_id = $1 AND revoked_at IS NULL",
    )
    .bind(anon_guest)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(grant_alive, 0, "grants revoked");
    // Финансовая запись на месте.
    let p = billing.find_payment(fin_payment.id).await.unwrap().unwrap();
    assert_eq!(p.status, "intent", "financial record kept");

    // Retention-sweep: устройство неактивно 181 день → затирается.
    let stale_guest = uuid::Uuid::now_v7();
    repo.create_guest_device(stale_guest, "ios", None)
        .await
        .unwrap();
    sqlx::query("UPDATE guest_device SET last_seen = now() - interval '181 days' WHERE id = $1")
        .bind(stale_guest)
        .execute(&pool)
        .await
        .unwrap();
    let swept = jobs.retention_sweep().await.unwrap();
    assert!(swept.contains(&stale_guest), "retention sweep: {swept:?}");

    // --- TOTP: шифрование roundtrip (модуль, Test Mode) --------------
    // Ключ задаётся тестом в common; здесь — контракт: v2-формат.
    std::env::set_var("DROPIT_TOTP_KEY", "11".repeat(32));
    let secret = "JBSWY3DPEHPK3PXP";
    let enc = dropit_common::encrypt_totp_secret(secret).unwrap();
    assert!(String::from_utf8(enc.clone()).unwrap().starts_with("v2:"));
    let dec = dropit_common::decrypt_totp_secret(&enc).unwrap();
    assert_eq!(dec, secret);
    // Легаси-plain читается.
    assert_eq!(
        dropit_common::decrypt_totp_secret(b"PLAINBASE32").unwrap(),
        "PLAINBASE32"
    );
    std::env::remove_var("DROPIT_TOTP_KEY");

    // --- Антифрод: лимит устройств — алерт, не запрет ------------------
    let alert = jobs.session_devices_alert(session2.id).await.unwrap();
    // Устройств мало → None (не алерт).
    assert!(alert.is_none());
    // Лимит 300: создаём счётчик напрямую — домен уже покрыт тестами,
    // здесь контракт: метод не падает и возвращает Option.
}

/// call с Idempotency-Key.
async fn call_with_key(
    env: &crate::routes::ApiEnv,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
    token: Option<&str>,
    idem_key: &str,
) -> (StatusCode, serde_json::Value) {
    use axum::body::Body;
    use tower::ServiceExt;

    let router = crate::routes::router(env.clone());
    let mut req = axum::http::Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    req = req.header("idempotency-key", idem_key);
    let req = match body {
        Some(b) => req
            .header("content-type", "application/json")
            .body(Body::from(b.to_string()))
            .unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap_or_default();
    let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, value)
}
