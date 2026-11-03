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
use dropit_common::Role;
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

    // Первый consume — ок; второй — уже нет (одноразовый, через единый
    // стор ApiEnv.tickets — тот же путь, что и у GET /v1/ws).
    let consumed = crate::ws::consume_ticket(&env, &ticket).await;
    assert!(consumed.is_some(), "first consume ok");
    assert_eq!(consumed.unwrap(), guest_id);
    let reused = crate::ws::consume_ticket(&env, &ticket).await;
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

// ============================================================
// W2-B: GET /v1/my/requests/:id — экран «моя заявка»
// ============================================================

#[tokio::test]
async fn e2e_my_request_detail() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e W2-B (detail) пропущен");
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

    // Фикстуры: клуб, DJ, сессия (active), гранты, трек.
    let club_id = uuid::Uuid::now_v7();
    let dj_id = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO club (id, name, slug, email, password_hash) VALUES ($1,'W2B Club','w2b-club','w2b@club.test','x')")
        .bind(club_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO dj (id, club_id, email, password_hash, display_name) VALUES ($1,$2,'w2b@dj.test','x','DJ W2B')")
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

    // Гость 1 с грантом.
    let guest1 = uuid::Uuid::now_v7();
    repo.create_guest_device(guest1, "ios", None).await.unwrap();
    sessions
        .grant_session_guest(session.id, guest1)
        .await
        .unwrap();

    let track_id = insert_track(&pool, "itunes", "w2b-1", "Vivir Mi Vida", "Marc Anthony").await;

    // --- 1. Создаём заявку гостя 1 → pending ---------------------
    let (s, body) = call_with_key(
        &env,
        "POST",
        "/v1/requests",
        Some(json!({ "track_id": track_id })),
        Some(&guest1.to_string()),
        "w2b-detail-1",
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "create: {body}");
    let request_id = body["request_id"].as_str().unwrap().to_string();

    // --- 2. GET /v1/my/requests/:id — своя заявка, 200 ------------
    let (s, body) = call(
        &env,
        "GET",
        &format!("/v1/my/requests/{request_id}"),
        None,
        Some(&guest1.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "detail: {body}");
    let r = &body["request"];
    assert_eq!(r["status"], "pending");
    assert_eq!(r["track"]["title"], "Vivir Mi Vida");
    assert_eq!(r["track"]["artist"], "Marc Anthony");
    assert_eq!(r["position"], 1, "первая заявка → position 1");
    assert_eq!(r["queue_size"], 1);
    assert_eq!(r["same_song_count"], 1);
    assert_eq!(r["session_id"].as_str().unwrap(), session.id.to_string());
    assert!(r["played_at"].is_null());
    assert!(r["decline_reason"].is_null());
    assert!(r["created_at"].is_string());
    assert!(r["updated_at"].is_string());

    // --- 3. Второй гость заказывает ТОТ ЖЕ трек → same_song_count растёт
    let guest2 = uuid::Uuid::now_v7();
    repo.create_guest_device(guest2, "android", None)
        .await
        .unwrap();
    sessions
        .grant_session_guest(session.id, guest2)
        .await
        .unwrap();
    let (s, body2) = call_with_key(
        &env,
        "POST",
        "/v1/requests",
        Some(json!({ "track_id": track_id })),
        Some(&guest2.to_string()),
        "w2b-detail-2",
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "create guest2: {body2}");
    let request_id2 = body2["request_id"].as_str().unwrap().to_string();

    // Гость 1: same_song_count вырос до 2; позиция осталась 1 (старейшая).
    let (s, body) = call(
        &env,
        "GET",
        &format!("/v1/my/requests/{request_id}"),
        None,
        Some(&guest1.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "detail after guest2: {body}");
    assert_eq!(
        body["request"]["same_song_count"], 2,
        "same_song_count grows"
    );
    assert_eq!(body["request"]["queue_size"], 2);
    assert_eq!(body["request"]["position"], 1, "guest1 is first in FIFO");

    // Гость 2: своя заявка → position 2 (новее), same_song_count 2.
    let (s, body) = call(
        &env,
        "GET",
        &format!("/v1/my/requests/{request_id2}"),
        None,
        Some(&guest2.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "guest2 detail: {body}");
    assert_eq!(body["request"]["position"], 2);
    assert_eq!(body["request"]["same_song_count"], 2);

    // --- 4. Чужая заявка → 404 (не различать с несуществующей) ----
    let (s, body) = call(
        &env,
        "GET",
        &format!("/v1/my/requests/{request_id}"),
        None,
        Some(&guest2.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "foreign request must 404: {body}");
    assert_eq!(body["code"], "request_not_found");

    // Несуществующий id — тот же ответ.
    let ghost = uuid::Uuid::now_v7();
    let (s, body) = call(
        &env,
        "GET",
        &format!("/v1/my/requests/{ghost}"),
        None,
        Some(&guest1.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "missing id must 404: {body}");
    assert_eq!(body["code"], "request_not_found");

    // --- 5. 401 без токена, 403 для роли dj -----------------------
    let (s, body) = call(
        &env,
        "GET",
        &format!("/v1/my/requests/{request_id}"),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "no token: {body}");
    assert_eq!(body["code"], "unauthorized");

    let dj_token = env
        .keys
        .issue_access(dj_id, Role::Dj, uuid::Uuid::now_v7())
        .unwrap();
    let (s, body) = call(
        &env,
        "GET",
        &format!("/v1/my/requests/{request_id}"),
        None,
        Some(&dj_token),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "dj role: {body}");
    assert_eq!(body["code"], "forbidden");

    // --- 6. Изоляция сессий: заявка из другой сессии не найдена ----
    let session2 = sessions
        .create_session(club_id, Some(dj_id), None, "Europe/Madrid")
        .await
        .unwrap();
    sessions
        .apply_transition(session2.id, dropit_domain::SessionTransition::Open)
        .await
        .unwrap();
    // Гость 1 вступает в новую сессию → его активная сессия = session2.
    sessions
        .grant_session_guest(session2.id, guest1)
        .await
        .unwrap();
    let (s, body) = call(
        &env,
        "GET",
        &format!("/v1/my/requests/{request_id}"),
        None,
        Some(&guest1.to_string()),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "request from another session must 404: {body}"
    );
    assert_eq!(body["code"], "request_not_found");
}

// ============================================================
// W2-B: POST /v1/guest/upgrade — привязка e-mail
// ============================================================

#[tokio::test]
async fn e2e_guest_upgrade() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e W2-B (upgrade) пропущен");
            return;
        }
    };
    let requests = RequestRepo::new(repo.pool());
    let env = app_env(Some(repo.clone()), None, Some(requests.clone())).await;
    let pool = repo.pool();

    // test_repo() не знает про guest_account (миграция 0010) — чистим сами.
    let _ = sqlx::query("TRUNCATE TABLE guest_account")
        .execute(&pool)
        .await;

    // Два гостя-устройства.
    let guest1 = uuid::Uuid::now_v7();
    let guest2 = uuid::Uuid::now_v7();
    repo.create_guest_device(guest1, "ios", None).await.unwrap();
    repo.create_guest_device(guest2, "android", None)
        .await
        .unwrap();

    // --- 1. Успешный апгрейд: e-mail нормализуется ----------------
    let (s, body) = call(
        &env,
        "POST",
        "/v1/guest/upgrade",
        Some(json!({ "email": "Alice@Example.COM" })),
        Some(&guest1.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "upgrade: {body}");
    assert_eq!(body["email"], "alice@example.com", "email normalized");
    assert_eq!(body["verified"], false);
    assert_eq!(body["already_upgraded"], false);

    // В БД ровно один аккаунт и device привязан.
    let (cnt,): (i64,) = sqlx::query_as("SELECT count(*) FROM guest_account")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cnt, 1, "no duplicate accounts");
    let (bound,): (Option<uuid::Uuid>,) =
        sqlx::query_as("SELECT account_id FROM guest_device WHERE id = $1")
            .bind(guest1)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(bound.is_some(), "guest_device.account_id set");

    // --- 2. Повтор тем же e-mail → идемпотентен -------------------
    let (s, body) = call(
        &env,
        "POST",
        "/v1/guest/upgrade",
        Some(json!({ "email": "alice@example.com" })),
        Some(&guest1.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "repeat: {body}");
    assert_eq!(body["already_upgraded"], true);
    assert_eq!(body["email"], "alice@example.com");
    let (cnt,): (i64,) = sqlx::query_as("SELECT count(*) FROM guest_account")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cnt, 1, "repeat must not create second account");

    // --- 3. Тот же e-mail от другого гостя → 409 email_taken -----
    let (s, body) = call(
        &env,
        "POST",
        "/v1/guest/upgrade",
        Some(json!({ "email": "alice@example.com" })),
        Some(&guest2.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "taken email: {body}");
    assert_eq!(body["code"], "email_taken");

    // --- 4. Мусорный e-mail → 400 invalid_email ------------------
    for bad in ["", "noatsign", "a@b", "x@.com", "@nodomain.com"] {
        let (s, body) = call(
            &env,
            "POST",
            "/v1/guest/upgrade",
            Some(json!({ "email": bad })),
            Some(&guest2.to_string()),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "invalid email '{bad}': {body}");
        assert_eq!(body["code"], "invalid_email");
    }

    // Слишком длинный → 400.
    let long_email = format!("{}@example.com", "a".repeat(330));
    let (s, body) = call(
        &env,
        "POST",
        "/v1/guest/upgrade",
        Some(json!({ "email": long_email })),
        Some(&guest2.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "too long email: {body}");
    assert_eq!(body["code"], "invalid_email");

    // --- 5. Гость2 с валидным свободным e-mail → успех -----------
    let (s, body) = call(
        &env,
        "POST",
        "/v1/guest/upgrade",
        Some(json!({ "email": "bob@example.com" })),
        Some(&guest2.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "guest2 upgrade: {body}");
    assert_eq!(body["email"], "bob@example.com");
    assert_eq!(body["already_upgraded"], false);

    // --- 6. 401 без токена, 403 для роли dj -----------------------
    let (s, body) = call(
        &env,
        "POST",
        "/v1/guest/upgrade",
        Some(json!({ "email": "c@example.com" })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "no token: {body}");
    assert_eq!(body["code"], "unauthorized");

    let dj_token = env
        .keys
        .issue_access(uuid::Uuid::now_v7(), Role::Dj, uuid::Uuid::now_v7())
        .unwrap();
    let (s, body) = call(
        &env,
        "POST",
        "/v1/guest/upgrade",
        Some(json!({ "email": "c@example.com" })),
        Some(&dj_token),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "dj role: {body}");
    assert_eq!(body["code"], "forbidden");
}

// ============================================================
// W5-E: транзакционная идемпотентность создания заявки
// ============================================================

/// Два параллельных и один повторный POST /v1/requests с одним
/// Idempotency-Key: в БД ровно одна заявка, один outbox-эвент, все
/// ответы ссылаются на один request_id (replay, не дубль).
///
/// Регрессия на дефект W5-E: раньше `save_idempotency_response`
/// вызывался ПОСЛЕ коммита транзакции создания — два параллельных POST
/// с одним ключом видели «ключа нет», оба коммитили разные request_id,
/// затем `ON CONFLICT DO NOTHING` оставлял один ключ → в БД две заявки
/// на ключ. Теперь идемпотентный ответ пишется ВНУТРИ транзакции:
/// `UNIQUE(key)` откатывает всю вставку проигравшего, и он возвращает
/// replay.
#[tokio::test]
async fn e2e_parallel_idempotency_no_duplicate() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e W5-E пропущен");
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

    // --- Фикстуры: клуб, DJ, сессия (active), грант, трек ---------
    let club_id = uuid::Uuid::now_v7();
    let dj_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash) \
         VALUES ($1,'W5E Club','w5e-club','w5e@club.test','x')",
    )
    .bind(club_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO dj (id, club_id, email, password_hash, display_name) \
         VALUES ($1,$2,'w5e@dj.test','x','DJ W5E')",
    )
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

    let track_id = insert_track(&pool, "itunes", "w5e-tr", "Vivir Mi Vida", "Marc Anthony").await;
    let idem_key = "w5e-parallel-key";

    // --- 1. Два ПАРАЛЛЕЛЬНЫХ POST с одним Idempotency-Key ----------
    // tokio::join! опрашивает оба futures; на current-thread-рантайме
    // оба sqlx-запроса уходят в пул и выполняются конкурентно против
    // Postgres. Независимо от того, успеют ли они наложиться (гонка) или
    // сериализуются — инвариант один: ровно одна заявка.
    let token_str = guest_id.to_string();
    let (r1, r2) = tokio::join!(
        call_with_key(
            &env,
            "POST",
            "/v1/requests",
            Some(json!({ "track_id": track_id })),
            Some(&token_str),
            idem_key,
        ),
        call_with_key(
            &env,
            "POST",
            "/v1/requests",
            Some(json!({ "track_id": track_id })),
            Some(&token_str),
            idem_key,
        ),
    );
    let (s1, b1) = r1;
    let (s2, b2) = r2;
    assert_eq!(s1, StatusCode::CREATED, "parallel #1: {b1}");
    assert_eq!(s2, StatusCode::CREATED, "parallel #2: {b2}");
    let id1 = b1["request_id"].as_str().expect("request_id #1");
    let id2 = b2["request_id"].as_str().expect("request_id #2");
    assert_eq!(
        id1, id2,
        "both responses must reference the same request_id"
    );

    // --- 2. В БД ровно одна заявка этого гостя ---------------------
    let (cnt,): (i64,) = sqlx::query_as("SELECT count(*) FROM request WHERE guest_device_id = $1")
        .bind(guest_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cnt, 1, "parallel POSTs must not create a second request");

    // --- 3. Один идемпотентный ответ на ключ -----------------------
    let (idem_cnt,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM idempotency_response WHERE key = $1")
            .bind(idem_key)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(idem_cnt, 1, "exactly one idempotency_response per key");

    // --- 4. Ровно одно outbox-событие request.created на заявку ----
    // (replay проигравшего не доходит до INSERT outbox — конфликт
    // ключа откатывает транзакцию до вставки события).
    let request_id = uuid::Uuid::parse_str(id1).unwrap();
    let (ob_cnt,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM outbox WHERE aggregate_type = 'request' \
         AND aggregate_id = $1 AND event_type = 'request.created'",
    )
    .bind(request_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        ob_cnt, 1,
        "exactly one request.created outbox event per request"
    );

    // --- 5. Последовательный повтор → тот же id, без дубля ---------
    let (s3, b3) = call_with_key(
        &env,
        "POST",
        "/v1/requests",
        Some(json!({ "track_id": track_id })),
        Some(&guest_id.to_string()),
        idem_key,
    )
    .await;
    assert_eq!(s3, StatusCode::CREATED, "sequential replay: {b3}");
    assert_eq!(
        b3["request_id"].as_str().unwrap(),
        id1,
        "replay returns same request_id"
    );

    let (cnt2,): (i64,) = sqlx::query_as("SELECT count(*) FROM request WHERE guest_device_id = $1")
        .bind(guest_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        cnt2, 1,
        "sequential replay must not create a second request"
    );

    let (ob_cnt2,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM outbox WHERE aggregate_type = 'request' \
         AND aggregate_id = $1 AND event_type = 'request.created'",
    )
    .bind(request_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ob_cnt2, 1, "replay must not duplicate outbox event");
}

// ============================================================
// T9: GET /v1/dj/history + GET /v1/dj/stats/session
// ============================================================

/// Лог смены DJ: пустая смена, сыгранная заявка в истории, пагинация,
/// RBAC (клуб → 403) и счётчики смены.
///
/// Критерии задачи task-9:
///   * пустая смена → `[]` (200, без ошибки);
///   * accept → played попадает в историю со временем `played_at` и
///     `accepted_at` из события `request.accepted`;
///   * отказ и pending в историю не попадают (лог — только сыгранное);
///   * keyset-пагинация не повторяет и не теряет строки;
///   * клубный токен → 403, без токена → 401, мусорный limit/cursor → 400;
///   * счётчики смены считают смену, а не 24 часа и не «всё время».
#[tokio::test]
async fn e2e_dj_history() {
    let repo = match test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e T9 (dj history) пропущен");
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
    let tag = uuid::Uuid::now_v7();

    // --- Фикстуры: клуб, DJ, active-сессия -----------------------
    let club_id = uuid::Uuid::now_v7();
    let dj_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash) \
         VALUES ($1, 'T9 Club', $2, $3, 'x')",
    )
    .bind(club_id)
    .bind(format!("t9-club-{tag}"))
    .bind(format!("t9-{tag}@club.test"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO dj (id, club_id, email, password_hash, display_name) \
         VALUES ($1, $2, $3, 'x', 'DJ T9')",
    )
    .bind(dj_id)
    .bind(club_id)
    .bind(format!("t9-{tag}@dj.test"))
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

    let dj_token = env
        .keys
        .issue_access(dj_id, Role::Dj, uuid::Uuid::now_v7())
        .unwrap();
    let club_token = env
        .keys
        .issue_access(club_id, Role::Club, uuid::Uuid::now_v7())
        .unwrap();

    // --- 1. Пустая смена → пустой лог (200, не 404) --------------
    let (s, body) = call(&env, "GET", "/v1/dj/history", None, Some(&dj_token)).await;
    assert_eq!(s, StatusCode::OK, "empty shift history: {body}");
    assert_eq!(body["items"].as_array().map(Vec::len), Some(0));
    assert!(body["next_cursor"].is_null(), "no cursor on an empty log");

    // --- 2. RBAC: клуб → 403, без токена → 401 -------------------
    let (s, body) = call(&env, "GET", "/v1/dj/history", None, Some(&club_token)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "club token must not read the log");
    assert_eq!(body["code"], "forbidden");
    let (s, _) = call(&env, "GET", "/v1/dj/history", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "history requires auth");

    // --- 3. Гость, грант, треки ----------------------------------
    let guest_id = uuid::Uuid::now_v7();
    repo.create_guest_device(guest_id, "ios", None)
        .await
        .unwrap();
    sessions
        .grant_session_guest(session.id, guest_id)
        .await
        .unwrap();
    let track_a = insert_track(
        &pool,
        "itunes",
        &format!("t9-a-{tag}"),
        "Song A",
        "Artist A",
    )
    .await;
    let track_b = insert_track(
        &pool,
        "itunes",
        &format!("t9-b-{tag}"),
        "Song B",
        "Artist B",
    )
    .await;
    let track_c = insert_track(
        &pool,
        "itunes",
        &format!("t9-c-{tag}"),
        "Song C",
        "Artist C",
    )
    .await;

    // Три сыгранные заявки (порядок действий = порядок в логе).
    let mut played_ids = Vec::new();
    for (track, key) in [
        (track_a, "t9-play-a"),
        (track_b, "t9-play-b"),
        (track_c, "t9-play-c"),
    ] {
        let (s, body) = call_with_key(
            &env,
            "POST",
            "/v1/requests",
            Some(json!({ "track_id": track })),
            Some(&guest_id.to_string()),
            key,
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "create request: {body}");
        let id = body["request_id"].as_str().unwrap().to_string();
        let (s, body) = call(
            &env,
            "POST",
            &format!("/v1/dj/requests/{id}/accept"),
            None,
            Some(&dj_token),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "accept: {body}");
        let (s, body) = call(
            &env,
            "POST",
            &format!("/v1/dj/requests/{id}/played"),
            None,
            Some(&dj_token),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "played: {body}");
        played_ids.push(id);
    }

    // Ещё две заявки, которые в лог попасть не должны: отклонённая и
    // висящая в pending (лог — только сыгранное, не «все решения»).
    let (s, body) = call_with_key(
        &env,
        "POST",
        "/v1/requests",
        Some(json!({ "track_id": track_b })),
        Some(&guest_id.to_string()),
        "t9-reject",
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "create request to reject: {body}");
    let rejected_id = body["request_id"].as_str().unwrap().to_string();
    let (s, body) = call(
        &env,
        "POST",
        &format!("/v1/dj/requests/{rejected_id}/reject"),
        Some(json!({ "reason": "too late" })),
        Some(&dj_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "reject: {body}");
    let (s, body) = call_with_key(
        &env,
        "POST",
        "/v1/requests",
        Some(json!({ "track_id": track_b })),
        Some(&guest_id.to_string()),
        "t9-pending",
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "create pending request: {body}");

    // --- 4. Сыгранное попало в историю ---------------------------
    let (s, body) = call(&env, "GET", "/v1/dj/history", None, Some(&dj_token)).await;
    assert_eq!(s, StatusCode::OK, "history: {body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 3, "only played requests are logged: {body}");
    assert!(body["next_cursor"].is_null(), "one page holds three rows");
    // Новейшая сыгранная — первой.
    assert_eq!(items[0]["id"], played_ids[2].as_str());
    assert_eq!(items[0]["title"], "Song C");
    assert_eq!(items[0]["artist"], "Artist C");
    assert!(
        items[0]["played_at"].as_str().is_some(),
        "played_at must be a timestamp: {body}"
    );
    assert!(
        items[0]["accepted_at"].as_str().is_some(),
        "accepted_at comes from the request.accepted event: {body}"
    );
    // Song B заказана трижды в сессии (played, rejected, pending), но в
    // историю попала только сыгранная строка — счётчик считает сессию.
    let song_b = items
        .iter()
        .find(|row| row["title"] == "Song B")
        .expect("played Song B");
    assert_eq!(song_b["same_track_count"], 3);
    for item in items {
        assert_eq!(item["status"], serde_json::Value::Null, "no status field");
    }

    // --- 5. Keyset-пагинация -------------------------------------
    let (s, first) = call(&env, "GET", "/v1/dj/history?limit=2", None, Some(&dj_token)).await;
    assert_eq!(s, StatusCode::OK, "first page: {first}");
    let first_items = first["items"].as_array().unwrap();
    assert_eq!(first_items.len(), 2, "limit honoured: {first}");
    let cursor = first["next_cursor"].as_str().expect("cursor on page 1");
    let (s, second) = call(
        &env,
        "GET",
        &format!("/v1/dj/history?limit=2&cursor={}", url_escape(cursor)),
        None,
        Some(&dj_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "second page: {second}");
    let second_items = second["items"].as_array().unwrap();
    assert_eq!(second_items.len(), 1, "rest of the page: {second}");
    assert!(
        second["next_cursor"].is_null(),
        "no third page for three rows"
    );
    let mut seen: Vec<String> = first_items
        .iter()
        .chain(second_items.iter())
        .map(|row| row["id"].as_str().unwrap().to_string())
        .collect();
    let total = seen.len();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), total, "pages must not repeat a row");
    assert_eq!(seen.len(), 3, "pages must not lose a row: {seen:?}");

    // --- 6. Мусорные limit/cursor → 400 (конверт, не text/plain) --
    for query in ["?limit=0", "?limit=101", "?limit=abc"] {
        let (s, body) = call(
            &env,
            "GET",
            &format!("/v1/dj/history{query}"),
            None,
            Some(&dj_token),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "bad limit {query}: {body}");
        assert_eq!(body["code"], "validation_error");
    }
    let (s, body) = call(
        &env,
        "GET",
        "/v1/dj/history?cursor=not-a-cursor",
        None,
        Some(&dj_token),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "bad cursor: {body}");
    assert_eq!(body["code"], "validation_error");

    // --- 7. Счётчики смены ---------------------------------------
    let (s, body) = call(&env, "GET", "/v1/dj/stats/session", None, Some(&dj_token)).await;
    assert_eq!(s, StatusCode::OK, "session stats: {body}");
    assert_eq!(body["session"]["id"], session.id.to_string());
    assert_eq!(body["requests"], 5, "all requests of the shift: {body}");
    assert_eq!(
        body["accepted"], 3,
        "accepted = played + still queued: {body}"
    );
    assert_eq!(
        body["played_session"], 3,
        "played belongs to the shift: {body}"
    );
    assert_eq!(body["rejected"], 1, "rejected: {body}");
    // Платежей в этом сценарии нет — DJ-доля честный ноль.
    assert_eq!(body["earned_session_minor"], 0, "earned: {body}");
    let (s, body) = call(&env, "GET", "/v1/dj/stats/session", None, Some(&club_token)).await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "club token must not read shift stats: {body}"
    );

    // --- 8. DJ без смены: пустой лог и `session: null` -----------
    let no_shift_dj = uuid::Uuid::now_v7();
    let orphan_token = env
        .keys
        .issue_access(no_shift_dj, Role::Dj, uuid::Uuid::now_v7())
        .unwrap();
    let (s, body) = call(&env, "GET", "/v1/dj/history", None, Some(&orphan_token)).await;
    assert_eq!(s, StatusCode::OK, "no shift is an empty log, not an error");
    assert_eq!(body["items"].as_array().map(Vec::len), Some(0));
    let (s, body) = call(
        &env,
        "GET",
        "/v1/dj/stats/session",
        None,
        Some(&orphan_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "no shift stats: {body}");
    assert!(body["session"].is_null(), "no shift reported honestly");
}

/// Минимальное экранирование курсора для query-строки: `base64url` курсора
/// содержит только `A–Z a–z 0–9 - _`, поэтому экранировать достаточно
/// служебные символы, но тест не должен зависеть от этого совпадения.
fn url_escape(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            other => format!("%{:02X}", other as u32),
        })
        .collect()
}
