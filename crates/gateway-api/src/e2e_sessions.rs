//! E2e-тест F2: сессии, QR, join гостя.
//!
//! Критерий приёмки фазы: скан тест-QR → session info (клуб, DJ,
//! цена); чужой QR / чужая сессия отклонены; повторный join —
//! re-grant, не дубль.
//!
//! Требует живой Postgres (compose). При недоступной БД — пропуск.

use axum::http::StatusCode;
use dropit_common::Role;
use dropit_infra_db::SessionRepo;
use serde_json::{json, Value};
use uuid::Uuid;

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
    let env = crate::e2e::app_env(Some(repo.clone()), Some(sessions.clone()), None).await;

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

// ============================================================
// E2e: GET /v1/sessions/current (W2-A)
// ============================================================

/// Выдаёт гостю грант на сессию через реальный QR-флоу (issue QR +
/// POST /v1/sessions/join), как после скана QR. Возвращает тело join.
async fn join_via_qr(
    env: &crate::routes::ApiEnv,
    sessions: &SessionRepo,
    session_id: Uuid,
    device_token: &str,
) -> Value {
    // Уникальный 64-hex QR-токен на вызов (UNIQUE(token_hash) в БД
    // не даёт переиспользовать один хеш на разных issue-вызовах).
    let nonce = Uuid::now_v7();
    let qr_plain = format!("{}{}", nonce.simple(), nonce.simple());
    let qr_expires = chrono::Utc::now() + chrono::Duration::hours(6);
    sessions
        .issue_qr_token(
            session_id,
            &crate::sessions_routes::hash_qr_for_test(&qr_plain),
            qr_expires,
        )
        .await
        .unwrap();
    let (s, body) = crate::e2e::call(
        env,
        "POST",
        "/v1/sessions/join",
        Some(json!({ "qr_token": qr_plain })),
        Some(device_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "join setup: {body}");
    body
}

/// Вставляет клуб + DJ (фикстуры e2e). Возвращает (club_id, dj_id).
async fn seed_club_dj(pool: &sqlx::PgPool, name: &str, slug: &str, city: &str) -> (Uuid, Uuid) {
    let club_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash, address) \
         VALUES ($1, $2, $3, $4, 'no-hash', $5)",
    )
    .bind(club_id)
    .bind(name)
    .bind(slug)
    .bind(format!("{slug}@club.test"))
    .bind(city)
    .execute(pool)
    .await
    .unwrap();
    let dj_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO dj (id, club_id, email, password_hash, display_name) \
         VALUES ($1, $2, $3, 'no-hash', $4)",
    )
    .bind(dj_id)
    .bind(club_id)
    .bind(format!("{slug}@dj.test"))
    .bind(format!("DJ {name}"))
    .execute(pool)
    .await
    .unwrap();
    (club_id, dj_id)
}

#[tokio::test]
async fn e2e_sessions_current() {
    let repo = match crate::e2e::test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e W2-A пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let env =
        crate::e2e::app_env(Some(repo.clone()), Some(sessions.clone()), None).await;
    let pool = repo.pool();

    // --- Фикстуры: клуб A/B + DJ + сессии -------------------------
    let (club_a, dj_a) = seed_club_dj(&pool, "E2E Club A", "e2e-club-a", "Madrid").await;
    let (club_b, dj_b) = seed_club_dj(&pool, "E2E Club B", "e2e-club-b", "Barcelona").await;

    // Сессия A: created → active. Сессия B открывается позже → свежее.
    let session_a = sessions
        .create_session(club_a, Some(dj_a), None, "Europe/Madrid")
        .await
        .unwrap();
    sessions
        .apply_transition(session_a.id, dropit_domain::SessionTransition::Open)
        .await
        .unwrap();
    // Делаем A «старее» по opened_at — чтобы B был свежее (тест ordering).
    sqlx::query("UPDATE session SET opened_at = now() - interval '5 minutes' WHERE id = $1")
        .bind(session_a.id)
        .execute(&pool)
        .await
        .unwrap();
    let session_b = sessions
        .create_session(club_b, Some(dj_b), None, "Europe/Madrid")
        .await
        .unwrap();
    sessions
        .apply_transition(session_b.id, dropit_domain::SessionTransition::Open)
        .await
        .unwrap();

    // --- Гость G_A: грант только на A (через QR) -------------------
    let g_a = Uuid::now_v7();
    repo.create_guest_device(g_a, "ios", Some("e2e"))
        .await
        .unwrap();
    join_via_qr(&env, &sessions, session_a.id, &g_a.to_string()).await;

    // 200: корректные поля, requests_open=true (active), DJ, city,
    // guest_grant_expires_at == expires_at активного QR.
    let (s, body) = crate::e2e::call(
        &env,
        "GET",
        "/v1/sessions/current",
        None,
        Some(&g_a.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "current 200: {body}");
    let sess = &body["session"];
    assert_eq!(sess["club_id"], json!(session_a.club_id));
    assert_eq!(sess["club_name"], "E2E Club A");
    assert_eq!(sess["club_city"], "Madrid");
    assert_eq!(sess["dj"]["id"], json!(dj_a));
    assert_eq!(sess["dj"]["display_name"], "DJ E2E Club A");
    assert_eq!(sess["requests_open"], true);
    assert!(sess["closes_at"].is_null(), "active session: closes_at null");
    assert!(sess["opened_at"].as_str().is_some(), "opened_at present");
    // grant_expires_at — будущая RFC3339 (плановое закрытие QR).
    let ge = chrono::DateTime::parse_from_rfc3339(
        sess["guest_grant_expires_at"].as_str().expect("grant_expires_at"),
    )
    .expect("grant_expires_at rfc3339");
    assert!(ge.with_timezone(&chrono::Utc) > chrono::Utc::now());

    // --- Изоляция: G_A не видит сессию клуба B ---------------------
    // (B активна, но у G_A нет на неё гранта — ответ остаётся A,
    // не B).
    assert_eq!(sess["club_id"], json!(session_a.club_id));
    assert_ne!(sess["club_id"], json!(club_b));

    // --- Paused: requests_open=false, сессия возвращается (200) ----
    sessions
        .apply_transition(session_a.id, dropit_domain::SessionTransition::Pause)
        .await
        .unwrap();
    let (s, body) = crate::e2e::call(
        &env,
        "GET",
        "/v1/sessions/current",
        None,
        Some(&g_a.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "paused current: {body}");
    assert_eq!(body["session"]["requests_open"], false);
    // Resume — обратно active, requests_open=true.
    sessions
        .apply_transition(session_a.id, dropit_domain::SessionTransition::Resume)
        .await
        .unwrap();

    // --- Истёкший (отозванный) грант → 404 -------------------------
    let g_revoke = Uuid::now_v7();
    repo.create_guest_device(g_revoke, "ios", Some("e2e"))
        .await
        .unwrap();
    join_via_qr(&env, &sessions, session_a.id, &g_revoke.to_string()).await;
    sqlx::query("UPDATE session_guest SET revoked_at = now() WHERE session_id = $1 AND guest_device_id = $2")
        .bind(session_a.id)
        .bind(g_revoke)
        .execute(&pool)
        .await
        .unwrap();
    let (s, body) = crate::e2e::call(
        &env,
        "GET",
        "/v1/sessions/current",
        None,
        Some(&g_revoke.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "revoked grant: {body}");
    assert_eq!(body["code"], "session_not_found");

    // --- Ordering: гость с грантами на A и B → свежая (B) ----------
    let g_ab = Uuid::now_v7();
    repo.create_guest_device(g_ab, "ios", Some("e2e"))
        .await
        .unwrap();
    join_via_qr(&env, &sessions, session_a.id, &g_ab.to_string()).await;
    join_via_qr(&env, &sessions, session_b.id, &g_ab.to_string()).await;
    let (s, body) = crate::e2e::call(
        &env,
        "GET",
        "/v1/sessions/current",
        None,
        Some(&g_ab.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "ordering: {body}");
    assert_eq!(
        body["session"]["club_id"],
        json!(session_b.club_id),
        "freshest by opened_at = B"
    );
    assert_eq!(body["session"]["club_name"], "E2E Club B");

    // --- Закрытая сессия → 404 (доступа больше нет) ----------------
    // G_A имел грант на A; закрываем A — current исчезает.
    sessions
        .apply_transition(session_a.id, dropit_domain::SessionTransition::Close)
        .await
        .unwrap();
    let (s, body) = crate::e2e::call(
        &env,
        "GET",
        "/v1/sessions/current",
        None,
        Some(&g_a.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "closed session: {body}");
    assert_eq!(body["code"], "session_not_found");

    // --- Гость без гранта → 404 ------------------------------------
    let g_none = Uuid::now_v7();
    repo.create_guest_device(g_none, "ios", Some("e2e"))
        .await
        .unwrap();
    let (s, body) = crate::e2e::call(
        &env,
        "GET",
        "/v1/sessions/current",
        None,
        Some(&g_none.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "no grant: {body}");
    assert_eq!(body["code"], "session_not_found");

    // --- 401: без токена и битый токен -----------------------------
    let (s, body) = crate::e2e::call(&env, "GET", "/v1/sessions/current", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "no token: {body}");
    assert_eq!(body["code"], "unauthorized");
    let (s, _) = crate::e2e::call(
        &env,
        "GET",
        "/v1/sessions/current",
        None,
        Some("garbage.token"),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "bad token");

    // --- 403: токен чужой роли (club_owner JWT) --------------------
    let club_jwt = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Club, Uuid::now_v7())
        .unwrap();
    let (s, body) = crate::e2e::call(
        &env,
        "GET",
        "/v1/sessions/current",
        None,
        Some(&club_jwt),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "club_owner token: {body}");
    assert_eq!(body["code"], "forbidden");
}

// ============================================================
// E2e: POST /v1/club/sessions/{id}/qr/activate|deactivate (W5-D)
// ============================================================

/// Подсчитывает активные QR-токены сессии в БД (для проверок
/// идемпотентности и реального состояния).
async fn count_active_qr(pool: &sqlx::PgPool, session_id: Uuid) -> i64 {
    let row: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM qr_token WHERE session_id = $1 AND active",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await
    .unwrap();
    row.0
}

/// Возвращает token_hash активного QR-токена сессии (для проверки,
/// что повторный activate не выпустил новый токен — хеш тот же).
async fn active_qr_hash(pool: &sqlx::PgPool, session_id: Uuid) -> Option<String> {
    let row: (Option<String>,) = sqlx::query_as(
        "SELECT token_hash FROM qr_token WHERE session_id = $1 AND active \
         ORDER BY generated_at DESC LIMIT 1",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await
    .unwrap();
    row.0
}

#[tokio::test]
async fn e2e_qr_activate_deactivate_real_state() {
    let repo = match crate::e2e::test_repo().await {
        Some(r) => r,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e W5-D пропущен");
            return;
        }
    };
    let sessions = SessionRepo::new(repo.pool());
    let env =
        crate::e2e::app_env(Some(repo.clone()), Some(sessions.clone()), None).await;
    let pool = repo.pool();

    // --- Фикстуры: клуб + DJ + сессия (active) ---------------------
    let (club_id, dj_id) = seed_club_dj(&pool, "W5D Club", "w5d-club", "Madrid").await;
    let session = sessions
        .create_session(club_id, Some(dj_id), None, "Europe/Madrid")
        .await
        .unwrap();
    // Открываем приём — чтобы GET /v1/sessions/current работал для
    // уже вошедшего гостя после deactivate.
    sessions
        .apply_transition(session.id, dropit_domain::SessionTransition::Open)
        .await
        .unwrap();

    // JWT клуба: subject_id == club_id (как требует владение сессией).
    let club_token = env
        .keys
        .issue_access(club_id, Role::Club, uuid::Uuid::now_v7())
        .unwrap();

    // --- activate (первичный): 200, выпущен токен ------------------
    let (s, body) = crate::e2e::call(
        &env,
        "POST",
        &format!("/v1/club/sessions/{}/qr/activate", session.id),
        None,
        Some(&club_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "activate: {body}");
    assert_eq!(body["session_id"], json!(session.id));
    assert_eq!(body["qr_active"], true);
    // Первичная активация выпускает новый токен → qr_token/expires_at.
    let qr_plain = body["qr_token"]
        .as_str()
        .expect("activate issues qr_token on first call")
        .to_string();
    assert!(qr_plain.len() >= 64, "qr_token entropy");
    assert!(
        body["expires_at"].as_str().is_some(),
        "expires_at present on issue"
    );

    // В БД: ровно один активный токен.
    assert_eq!(count_active_qr(&pool, session.id).await, 1, "one active qr");
    let hash_after_first = active_qr_hash(&pool, session.id)
        .await
        .expect("active qr hash");

    // --- Гость может войти по QR (join → 200) -----------------------
    let device = Uuid::now_v7();
    repo.create_guest_device(device, "ios", Some("e2e"))
        .await
        .unwrap();
    let (s, body) = crate::e2e::call(
        &env,
        "POST",
        "/v1/sessions/join",
        Some(json!({ "qr_token": qr_plain })),
        Some(&device.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "guest join after activate: {body}");
    assert_eq!(body["club_name"], "W5D Club");

    // --- Повторный activate: 200, идемпотентность (токен тот же) ----
    let (s, body) = crate::e2e::call(
        &env,
        "POST",
        &format!("/v1/club/sessions/{}/qr/activate", session.id),
        None,
        Some(&club_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "re-activate: {body}");
    assert_eq!(body["qr_active"], true);
    // Повторный activate НЕ возвращает plain-токен (он показывается
    // один раз при выпуске) и не выпускает второй токен.
    assert!(
        body.get("qr_token").map(|v| v.is_null()).unwrap_or(true),
        "re-activate must not re-issue plain token"
    );
    assert_eq!(
        count_active_qr(&pool, session.id).await,
        1,
        "idempotent: still one active qr"
    );
    assert_eq!(
        active_qr_hash(&pool, session.id).await,
        Some(hash_after_first.clone()),
        "idempotent: same token hash"
    );

    // --- deactivate: 200, состояние выключено ----------------------
    let (s, body) = crate::e2e::call(
        &env,
        "POST",
        &format!("/v1/club/sessions/{}/qr/deactivate", session.id),
        None,
        Some(&club_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "deactivate: {body}");
    assert_eq!(body["qr_active"], false);
    // В БД: активных токенов нет.
    assert_eq!(
        count_active_qr(&pool, session.id).await,
        0,
        "no active qr after deactivate"
    );

    // --- НОВЫЙ вход по тому же QR -> 404 (деактивирован) ------------
    let device2 = Uuid::now_v7();
    repo.create_guest_device(device2, "ios", Some("e2e"))
        .await
        .unwrap();
    let (s, body) = crate::e2e::call(
        &env,
        "POST",
        "/v1/sessions/join",
        Some(json!({ "qr_token": qr_plain })),
        Some(&device2.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "new guest blocked after deactivate: {body}");
    assert_eq!(body["code"], "not_found");

    // --- Уже вошедший гость сохраняет доступ (current -> 200) -------
    let (s, body) = crate::e2e::call(
        &env,
        "GET",
        "/v1/sessions/current",
        None,
        Some(&device.to_string()),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "already-joined guest keeps access: {body}");
    assert_eq!(body["session"]["club_name"], "W5D Club");
    // Грант не отозван — основа «вечер продолжается для вошедших».

    // --- Повторный deactivate: 200 (идемпотентность) ----------------
    let (s, body) = crate::e2e::call(
        &env,
        "POST",
        &format!("/v1/club/sessions/{}/qr/deactivate", session.id),
        None,
        Some(&club_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "re-deactivate: {body}");
    assert_eq!(body["qr_active"], false);

    // --- Чужой клуб -> 404 (владение не раскрыто) -------------------
    let (club_b, _) = seed_club_dj(&pool, "W5D Club B", "w5d-club-b", "Madrid").await;
    let club_b_token = env
        .keys
        .issue_access(club_b, Role::Club, uuid::Uuid::now_v7())
        .unwrap();
    let (s, body) = crate::e2e::call(
        &env,
        "POST",
        &format!("/v1/club/sessions/{}/qr/activate", session.id),
        None,
        Some(&club_b_token),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "foreign club activate: {body}");
    assert_eq!(body["code"], "not_found");
    let (s, body) = crate::e2e::call(
        &env,
        "POST",
        &format!("/v1/club/sessions/{}/qr/deactivate", session.id),
        None,
        Some(&club_b_token),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "foreign club deactivate: {body}");

    // --- Роль guest/dj -> 403 ---------------------------------------
    let dj_token = env
        .keys
        .issue_access(uuid::Uuid::now_v7(), Role::Dj, uuid::Uuid::now_v7())
        .unwrap();
    let guest_token = env
        .keys
        .issue_access(uuid::Uuid::now_v7(), Role::Guest, uuid::Uuid::now_v7())
        .unwrap();
    let (s, body) = crate::e2e::call(
        &env,
        "POST",
        &format!("/v1/club/sessions/{}/qr/activate", session.id),
        None,
        Some(&dj_token),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "dj role activate: {body}");
    assert_eq!(body["code"], "forbidden");
    let (s, body) = crate::e2e::call(
        &env,
        "POST",
        &format!("/v1/club/sessions/{}/qr/deactivate", session.id),
        None,
        Some(&guest_token),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "guest role deactivate: {body}");
    assert_eq!(body["code"], "forbidden");

    // --- Без токена -> 401 ------------------------------------------
    let (s, body) = crate::e2e::call(
        &env,
        "POST",
        &format!("/v1/club/sessions/{}/qr/activate", session.id),
        None,
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "no token activate: {body}");
    assert_eq!(body["code"], "unauthorized");

    // --- re-activate после deactivate снова выпускает токен ---------
    // (прежний деактивирован — рабочего нет → новый выпуск).
    let (s, body) = crate::e2e::call(
        &env,
        "POST",
        &format!("/v1/club/sessions/{}/qr/activate", session.id),
        None,
        Some(&club_token),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "reactivate after deactivate: {body}");
    assert_eq!(body["qr_active"], true);
    assert!(
        body["qr_token"].as_str().is_some(),
        "reactivate issues new token"
    );
    assert_eq!(
        count_active_qr(&pool, session.id).await,
        1,
        "one active qr after reactivate"
    );
    // Это ДРУГОЙ токен, не тот, что был до deactivate (хеш различается).
    let hash_after_reactivate = active_qr_hash(&pool, session.id)
        .await
        .expect("active qr hash after reactivate");
    assert_ne!(
        hash_after_reactivate, hash_after_first,
        "reactivate issued a fresh token, old one stays deactivated"
    );
}
