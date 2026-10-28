//! E2e сброса пароля (`password_reset_token`, миграция 0015).
//!
//! Требует живой Postgres (`DATABASE_URL`): при недоступной БД тесты
//! пропускаются так же, как остальные e2e в репозитории (`test_repo()`
//! вернул `None` → eprintln + return). Запуск — только через лок-скрипт,
//! потому что тесты чистят общие таблицы:
//!
//! ```sh
//! ./scripts/db-test.sh cargo test -p dropit-gateway-api e2e_reset -- --test-threads=1
//! ```
//!
//! Покрытие: happy path (запрос → 202 + `reset_url` на стенде → погашение
//! токена → 204 → старый пароль не пускает, новый пускает → refresh-токены
//! отозваны), неизвестный адрес — тоже 202 без `reset_url`, повторное
//! использование токена → 401 `invalid_reset_token`, короткий пароль →
//! 400 (токен при этом НЕ сжигается), лимит 5/час → 429, `503` без БД и
//! `400` на неизвестную роль.
//!
//! `password_reset_token` чистится здесь, а не в `e2e::test_repo()`:
//! чужой файл под этот тест не меняется (`e2e.rs` — общий, его правят
//! другие задачи), а строка без FK не попадает под `TRUNCATE … CASCADE`
//! соседних таблиц.

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::e2e::{app_env, call, test_repo};

/// Env-флаг стенда: выдавать `reset_url` в ответе 202.
const REVEAL_ENV: &str = "DROPIT_RESET_REVEAL_TOKEN";

/// Окружение + репозиторий; `None`, если БД недоступна (тест пропускается).
macro_rules! skip_without_db {
    () => {
        match test_repo().await {
            Some(repo) => repo,
            None => {
                eprintln!("DATABASE_URL недоступен — e2e_reset пропущен (см. README CI)");
                return;
            }
        }
    };
}

/// Уникальный e-mail теста (общая тестовая БД не гарантирует чистоту).
fn unique_email(prefix: &str) -> String {
    format!("{prefix}+{}@reset.test", uuid::Uuid::now_v7().as_simple())
}

/// Чистит токены сброса перед тестом: строки не имеют FK и переживают
/// `TRUNCATE club CASCADE`, а лимит по IP (`0.0.0.0` в e2e без
/// `ConnectInfo`) иначе накопится между прогонами.
async fn clean_reset_tokens(repo: &dropit_infra_db::IdentityRepo) {
    let _ = sqlx::query("DELETE FROM password_reset_token")
        .execute(&repo.pool())
        .await;
}

/// Включает выдачу ссылки на время теста и возвращает прежнее значение
/// переменной (чтобы восстановить после).
fn enable_reveal() -> Option<String> {
    let prev = std::env::var(REVEAL_ENV).ok();
    std::env::set_var(REVEAL_ENV, "1");
    prev
}

/// Восстанавливает env-флаг после теста.
fn restore_reveal(prev: Option<String>) {
    match prev {
        Some(v) => std::env::set_var(REVEAL_ENV, v),
        None => std::env::remove_var(REVEAL_ENV),
    }
}

/// Создаёт рабочий клуб открытой регистрацией и возвращает
/// `(email, password, refresh_token)`.
async fn create_club(env: &crate::routes::ApiEnv) -> (String, String, String) {
    let email = unique_email("club");
    let password = "old-club-pass";
    let (status, body) = call(
        env,
        "POST",
        "/v1/auth/club/register",
        Some(json!({ "name": "Reset Club", "email": email, "password": password })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "club register: {body}");
    let refresh = body["refresh_token"]
        .as_str()
        .expect("refresh_token регистрации")
        .to_string();
    (email, password.to_string(), refresh)
}

/// Запрос сброса; возвращает `(status, body)`.
async fn request_reset(
    env: &crate::routes::ApiEnv,
    email: &str,
    role: &str,
) -> (StatusCode, Value) {
    call(
        env,
        "POST",
        "/v1/auth/password/reset-request",
        Some(json!({ "email": email, "role": role })),
        None,
    )
    .await
}

/// Достаёт сырой токен из `reset_url` (`/reset-password/<token>`).
fn token_from_url(body: &Value) -> String {
    body["reset_url"]
        .as_str()
        .expect("reset_url при включённом DROPIT_RESET_REVEAL_TOKEN")
        .strip_prefix("/reset-password/")
        .expect("формат reset_url")
        .to_string()
}

/// Сброс пароля по токену; возвращает `(status, body)`.
async fn confirm_reset(
    env: &crate::routes::ApiEnv,
    token: &str,
    password: &str,
) -> (StatusCode, Value) {
    call(
        env,
        "POST",
        "/v1/auth/password/reset",
        Some(json!({ "token": token, "password": password })),
        None,
    )
    .await
}

/// Логин клуба паролем; возвращает статус.
async fn club_login(env: &crate::routes::ApiEnv, email: &str, password: &str) -> StatusCode {
    let (status, body) = call(
        env,
        "POST",
        "/v1/auth/club/login",
        Some(json!({ "email": email, "password": password })),
        None,
    )
    .await;
    if status != StatusCode::OK {
        eprintln!("club login: {status} {body}");
    }
    status
}

// ============================================================
// Happy path
// ============================================================

/// Полный сценарий: 202 + `reset_url`/`expires_at` → 204 → старый пароль
/// отвергнут, новый принят → refresh-токен, выданный до сброса, отозван.
#[tokio::test]
async fn e2e_reset_happy_path_revokes_sessions() {
    let repo = skip_without_db!();
    clean_reset_tokens(&repo).await;
    let env = app_env(Some(repo.clone()), None, None).await;
    let prev = enable_reveal();

    let (email, old_password, refresh_before) = create_club(&env).await;

    let (status, body) = request_reset(&env, &email, "club").await;
    assert_eq!(status, StatusCode::ACCEPTED, "reset-request: {body}");
    assert_eq!(body["requested"], true);
    let token = token_from_url(&body);
    assert!(!token.is_empty(), "токен не пустой");
    assert!(
        body["expires_at"].as_str().is_some_and(|s| !s.is_empty()),
        "expires_at в RFC3339: {body}"
    );

    let new_password = "new-club-pass-42";
    let (status, body) = confirm_reset(&env, &token, new_password).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "reset: {body}");

    // Пароль сменился: старый не пускает, новый пускает.
    assert_eq!(
        club_login(&env, &email, &old_password).await,
        StatusCode::UNAUTHORIZED,
        "старый пароль должен быть отвергнут"
    );
    assert_eq!(
        club_login(&env, &email, new_password).await,
        StatusCode::OK,
        "новый пароль должен пускать"
    );

    // Все refresh-токены субъекта отозваны (выданный до сброса — тоже).
    let (status, body) = call(
        &env,
        "POST",
        "/v1/auth/refresh",
        Some(json!({ "refresh_token": refresh_before })),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "refresh после сброса пароля: {body}"
    );

    restore_reveal(prev);
}

/// Неизвестный адрес: ответ тот же `202 {requested:true}` и БЕЗ
/// `reset_url` даже при включённом флаге — иначе флаг превращается в
/// оракул существования аккаунта.
#[tokio::test]
async fn e2e_reset_unknown_email_still_202() {
    let repo = skip_without_db!();
    clean_reset_tokens(&repo).await;
    let env = app_env(Some(repo.clone()), None, None).await;
    let prev = enable_reveal();

    let (status, body) = request_reset(&env, &unique_email("ghost"), "club").await;
    assert_eq!(status, StatusCode::ACCEPTED, "неизвестный адрес: {body}");
    assert_eq!(body["requested"], true);
    assert!(
        body["reset_url"].is_null(),
        "для неизвестного адреса ссылки нет: {body}"
    );
    assert!(body["expires_at"].is_null(), "и срока нет: {body}");

    restore_reveal(prev);
}

// ============================================================
// Ошибки токена и пароля
// ============================================================

/// Повторное использование того же токена → `401 invalid_reset_token`.
#[tokio::test]
async fn e2e_reset_token_is_single_use() {
    let repo = skip_without_db!();
    clean_reset_tokens(&repo).await;
    let env = app_env(Some(repo.clone()), None, None).await;
    let prev = enable_reveal();

    let (email, _, _) = create_club(&env).await;
    let (status, body) = request_reset(&env, &email, "club").await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let token = token_from_url(&body);

    let (status, body) = confirm_reset(&env, &token, "first-new-pass").await;
    assert_eq!(status, StatusCode::NO_CONTENT, "первый сброс: {body}");

    let (status, body) = confirm_reset(&env, &token, "second-new-pass").await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "повторное использование: {body}"
    );
    assert_eq!(body["code"], "invalid_reset_token");

    restore_reveal(prev);
}

/// Короткий пароль → `400 validation_failed`, и токен остаётся живым:
/// опечатка не должна сжигать ссылку.
#[tokio::test]
async fn e2e_reset_short_password_400_keeps_token() {
    let repo = skip_without_db!();
    clean_reset_tokens(&repo).await;
    let env = app_env(Some(repo.clone()), None, None).await;
    let prev = enable_reveal();

    let (email, _, _) = create_club(&env).await;
    let (status, body) = request_reset(&env, &email, "club").await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let token = token_from_url(&body);

    let (status, body) = confirm_reset(&env, &token, "1234567").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "7 символов: {body}");
    assert_eq!(body["code"], "validation_failed");

    // Границы 8…128: 129 символов тоже 400.
    let (status, body) = confirm_reset(&env, &token, &"x".repeat(129)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "129 символов: {body}");

    // Токен не сожжён валидацией — корректный пароль проходит.
    let (status, body) = confirm_reset(&env, &token, "valid-new-pass").await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "токен должен остаться живым: {body}"
    );

    restore_reveal(prev);
}

/// Неизвестный/мусорный токен → `401 invalid_reset_token` (без различения
/// причин).
#[tokio::test]
async fn e2e_reset_unknown_token_401() {
    let repo = skip_without_db!();
    clean_reset_tokens(&repo).await;
    let env = app_env(Some(repo.clone()), None, None).await;

    let (status, body) = confirm_reset(&env, "deadbeef-not-a-token", "valid-new-pass").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["code"], "invalid_reset_token");
}

// ============================================================
// Лимит
// ============================================================

/// Лимит 5/час: первые пять запросов — `202`, шестой — `429 rate_limited`.
///
/// Считается по существующему адресу (строка появляется на каждый
/// принятый запрос); IP-бакет в e2e общий (`0.0.0.0`), поэтому чистим
/// таблицу перед тестом.
#[tokio::test]
async fn e2e_reset_rate_limit_after_five() {
    let repo = skip_without_db!();
    clean_reset_tokens(&repo).await;
    let env = app_env(Some(repo.clone()), None, None).await;
    let _ = std::env::remove_var(REVEAL_ENV);

    let (email, _, _) = create_club(&env).await;
    for i in 0..5 {
        let (status, body) = request_reset(&env, &email, "club").await;
        assert_eq!(
            status,
            StatusCode::ACCEPTED,
            "запрос {i} в пределах лимита: {body}"
        );
    }

    let (status, body) = request_reset(&env, &email, "club").await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "сверх лимита: {body}"
    );
    assert_eq!(body["code"], "rate_limited");
}

// ============================================================
// Валидация без БД + деградация без БД
// ============================================================

/// `503 external_service_unavailable` без `DATABASE_URL`
/// (тот же паттерн, что у регистрации). Валидация при этом срабатывает
/// раньше: мусорное тело — `400`, а не `503`.
#[tokio::test]
async fn e2e_reset_without_db_503() {
    let env = app_env(None, None, None).await;

    let (status, body) = request_reset(&env, "owner@club.test", "club").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(
        body["code"], "external_service_unavailable",
        "код 503: {body}"
    );

    let (status, body) = confirm_reset(&env, "any-token", "valid-new-pass").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(
        body["code"], "external_service_unavailable",
        "код 503: {body}"
    );

    // Валидация — до обращения к БД.
    let (status, body) = request_reset(&env, "", "club").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "validation_failed");

    let (status, body) = request_reset(&env, "owner@club.test", "guest").await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "guest — не парольная роль: {body}"
    );
    assert_eq!(body["code"], "validation_failed");

    let (status, body) = confirm_reset(&env, "any-token", "short").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "validation_failed");
}
