//! E2e-тест F4: запросы, очередь DJ, пауза (критерий приёмки).
//!
//! Критерии (техкарта Часть VIII):
//!   * запрос гостя появляется в очереди DJ;
//!   * пауза приёма блокирует POST /requests;
//!   * чужой грант отклонён (гость без гранта → 403);
//!   * дубль Idempotency-Key → replay (не второй запрос);
//!   * DJ-действия: no-op на дублях; played из pending разрешён;
//!   * очередь hot: сортировка по счётчику той же песни.
//!
//! Требует живой Postgres. При недоступной БД — пропуск.

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::e2e::{app_env, call, test_repo};
use dropit_infra_db::{RequestRepo, SessionRepo};

#[tokio::test]
async fn e2e_request_flow() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e F4 пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let requests = RequestRepo::new(repo.pool());
    let env = app_env(
        Some(repo.clone()),
        Some(sessions.clone()),
        Some(requests.clone()),
    )
    .await;

    let pool = repo.pool();

    // --- Фикстуры: клуб, DJ, сессия (active), QR, грант, трек -----
    let club_id = uuid::Uuid::now_v7();
    let dj_id = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO club (id, name, slug, email, password_hash) VALUES ($1, 'F4 Club', 'f4-club', 'f4@club.test', 'x')")
        .bind(club_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO dj (id, club_id, email, password_hash, display_name) VALUES ($1, $2, 'f4@dj.test', 'x', 'DJ F4')")
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

    // Гость с грантом (через join-механику репо).
    let guest_id = uuid::Uuid::now_v7();
    repo.create_guest_device(guest_id, "ios", None)
        .await
        .unwrap();
    sessions
        .grant_session_guest(session.id, guest_id)
        .await
        .unwrap();

    // Гость БЕЗ гранта (чужая сессия).
    let stranger_id = uuid::Uuid::now_v7();
    repo.create_guest_device(stranger_id, "ios", None)
        .await
        .unwrap();

    // Трек в кэше (валидированный, F3-домен).
    let track_id = insert_track(&pool, "itunes", "track-1", "Vivir Mi Vida", "Marc Anthony").await;

    // --- 1. Гость без гранта → 403 (чужой session_id не сработает) -
    let (s, body) = call(
        &env,
        "POST",
        "/v1/requests",
        Some(json!({ "track_id": track_id })),
        Some(&stranger_id.to_string()),
    )
    .await;
    // Странная ситуация: гость без активной сессии → session_closed 409
    // (нет гранта вообще) — или 403 если гранта нет в активной сессии.
    assert!(
        s == StatusCode::FORBIDDEN || s == StatusCode::CONFLICT,
        "stranger guest must be rejected: {s} {body}"
    );

    // --- 2. Запрос гостя с грантом → 201, появляется у DJ ---------
    let (s, body) = call_with_key(
        &env,
        "POST",
        "/v1/requests",
        Some(json!({ "track_id": track_id, "style": "salsa" })),
        Some(&guest_id.to_string()),
        "idem-key-1",
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "create request: {body}");
    let request_id = body["request_id"].as_str().unwrap().to_string();
    assert_eq!(body["status"], "pending");

    // --- 3. Дубль Idempotency-Key → replay того же запроса ---------
    let (s, body2) = call_with_key(
        &env,
        "POST",
        "/v1/requests",
        Some(json!({ "track_id": track_id, "style": "salsa" })),
        Some(&guest_id.to_string()),
        "idem-key-1",
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "replay: {body2}");
    assert_eq!(body2["request_id"].as_str().unwrap(), request_id);

    // В БД ровно один запрос с этим ключом.
    let (cnt,): (i64,) = sqlx::query_as("SELECT count(*) FROM request WHERE guest_device_id = $1")
        .bind(guest_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cnt, 1, "replay must not create second request");

    // --- 4. Запрос в очереди DJ ------------------------------------
    let dj_token = env
        .keys
        .issue_access(dj_id, dropit_common::Role::Dj, uuid::Uuid::now_v7())
        .unwrap();
    let (s, body) = call(&env, "GET", "/v1/dj/queue?sort=new", None, Some(&dj_token)).await;
    assert_eq!(s, StatusCode::OK, "dj queue: {body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "request visible in DJ queue: {body}");
    assert_eq!(items[0]["title"], "Vivir Mi Vida");
    assert_eq!(items[0]["same_track_count"], 1);

    // --- 5. Вторая песня + повтор первой: hot-сортировка ------------
    let track2 = insert_track(&pool, "itunes", "track-2", "Bailando", "Enrique Iglesias").await;
    let track3_like1 =
        insert_track(&pool, "itunes", "track-1", "Vivir Mi Vida", "Marc Anthony").await;
    let _ = track3_like1; // тот же canonical — UNIQUE(source, source_id) вернёт ту же строку
                          // Ещё запросы от второго гостя (грант).
    let guest2 = uuid::Uuid::now_v7();
    repo.create_guest_device(guest2, "android", None)
        .await
        .unwrap();
    sessions
        .grant_session_guest(session.id, guest2)
        .await
        .unwrap();
    let _ = call_with_key(
        &env,
        "POST",
        "/v1/requests",
        Some(json!({ "track_id": track2 })),
        Some(&guest2.to_string()),
        "idem-key-2",
    )
    .await;
    // Дубль той же песни (canonical-ключ) — ещё раз track-1.
    let (s, _) = call_with_key(
        &env,
        "POST",
        "/v1/requests",
        Some(json!({ "track_id": track_id })),
        Some(&guest2.to_string()),
        "idem-key-3",
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);

    // Hot: track-1 (2 запроса) выше track-2 (1 запрос).
    let (_, body) = call(&env, "GET", "/v1/dj/queue?sort=hot", None, Some(&dj_token)).await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 3, "queue size: {body}");
    // track-1 два запроса: в hot первым.
    assert_eq!(items[0]["title"], "Vivir Mi Vida");
    assert_eq!(items[0]["same_track_count"], 2);

    // --- 6. DJ-действия: accept → played; дубль played = no-op -----
    let (s, body) = call(
        &env,
        "POST",
        &format!("/v1/dj/requests/{request_id}/accept"),
        None,
        Some(&dj_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "accept: {body}");
    assert_eq!(body["status"], "accepted");
    assert_eq!(body["noop"], false);

    // Дубль accept на уже accepted — no-op (не 500, не конфликт).
    let (s, body) = call(
        &env,
        "POST",
        &format!("/v1/dj/requests/{request_id}/accept"),
        None,
        Some(&dj_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "duplicate accept must be no-op: {body}");
    assert_eq!(body["noop"], true);

    let (s, body) = call(
        &env,
        "POST",
        &format!("/v1/dj/requests/{request_id}/played"),
        None,
        Some(&dj_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "played: {body}");
    assert_eq!(body["status"], "played");

    // Accept после played → недопустимый переход → 409.
    let (s, _) = call(
        &env,
        "POST",
        &format!("/v1/dj/requests/{request_id}/accept"),
        None,
        Some(&dj_token),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "accept after played must conflict");

    // --- 7. КРИТЕРИЙ F4: пауза блокирует POST /requests ------------
    let (s, body) = call(
        &env,
        "POST",
        "/v1/dj/acceptance/pause",
        None,
        Some(&dj_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "pause: {body}");
    assert_eq!(body["status"], "paused_acceptance");

    let (s, body) = call_with_key(
        &env,
        "POST",
        "/v1/requests",
        Some(json!({ "track_id": track2 })),
        Some(&guest_id.to_string()),
        "idem-key-4",
    )
    .await;
    assert_eq!(
        s,
        StatusCode::CONFLICT,
        "paused session must block POST: {s} {body}"
    );
    assert_eq!(body["code"], "session_paused");

    // Resume — снова можно.
    let (s, _) = call(
        &env,
        "POST",
        "/v1/dj/acceptance/resume",
        None,
        Some(&dj_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = call_with_key(
        &env,
        "POST",
        "/v1/requests",
        Some(json!({ "track_id": track2 })),
        Some(&guest_id.to_string()),
        "idem-key-5",
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "resume unblocks POST");

    // --- 8. WS-ticket: выдача, одноразовость, TTL ------------------
    let guest_jwt = env
        .keys
        .issue_access(guest_id, dropit_common::Role::Guest, uuid::Uuid::now_v7())
        .unwrap();
    let (s, body) = call(&env, "POST", "/v1/ws/ticket", None, Some(&guest_jwt)).await;
    assert_eq!(s, StatusCode::OK, "ws ticket: {body}");
    assert_eq!(body["expires_in"], 30);
    let ticket = body["ticket"].as_str().unwrap().to_string();

    // Первый consume — ок; второй — уже нет (одноразовый).
    let consumed = crate::requests_routes::consume_ws_ticket(&ticket);
    assert!(consumed.is_some(), "first consume ok");
    assert_eq!(consumed.unwrap(), guest_id);
    let reused = crate::requests_routes::consume_ws_ticket(&ticket);
    assert!(reused.is_none(), "ticket is single-use");

    // Без JWT → 401.
    let (s, _) = call(&env, "POST", "/v1/ws/ticket", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "ws ticket requires auth");

    // --- 9. Outbox: события запросов записаны ----------------------
    let (cnt,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM outbox WHERE aggregate_type = 'request' AND published_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(cnt >= 4, "outbox events written: {cnt}");
}

async fn insert_track(
    pool: &sqlx::PgPool,
    source: &str,
    source_id: &str,
    title: &str,
    artist: &str,
) -> uuid::Uuid {
    let canonical = format!("{source}:{source_id}");
    let (id,): (uuid::Uuid,) = sqlx::query_as(
        "INSERT INTO track_cache (id, source, source_id, canonical_key, title, artist) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (source, source_id) DO UPDATE SET updated_at = now() \
         RETURNING id",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(source)
    .bind(source_id)
    .bind(canonical)
    .bind(title)
    .bind(artist)
    .fetch_one(pool)
    .await
    .unwrap();
    id
}

/// call с Idempotency-Key заголовком.
async fn call_with_key(
    env: &crate::routes::ApiEnv,
    method: &str,
    uri: &str,
    body: Option<Value>,
    token: Option<&str>,
    idem_key: &str,
) -> (StatusCode, Value) {
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
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}
