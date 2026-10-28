//! E2e открытой регистрации (Test Mode stand): `docs/signup-contract.md`.
//!
//! Требует живой Postgres (`DATABASE_URL`): при недоступной БД тесты
//! пропускаются так же, как остальные e2e в репозитории (`test_repo()`
//! вернул `None` → eprintln + return). Запуск — только через лок-скрипт,
//! потому что тесты чистят общие таблицы:
//!
//! ```sh
//! ./scripts/db-test.sh cargo test -p dropit-gateway-api e2e_signup -- --test-threads=1
//! ```
//!
//! Покрытие: happy path клуба (201 + токены + `club`, `status='active'`,
//! логин без 2FA), happy path DJ (фрилансер и привязка по `club_slug`),
//! `409 email_taken`, `403 signup_disabled`, `400 validation_failed`,
//! `404 club_not_found`, суффикс slug при коллизии, `429 rate_limited`
//! (10/час/IP) и ротация refresh-токена после регистрации.

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::e2e::{app_env, call, test_repo};

/// Уникальный e-mail теста: в общей тестовой БД нельзя рассчитывать на
/// «чистоту» между прогонами, поэтому суффикс — UUID.
fn unique_email(prefix: &str) -> String {
    format!("{prefix}+{}@signup.test", uuid::Uuid::now_v7().as_simple())
}

/// Регистрация клуба в окружении с открытой регистрацией.
async fn register_club(env: &crate::routes::ApiEnv, body: Value) -> (StatusCode, Value) {
    call(env, "POST", "/v1/auth/club/register", Some(body), None).await
}

/// Регистрация DJ.
async fn register_dj(env: &crate::routes::ApiEnv, body: Value) -> (StatusCode, Value) {
    call(env, "POST", "/v1/auth/dj/register", Some(body), None).await
}

/// Окружение + репозиторий; `None`, если БД недоступна (тест пропускается).
macro_rules! skip_without_db {
    () => {
        match test_repo().await {
            Some(repo) => repo,
            None => {
                eprintln!("DATABASE_URL недоступен — e2e_signup пропущен (см. README CI)");
                return;
            }
        }
    };
}

// ============================================================
// Happy path
// ============================================================

/// Клуб: 201, пара токенов + `club{id,name,slug,email}`, статус `active`,
/// затем обычный логин паролем работает без 2FA (Test Mode: аккаунт сразу
/// рабочий).
#[tokio::test]
async fn e2e_signup_club_happy_path_and_login() {
    let repo = skip_without_db!();
    let pool = repo.pool();
    let env = app_env(Some(repo.clone()), None, None).await;

    let email = unique_email("club-happy");
    let password = "demo-club-pass";
    let (status, body) = register_club(
        &env,
        json!({
            "name": "Barrio Latino",
            "email": email,
            "password": password,
            "city": "Madrid",
            "country": "ES"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "club register: {body}");

    // Токены — тот же формат, что у логина (TokenPair, развёрнут в корень).
    assert!(
        body["access_token"].as_str().is_some_and(|t| !t.is_empty()),
        "нет access_token: {body}"
    );
    assert!(
        body["refresh_token"]
            .as_str()
            .is_some_and(|t| !t.is_empty()),
        "нет refresh_token: {body}"
    );
    assert_eq!(body["token_type"], "Bearer");
    assert!(body["expires_in"].as_i64().unwrap_or(0) > 0);

    // Доп. поле club.
    assert_eq!(body["club"]["name"], "Barrio Latino");
    assert_eq!(body["club"]["email"], email);
    assert_eq!(body["club"]["slug"], "barrio-latino");
    let club_id: uuid::Uuid = body["club"]["id"]
        .as_str()
        .expect("club.id")
        .parse()
        .expect("club.id — uuid");

    // В БД: статус active (Test Mode: без верификации админом), город/страна.
    let (db_status, db_city, db_country, db_slug): (
        String,
        Option<String>,
        Option<String>,
        String,
    ) = sqlx::query_as("SELECT status, city, country, slug FROM club WHERE id = $1")
        .bind(club_id)
        .fetch_one(&pool)
        .await
        .expect("club row");
    assert_eq!(db_status, "active", "клуб создан активным");
    assert_eq!(db_city.as_deref(), Some("Madrid"));
    assert_eq!(db_country.as_deref(), Some("ES"));
    assert_eq!(db_slug, "barrio-latino");

    // Access-токен — рабочая роль club (RBAC-гейт).
    let access = body["access_token"].as_str().unwrap();
    let (s, echo) = call(&env, "GET", "/v1/club/echo", None, Some(access)).await;
    assert_eq!(s, StatusCode::OK, "club echo по токену регистрации: {echo}");

    // Логин сразу после регистрации: без 2FA → сразу пара токенов
    // (не TotpChallenge), пароль проверяется Argon2id-хешем из регистрации.
    let (s, login) = call(
        &env,
        "POST",
        "/v1/auth/club/login",
        Some(json!({ "email": email, "password": password })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "login после регистрации: {login}");
    assert!(
        login["mfa_required"].is_null(),
        "2FA не должна требоваться: {login}"
    );
    assert!(login["access_token"]
        .as_str()
        .is_some_and(|t| !t.is_empty()));

    // Регистр e-mail не важен (citext): логин по EMAIL@… тоже проходит.
    let (s, _) = call(
        &env,
        "POST",
        "/v1/auth/club/login",
        Some(json!({ "email": email.to_uppercase(), "password": password })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "логин по email в верхнем регистре");
}

/// DJ-фрилансер: 201, `dj.club_id = null`, статус active, логин без 2FA.
#[tokio::test]
async fn e2e_signup_dj_freelancer_happy_path() {
    let repo = skip_without_db!();
    let pool = repo.pool();
    let env = app_env(Some(repo.clone()), None, None).await;

    let email = unique_email("dj-free");
    let (status, body) = register_dj(
        &env,
        json!({
            "display_name": "DJ Noche",
            "email": email,
            "password": "demo-dj-pass"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "dj register: {body}");
    assert!(body["access_token"].as_str().is_some_and(|t| !t.is_empty()));
    assert_eq!(body["dj"]["display_name"], "DJ Noche");
    assert_eq!(body["dj"]["email"], email);
    assert!(body["dj"]["club_id"].is_null(), "фрилансер: club_id = null");
    let dj_id: uuid::Uuid = body["dj"]["id"].as_str().expect("dj.id").parse().unwrap();

    let (db_status, db_club): (String, Option<uuid::Uuid>) =
        sqlx::query_as("SELECT status, club_id FROM dj WHERE id = $1")
            .bind(dj_id)
            .fetch_one(&pool)
            .await
            .expect("dj row");
    assert_eq!(db_status, "active");
    assert!(db_club.is_none());

    let (s, login) = call(
        &env,
        "POST",
        "/v1/auth/dj/login",
        Some(json!({ "email": email, "password": "demo-dj-pass" })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "dj login: {login}");
    assert!(login["access_token"]
        .as_str()
        .is_some_and(|t| !t.is_empty()));
}

/// DJ с `club_slug`: привязка к существующему клубу (`dj.club_id`).
#[tokio::test]
async fn e2e_signup_dj_with_club_slug() {
    let repo = skip_without_db!();
    let pool = repo.pool();
    let env = app_env(Some(repo.clone()), None, None).await;

    // Клуб, к которому привязывается DJ (создан регистрацией — как в UI).
    let club_email = unique_email("dj-club");
    let (s, club_body) = register_club(
        &env,
        json!({ "name": "Bass Room", "email": club_email, "password": "club-pass-123" }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "клуб для привязки: {club_body}");
    let club_id: uuid::Uuid = club_body["club"]["id"].as_str().unwrap().parse().unwrap();

    let (status, body) = register_dj(
        &env,
        json!({
            "display_name": "DJ Bind",
            "email": unique_email("dj-bind"),
            "password": "demo-dj-pass",
            "club_slug": "bass-room"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "dj с club_slug: {body}");
    assert_eq!(
        body["dj"]["club_id"].as_str().unwrap(),
        club_id.to_string(),
        "club_id из slug"
    );

    let dj_id: uuid::Uuid = body["dj"]["id"].as_str().unwrap().parse().unwrap();
    let (db_club,): (Option<uuid::Uuid>,) = sqlx::query_as("SELECT club_id FROM dj WHERE id = $1")
        .bind(dj_id)
        .fetch_one(&pool)
        .await
        .expect("dj row");
    assert_eq!(db_club, Some(club_id));

    // Неизвестный slug → 404 club_not_found (контракт).
    let (status, body) = register_dj(
        &env,
        json!({
            "display_name": "DJ Lost",
            "email": unique_email("dj-lost"),
            "password": "demo-dj-pass",
            "club_slug": "no-such-club"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown slug: {body}");
    assert_eq!(body["code"], "club_not_found");
}

// ============================================================
// Ошибки контракта
// ============================================================

/// `409 email_taken` для клуба и для DJ (citext: регистр не важен).
#[tokio::test]
async fn e2e_signup_duplicate_email_conflict() {
    let repo = skip_without_db!();
    let env = app_env(Some(repo.clone()), None, None).await;

    let email = unique_email("dup");
    let (s, body) = register_club(
        &env,
        json!({ "name": "Dup Club", "email": email, "password": "club-pass-123" }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "первая регистрация: {body}");

    // Тот же e-mail другим клубом → 409.
    let (s, body) = register_club(
        &env,
        json!({ "name": "Dup Club Two", "email": email, "password": "club-pass-123" }),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "дубль e-mail: {body}");
    assert_eq!(body["code"], "email_taken");

    // Регистр не важен (citext).
    let (s, body) = register_club(
        &env,
        json!({ "name": "Dup Club Three", "email": email.to_uppercase(), "password": "club-pass-123" }),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::CONFLICT,
        "дубль e-mail в другом регистре: {body}"
    );
    assert_eq!(body["code"], "email_taken");

    // DJ: свой уникальный индекс по dj.email.
    let dj_email = unique_email("dup-dj");
    let (s, _) = register_dj(
        &env,
        json!({ "display_name": "Dup DJ", "email": dj_email, "password": "demo-dj-pass" }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, body) = register_dj(
        &env,
        json!({ "display_name": "Dup DJ 2", "email": dj_email, "password": "demo-dj-pass" }),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT, "дубль dj e-mail: {body}");
    assert_eq!(body["code"], "email_taken");
}

/// `403 signup_disabled`, когда `DROPIT_OPEN_SIGNUP` не включён.
#[tokio::test]
async fn e2e_signup_disabled_flag() {
    let repo = skip_without_db!();
    let mut env = app_env(Some(repo.clone()), None, None).await;
    env.open_signup = false;

    let email = unique_email("disabled");
    let (s, body) = register_club(
        &env,
        json!({ "name": "Disabled Club", "email": email, "password": "club-pass-123" }),
    )
    .await;
    assert_eq!(
        s,
        StatusCode::FORBIDDEN,
        "club при выключенном флаге: {body}"
    );
    assert_eq!(body["code"], "signup_disabled");

    let (s, body) = register_dj(
        &env,
        json!({ "display_name": "Disabled DJ", "email": email, "password": "demo-dj-pass" }),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "dj при выключенном флаге: {body}");
    assert_eq!(body["code"], "signup_disabled");

    // Ничего не создано.
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM club WHERE email = $1")
        .bind(&email)
        .fetch_one(&repo.pool())
        .await
        .unwrap();
    assert_eq!(count, 0, "при выключенном флаге клуб не создаётся");
}

/// `400 validation_failed`: пустое имя, кривой e-mail, короткий пароль,
/// кривая страна. Ошибки не создают строк и не тратят лимит.
#[tokio::test]
async fn e2e_signup_validation_failed() {
    let repo = skip_without_db!();
    let env = app_env(Some(repo.clone()), None, None).await;

    let cases = [
        json!({ "name": "  ", "email": unique_email("v1"), "password": "club-pass-123" }),
        json!({ "name": "Valid", "email": "not-an-email", "password": "club-pass-123" }),
        json!({ "name": "Valid", "email": unique_email("v2"), "password": "short" }),
        json!({ "name": "Valid", "email": unique_email("v3"), "password": "club-pass-123", "country": "ESP" }),
    ];
    for case in cases {
        let (s, body) = register_club(&env, case.clone()).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "ожидали 400 для {case}: {body}");
        assert_eq!(body["code"], "validation_failed", "{case}");
    }

    // DJ: пустой display_name и короткий пароль.
    let (s, body) = register_dj(
        &env,
        json!({ "display_name": "   ", "email": unique_email("v4"), "password": "demo-dj-pass" }),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "validation_failed");

    let (s, body) = register_dj(
        &env,
        json!({ "display_name": "DJ", "email": unique_email("v5"), "password": "1234567" }),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "validation_failed");
}

/// Админ публично не регистрируется: `403 forbidden` при любом теле
/// (роут существует, чтобы клиент не получал 404).
#[tokio::test]
async fn e2e_signup_admin_register_forbidden() {
    let env = app_env(None, None, None).await;
    let (s, body) = call(
        &env,
        "POST",
        "/v1/auth/admin/register",
        Some(json!({ "email": "admin@evil.test", "password": "admin-pass-123" })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "admin register: {body}");
    assert_eq!(body["code"], "forbidden");
}

// ============================================================
// Slug, лимит, refresh
// ============================================================

/// Коллизия slug: второй клуб с тем же именем получает суффикс `-2`
/// (UNIQUE(slug) обеспечивается вставкой с ретраем).
#[tokio::test]
async fn e2e_signup_slug_collision_suffix() {
    let repo = skip_without_db!();
    let env = app_env(Some(repo.clone()), None, None).await;

    let (s, first) = register_club(
        &env,
        json!({ "name": "Slug Race", "email": unique_email("slug1"), "password": "club-pass-123" }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{first}");

    let (s, second) = register_club(
        &env,
        json!({ "name": "Slug Race", "email": unique_email("slug2"), "password": "club-pass-123" }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "второй клуб: {second}");
    assert_eq!(first["club"]["slug"], "slug-race");
    assert_eq!(
        second["club"]["slug"], "slug-race-2",
        "суффикс при коллизии"
    );

    let (s, third) = register_club(
        &env,
        json!({ "name": "slug race!", "email": unique_email("slug3"), "password": "club-pass-123" }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "третий клуб: {third}");
    assert_eq!(third["club"]["slug"], "slug-race-3");
}

/// `429 rate_limited`: лимит регистраций с одного IP за час берётся из
/// `DROPIT_SIGNUP_RATE_PER_HOUR` (в окружении — `signup_rate_per_hour`).
/// Здесь лимит сужен до 3, чтобы тест не хешировал пароль 11 раз; дефолт 10
/// проверяется юнит-тестом `infra::tests::signup_rate_per_hour_values`.
/// В e2e нет `ConnectInfo`, поэтому IP — общий бакет `0.0.0.0`; тест сам
/// чистит счётчик, чтобы результат не зависел от порядка тестов.
#[tokio::test]
async fn e2e_signup_rate_limit_per_ip() {
    let repo = skip_without_db!();
    let pool = repo.pool();
    let _ = sqlx::query("DELETE FROM signup_attempt")
        .execute(&pool)
        .await;

    let mut env = app_env(Some(repo.clone()), None, None).await;
    env.signup_rate_per_hour = 3;

    for i in 0..3 {
        let (s, body) = register_club(
            &env,
            json!({
                "name": format!("Limit Club {i}"),
                "email": unique_email(&format!("limit{i}")),
                "password": "club-pass-123"
            }),
        )
        .await;
        assert_eq!(
            s,
            StatusCode::CREATED,
            "регистрация {i} в пределах лимита: {body}"
        );
    }

    let (s, body) = register_club(
        &env,
        json!({ "name": "Limit Club Over", "email": unique_email("limitover"), "password": "club-pass-123" }),
    )
    .await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS, "сверх лимита: {body}");
    assert_eq!(body["code"], "rate_limited", "форма конверта ошибки");

    // Лимит общий для клуба и DJ.
    let (s, body) = register_dj(
        &env,
        json!({ "display_name": "Limit DJ", "email": unique_email("limitdj"), "password": "demo-dj-pass" }),
    )
    .await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS, "DJ сверх лимита: {body}");
    assert_eq!(body["code"], "rate_limited");

    // Поднятый лимит (как на стенде: 100) снова пропускает регистрацию —
    // счётчик тот же, меняется только порог.
    env.signup_rate_per_hour = 100;
    let (s, body) = register_club(
        &env,
        json!({ "name": "Limit Raised", "email": unique_email("limitraised"), "password": "club-pass-123" }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "лимит 100 пропускает: {body}");
}

/// Отсутствие таблицы `signup_attempt` (миграция 0011 ещё не применена на
/// стенде) НЕ ломает регистрацию: лимитер fail-open, а инцидент пишется в
/// warn-лог. Гарантия важна для порядка деплоя — бинарник может приехать
/// раньше миграции, и онбординг всё равно работает.
#[tokio::test]
async fn e2e_signup_survives_missing_counter_table() {
    let repo = skip_without_db!();
    let pool = repo.pool();
    // В отдельной транзакции не выйдет: таблица нужна всем тестам процесса.
    // Поэтому переименовываем и обязательно возвращаем обратно.
    let renamed = sqlx::query("ALTER TABLE signup_attempt RENAME TO signup_attempt_hidden")
        .execute(&pool)
        .await
        .is_ok();
    if !renamed {
        eprintln!("signup_attempt отсутствует — тест fail-open не нужен");
        return;
    }

    let env = app_env(Some(repo.clone()), None, None).await;
    let (status, body) = register_club(
        &env,
        json!({
            "name": "No Counter Club",
            "email": unique_email("nocounter"),
            "password": "club-pass-123"
        }),
    )
    .await;

    // Возвращаем таблицу ДО ассертов, чтобы падение теста не оставило схему
    // сломанной для остальных прогонов.
    let _ = sqlx::query("ALTER TABLE signup_attempt_hidden RENAME TO signup_attempt")
        .execute(&pool)
        .await;

    assert_eq!(
        status,
        StatusCode::CREATED,
        "без таблицы счётчика регистрация должна работать: {body}"
    );
    assert!(body["access_token"].as_str().is_some_and(|t| !t.is_empty()));
}

/// Refresh-ротация работает для зарегистрированного пользователя: токен из
/// ответа регистрации живёт в `refresh_token` и ротируется как после логина
/// (повторное использование старого → 401 + ревок семьи).
#[tokio::test]
async fn e2e_signup_refresh_rotation() {
    let repo = skip_without_db!();
    let env = app_env(Some(repo.clone()), None, None).await;

    let (s, body) = register_club(
        &env,
        json!({ "name": "Rotate Club", "email": unique_email("rotate"), "password": "club-pass-123" }),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{body}");
    let refresh = body["refresh_token"].as_str().unwrap().to_string();

    let (s, rotated) = call(
        &env,
        "POST",
        "/v1/auth/refresh",
        Some(json!({ "refresh_token": refresh })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "ротация refresh: {rotated}");
    let new_refresh = rotated["refresh_token"].as_str().unwrap().to_string();
    assert_ne!(new_refresh, refresh, "refresh должен смениться");

    // Старый токен → reuse-детекция (401) и ревок всей семьи.
    let (s, body) = call(
        &env,
        "POST",
        "/v1/auth/refresh",
        Some(json!({ "refresh_token": refresh })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "reuse старого: {body}");

    // После ревока семьи мёртв и новый токен.
    let (s, _) = call(
        &env,
        "POST",
        "/v1/auth/refresh",
        Some(json!({ "refresh_token": new_refresh })),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "семья отозвана");
}
