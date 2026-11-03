//! E2e публичного онбординга клуба (`club_application`, миграция 0021).
//!
//! Требует живой Postgres (`DATABASE_URL`): при недоступной БД тесты
//! пропускаются так же, как остальные e2e в репозитории (`test_repo()`
//! вернул `None` → eprintln + return). Запуск — только через лок-скрипт,
//! потому что тесты чистят общие таблицы:
//!
//! ```sh
//! ./scripts/db-test.sh cargo test -p dropit-gateway-api --bin dropit-api \
//!     e2e_onboarding -- --test-threads=1
//! ```
//!
//! Покрытие (по пунктам приёмки задачи):
//!   * публичный apply: `201` + форма ответа, нормализованные колонки в БД,
//!     секреты из тела не сохраняются;
//!   * валидация: каждое правило контракта → `422 validation_error` с
//!     точным `details.field`;
//!   * антифрод: 6-я заявка с одного IP → `429 rate_limited` (лимит считается
//!     по `X-Forwarded-For` при `TRUST_PROXY=1`), чужой IP не задет;
//!     больше 3 pending на один contact → `429`;
//!   * идемпотентность: два POST с одним `Idempotency-Key` → один id
//!     (`201` + `200`), в БД одна строка;
//!   * админский список: `401` без токена, `403` клубу, `200` админу,
//!     фильтр по статусу, keyset-пагинация, `422` на мусорные параметры;
//!   * approve: клуб создан, `sha256(qr_master_token)` == `club.qr_master_token_hash`,
//!     заявка `approved` + `reviewed_by/at`, audit есть, клуб входит паролем,
//!     повтор → `409 already_reviewed`; email/password необязательны, а
//!     invite-ссылка (64 hex, sha256, TTL 7 дней) выпускается всегда;
//!   * инвайт-поток: approve без пароля → `password_hash='!'` → публичный
//!     превью → accept (Argon2id, `active`, токен погашен, audit
//!     `club.invite.accept`) → вход клуба паролем; повторная ссылка → `404`;
//!   * истёкшая ссылка → `404`, `invite_state` в списке/карточке меняется
//!     `pending → expired → (accept) accepted`; перевыпуск даёт новый токен
//!     и второе письмо, а после активации → `409 invite_already_accepted`;
//!     перевыпуск до одобрения → `409 not_approved`, readonly → `403`;
//!   * slug: явный, автоматический из названия и суффикс при коллизии,
//!     занятый slug → `409 slug_taken`;
//!   * reject: причина сохранена, audit есть, повтор → `409`;
//!   * approve с занятым email → `409 email_taken`;
//!   * `readonly`-админ: approve/reject → `403`, чтение списка — `200`.
//!
//! `club_application` чистится общим `e2e::test_repo()` (таблица уже в его
//! списке TRUNCATE): отдельная чистка здесь не нужна, а `e2e.rs` — общий
//! файл, который правят другие задачи.

#![cfg(test)]

use axum::http::StatusCode;
use dropit_common::Role;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::e2e::{app_env, call, call_with_headers, test_repo, EnvGuard};
use crate::routes::ApiEnv;

/// Окружение + репозиторий; `None`, если БД недоступна (тест пропускается).
macro_rules! skip_without_db {
    () => {
        match test_repo().await {
            Some(repo) => repo,
            None => {
                eprintln!("DATABASE_URL недоступен — e2e_onboarding пропущен (см. README CI)");
                return;
            }
        }
    };
}

/// Уникальный contact: общая тестовая БД не гарантирует чистоту, а лимит
/// pending считается по contact.
fn unique_contact(prefix: &str) -> String {
    format!("{prefix}+{}@connect.test", Uuid::now_v7().as_simple())
}

/// Полное валидное тело заявки в форме, которую отправляет лендинг
/// (`apps/web-club/src/api/connect.ts`).
fn apply_body(name: &str, contact: &str) -> Value {
    json!({
        "club": {
            "name": name,
            "city": "Мадрид",
            "address": "Calle Mayor 1",
            "website": "https://club.test",
            "contact": contact,
            "genres": ["Techno", "House"],
        },
        "venue": {
            "name": "Sala Principal",
            "type": "Ночной клуб",
            "address": "Calle Mayor 1",
            "city": "Мадрид",
            "district": "Centro",
            "capacity": 400,
            "seats": 80,
            "area_m2": 600,
        },
        "qr": { "price_minor": 200, "zones": ["Основной зал"] },
    })
}

/// `POST /v1/club/onboarding/apply` с необязательными заголовками.
async fn apply_with(
    env: &ApiEnv,
    body: &Value,
    key: Option<&str>,
    ip: Option<&str>,
) -> (StatusCode, Value) {
    let mut headers: Vec<(&str, &str)> = Vec::new();
    if let Some(key) = key {
        headers.push(("idempotency-key", key));
    }
    if let Some(ip) = ip {
        headers.push(("x-forwarded-for", ip));
    }
    call_with_headers(
        env,
        "POST",
        "/v1/club/onboarding/apply",
        Some(body.clone()),
        None,
        &headers,
    )
    .await
}

/// Валидная заявка → `201`; возвращает её id.
async fn create_application(env: &ApiEnv, body: &Value, ip: Option<&str>) -> Uuid {
    let (status, response) = apply_with(env, body, None, ip).await;
    assert_eq!(status, StatusCode::CREATED, "apply: {response}");
    response["application_id"]
        .as_str()
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .unwrap_or_else(|| panic!("application_id в ответе: {response}"))
}

/// Заявка должна быть отклонена `422` с конкретным полем.
async fn expect_field_error(env: &ApiEnv, body: Value, field: &str) {
    let (status, response) = apply_with(env, &body, None, None).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "должна быть 422 для поля {field}: {response}"
    );
    assert_eq!(response["code"], "validation_error", "{response}");
    assert_eq!(
        response["details"]["field"], field,
        "ожидалось поле {field}: {response}"
    );
}

/// Админ-фикстура: строка `admin_user` (её требует `require_admin_write`) +
/// рабочий JWT роли Admin. `role` — `superadmin` | `operator` | `readonly`.
async fn admin_token(env: &ApiEnv, pool: &PgPool, role: &str) -> (Uuid, String) {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO admin_user \
         (id, email, password_hash, totp_secret_enc, totp_confirmed_at, role, status) \
         VALUES ($1, $2, 'x', $3, now(), $4, 'active')",
    )
    .bind(id)
    .bind(format!("onboarding-{id}@dropit.test"))
    .bind(b"JBSWY3DPEHPK3PXP".to_vec())
    .bind(role)
    .execute(pool)
    .await
    .expect("админ-фикстура должна вставиться");
    let token = env
        .keys
        .issue_access(id, Role::Admin, Uuid::now_v7())
        .expect("admin jwt");
    (id, token)
}

/// JWT роли Club (токен настоящий, но роль чужая для админских роутов).
fn club_token(env: &ApiEnv) -> String {
    env.keys
        .issue_access(Uuid::now_v7(), Role::Club, Uuid::now_v7())
        .expect("club jwt")
}

/// `GET /v1/admin/onboarding-applications…` c query-строкой.
async fn list_applications(env: &ApiEnv, token: Option<&str>, query: &str) -> (StatusCode, Value) {
    let uri = format!("/v1/admin/onboarding-applications{query}");
    call(env, "GET", &uri, None, token).await
}

/// `GET /v1/admin/onboarding-applications/:id`.
async fn get_application(env: &ApiEnv, token: &str, id: Uuid) -> (StatusCode, Value) {
    let uri = format!("/v1/admin/onboarding-applications/{id}");
    call(env, "GET", &uri, None, Some(token)).await
}

/// `POST /v1/admin/onboarding-applications/:id/approve`.
async fn approve_application(
    env: &ApiEnv,
    token: &str,
    id: Uuid,
    body: Value,
) -> (StatusCode, Value) {
    let uri = format!("/v1/admin/onboarding-applications/{id}/approve");
    call(env, "POST", &uri, Some(body), Some(token)).await
}

/// `POST /v1/admin/onboarding-applications/:id/reject`.
async fn reject_application(
    env: &ApiEnv,
    token: &str,
    id: Uuid,
    body: Value,
) -> (StatusCode, Value) {
    let uri = format!("/v1/admin/onboarding-applications/{id}/reject");
    call(env, "POST", &uri, Some(body), Some(token)).await
}

/// `GET /v1/club/onboarding/invite/:token` (публично).
async fn invite_preview(env: &ApiEnv, token: &str) -> (StatusCode, Value) {
    let uri = format!("/v1/club/onboarding/invite/{token}");
    call(env, "GET", &uri, None, None).await
}

/// `POST /v1/club/onboarding/invite/:token/accept` (публично).
async fn invite_accept(env: &ApiEnv, token: &str, body: Value) -> (StatusCode, Value) {
    let uri = format!("/v1/club/onboarding/invite/{token}/accept");
    call(env, "POST", &uri, Some(body), None).await
}

/// `POST /v1/admin/onboarding-applications/:id/invite` — перевыпуск ссылки.
async fn reissue_invite(env: &ApiEnv, admin: &str, id: Uuid) -> (StatusCode, Value) {
    let uri = format!("/v1/admin/onboarding-applications/{id}/invite");
    call(env, "POST", &uri, None, Some(admin)).await
}

/// Достаёт токен из `invite_url` (`…/club/invite/<token>`) и проверяет формат.
fn token_from_url(body: &Value) -> String {
    let url = body["invite_url"].as_str().unwrap_or_default();
    assert!(
        url.contains("/club/invite/"),
        "путь страницы активации: {url}"
    );
    let token = url.rsplit('/').next().unwrap_or_default().to_string();
    assert_eq!(token.len(), 64, "invite-токен — 64 hex: {token}");
    assert!(
        token.bytes().all(|b| b.is_ascii_hexdigit()),
        "invite-токен — hex: {token}"
    );
    token
}

/// Состояние клуба в БД: хеш пароля, статус, хеш/срок invite-токена.
type ClubInviteState = (
    String,
    String,
    Option<String>,
    Option<chrono::DateTime<chrono::Utc>>,
);

async fn club_invite_state(pool: &PgPool, club_id: Uuid) -> ClubInviteState {
    sqlx::query_as(
        "SELECT password_hash, status, invite_token_hash, invite_expires_at \
         FROM club WHERE id = $1",
    )
    .bind(club_id)
    .fetch_one(pool)
    .await
    .expect("клуб должен существовать")
}

/// `sha256(token)` в hex — та же форма, в которой хеш лежит в `club`.
fn sha256_hex(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

// ============================================================
// 1. Публичный apply: форма ответа, БД, отсутствие секретов
// ============================================================

#[tokio::test]
async fn e2e_onboarding_apply_returns_201_and_stores_application() {
    let repo = skip_without_db!();
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();

    let contact = unique_contact("owner");
    let mut body = apply_body("Barrio Latino", &contact);
    // Гипотетический секрет в теле не должен ни приниматься, ни храниться.
    body["club"]["password"] = json!("super-secret-pass");
    body["venue"]["capacity"] = json!(1234);
    body["venue"]["name"] = json!("Sala Norte");
    body["venue"]["city"] = json!("Valencia");

    let (status, response) = apply_with(&env, &body, None, None).await;
    assert_eq!(status, StatusCode::CREATED, "apply: {response}");
    let application_id: Uuid = response["application_id"]
        .as_str()
        .expect("application_id")
        .parse()
        .expect("application_id — uuid");
    assert_eq!(response["status"], "pending_moderation");
    assert!(
        response["submitted_at"].as_str().is_some(),
        "submitted_at обязателен: {response}"
    );

    // Нормализованные колонки + payload.
    let (
        club_name,
        city,
        contact_db,
        email,
        price_minor,
        application_status,
        source_ip,
        idempotency_key,
        club_payload,
    ): (
        String,
        String,
        String,
        Option<String>,
        Option<i64>,
        String,
        String,
        Option<String>,
        Value,
    ) = sqlx::query_as(
        "SELECT club_name, city, contact, email, price_minor, status, \
                host(source_ip), idempotency_key, club \
         FROM club_application WHERE id = $1",
    )
    .bind(application_id)
    .fetch_one(&pool)
    .await
    .expect("заявка должна быть в БД");

    assert_eq!(club_name, "Barrio Latino");
    assert_eq!(city, "Valencia", "город площадки важнее клубного");
    assert_eq!(contact_db, contact);
    assert_eq!(email.as_deref(), Some(contact.as_str()));
    assert_eq!(price_minor, Some(200));
    assert_eq!(application_status, "pending_moderation");
    assert_eq!(source_ip, "0.0.0.0", "без ConnectInfo — сентинел бакета");
    assert_eq!(idempotency_key, None, "без заголовка ключа нет");
    assert!(
        !club_payload.to_string().contains("super-secret-pass"),
        "секреты из тела не сохраняются: {club_payload}"
    );

    // Ровно одна строка: мусорные поля не создали копий.
    let (total,): (i64,) = sqlx::query_as("SELECT count(*) FROM club_application")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(total, 1);
}

// ============================================================
// 2. Валидация контракта: 422 + details.field
// ============================================================

#[tokio::test]
async fn e2e_onboarding_apply_validates_every_contract_rule() {
    let repo = skip_without_db!();
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    let contact = unique_contact("validation");
    let base = apply_body("Validation Club", &contact);

    // Тело целиком без блоков.
    let mut no_club = base.clone();
    no_club.as_object_mut().expect("объект").remove("club");
    expect_field_error(&env, no_club, "club").await;
    let mut no_venue = base.clone();
    no_venue.as_object_mut().expect("объект").remove("venue");
    expect_field_error(&env, no_venue, "venue").await;
    let mut no_qr = base.clone();
    no_qr.as_object_mut().expect("объект").remove("qr");
    expect_field_error(&env, no_qr, "qr").await;

    // Обязательные строки.
    for (pointer, field) in [
        ("/club/name", "club.name"),
        ("/club/contact", "club.contact"),
        ("/venue/name", "venue.name"),
    ] {
        let mut body = base.clone();
        *body.pointer_mut(pointer).expect("путь") = json!("   ");
        expect_field_error(&env, body, field).await;
    }

    // Длины.
    let mut long_city = base.clone();
    *long_city.pointer_mut("/club/city").expect("путь") = json!("x".repeat(201));
    expect_field_error(&env, long_city, "club.city").await;
    let mut long_name = base.clone();
    *long_name.pointer_mut("/club/name").expect("путь") = json!("x".repeat(121));
    expect_field_error(&env, long_name, "club.name").await;

    // Сайт: только http(s) или пусто.
    let mut bad_site = base.clone();
    *bad_site.pointer_mut("/club/website").expect("путь") = json!("club.test");
    expect_field_error(&env, bad_site, "club.website").await;
    let mut ftp_site = base.clone();
    *ftp_site.pointer_mut("/club/website").expect("путь") = json!("ftp://club.test");
    expect_field_error(&env, ftp_site, "club.website").await;

    // Жанры: ≤ 8 и ≤ 40 символов; пустой сайт допустим.
    let mut too_many_genres = base.clone();
    *too_many_genres.pointer_mut("/club/genres").expect("путь") = json!(vec!["Techno"; 9]);
    expect_field_error(&env, too_many_genres, "club.genres").await;
    let mut long_genre = base.clone();
    *long_genre.pointer_mut("/club/genres").expect("путь") = json!(["x".repeat(41)]);
    expect_field_error(&env, long_genre, "club.genres").await;
    let mut empty_site = base.clone();
    *empty_site.pointer_mut("/club/website").expect("путь") = json!("");
    let (status, response) = apply_with(&env, &empty_site, None, None).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "пустой сайт допустим: {response}"
    );

    // Числа площадки.
    let mut zero_capacity = base.clone();
    *zero_capacity.pointer_mut("/venue/capacity").expect("путь") = json!(0);
    expect_field_error(&env, zero_capacity, "venue.capacity").await;
    let mut huge_capacity = base.clone();
    *huge_capacity.pointer_mut("/venue/capacity").expect("путь") = json!(100_001);
    expect_field_error(&env, huge_capacity, "venue.capacity").await;
    let mut seats_over = base.clone();
    *seats_over.pointer_mut("/venue/seats").expect("путь") = json!(401);
    expect_field_error(&env, seats_over, "venue.seats").await;
    let mut negative_seats = base.clone();
    *negative_seats.pointer_mut("/venue/seats").expect("путь") = json!(-1);
    expect_field_error(&env, negative_seats, "venue.seats").await;
    let mut huge_area = base.clone();
    *huge_area.pointer_mut("/venue/area_m2").expect("путь") = json!(100_001);
    expect_field_error(&env, huge_area, "venue.area_m2").await;

    // Деньги (минорные единицы) и зоны.
    let mut cheap = base.clone();
    *cheap.pointer_mut("/qr/price_minor").expect("путь") = json!(99);
    expect_field_error(&env, cheap, "qr.price_minor").await;
    let mut pricey = base.clone();
    *pricey.pointer_mut("/qr/price_minor").expect("путь") = json!(100_001);
    expect_field_error(&env, pricey, "qr.price_minor").await;

    // Цена необязательна: компактная форма `/register` её не присылает, и
    // заявка не должна ни падать, ни выдумывать число за клуб.
    let mut no_price = base.clone();
    no_price
        .pointer_mut("/qr")
        .expect("путь")
        .as_object_mut()
        .expect("объект")
        .remove("price_minor");
    let application_id = create_application(&env, &no_price, None).await;
    let (stored,): (Option<i64>,) =
        sqlx::query_as("SELECT price_minor FROM club_application WHERE id = $1")
            .bind(application_id)
            .fetch_one(&pool)
            .await
            .expect("заявка без цены должна быть в БД");
    assert_eq!(
        stored, None,
        "без цены в заявке NULL, а не выдуманное число"
    );

    let mut too_many_zones = base.clone();
    *too_many_zones.pointer_mut("/qr/zones").expect("путь") = json!(vec!["Зал"; 13]);
    expect_field_error(&env, too_many_zones, "qr.zones").await;
    let mut long_zone = base.clone();
    *long_zone.pointer_mut("/qr/zones").expect("путь") = json!(["x".repeat(61)]);
    expect_field_error(&env, long_zone, "qr.zones").await;

    // Все отказы — до записи: в БД только валидные заявки (пустой сайт и
    // заявка без цены), ни одно невалидное тело не сохранилось.
    let (total,): (i64,) = sqlx::query_as("SELECT count(*) FROM club_application")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(total, 2, "невалидные тела не должны попадать в БД");
}

// ============================================================
// 3. Антифрод: 5/час на IP и 3 pending на contact
// ============================================================

#[tokio::test]
async fn e2e_onboarding_apply_rate_limits_by_ip_and_contact() {
    let repo = skip_without_db!();
    let env = app_env(Some(repo), None, None).await;
    // Лимит по IP считается по X-Forwarded-For только при TRUST_PROXY=1 —
    // ровно так стенд работает за доверенным прокси.
    let _guard = EnvGuard::set("TRUST_PROXY", "1");
    let same_ip = "203.0.113.10";

    for index in 1..=5 {
        let body = apply_body(&format!("IP Club {index}"), &unique_contact("ip"));
        let (status, response) = apply_with(&env, &body, None, Some(same_ip)).await;
        assert_eq!(
            status,
            StatusCode::CREATED,
            "заявка №{index} с одного IP должна пройти: {response}"
        );
    }
    let sixth = apply_body("IP Club 6", &unique_contact("ip"));
    let (status, response) = apply_with(&env, &sixth, None, Some(same_ip)).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "6-я: {response}");
    assert_eq!(response["code"], "rate_limited");

    // Другой IP — свой бакет, лимит не исчерпан.
    let other_ip = "203.0.113.11";
    let (status, response) = apply_with(
        &env,
        &apply_body("Other IP Club", &unique_contact("ip")),
        None,
        Some(other_ip),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "другой IP не задет: {response}"
    );

    // Лимит pending на contact: 3 проходят, 4-я (даже с нового IP) → 429.
    let shared_contact = unique_contact("shared");
    for index in 1..=3 {
        let ip = format!("198.51.100.{index}");
        let body = apply_body(&format!("Contact Club {index}"), &shared_contact);
        let (status, response) = apply_with(&env, &body, None, Some(ip.as_str())).await;
        assert_eq!(
            status,
            StatusCode::CREATED,
            "pending №{index} на contact должен пройти: {response}"
        );
    }
    let body = apply_body("Contact Club 4", &shared_contact);
    let (status, response) = apply_with(&env, &body, None, Some("198.51.100.4")).await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "4-я pending на contact: {response}"
    );
    assert_eq!(response["code"], "rate_limited");
}

// ============================================================
// 4. Идемпотентность по Idempotency-Key
// ============================================================

#[tokio::test]
async fn e2e_onboarding_apply_replays_by_idempotency_key() {
    let repo = skip_without_db!();
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    let key = format!("onb-{}", Uuid::now_v7().as_simple());
    let body = apply_body("Idempotent Club", &unique_contact("idem"));

    let (first_status, first) = apply_with(&env, &body, Some(key.as_str()), None).await;
    assert_eq!(first_status, StatusCode::CREATED, "первый POST: {first}");
    let first_id = first["application_id"].as_str().expect("id").to_string();
    let first_submitted = first["submitted_at"].as_str().expect("submitted_at");

    let (second_status, second) = apply_with(&env, &body, Some(key.as_str()), None).await;
    assert_eq!(second_status, StatusCode::OK, "повтор: {second}");
    assert_eq!(second["application_id"].as_str(), Some(first_id.as_str()));
    assert_eq!(second["status"], "pending_moderation");
    assert_eq!(
        second["submitted_at"].as_str(),
        Some(first_submitted),
        "время первой заявки не подменяется"
    );

    let (stored,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM club_application WHERE idempotency_key = $1")
            .bind(&key)
            .fetch_one(&pool)
            .await
            .expect("count by key");
    assert_eq!(stored, 1, "ключ идемпотентности даёт одну строку");

    // Другой ключ — новая заявка (ключ не глобальный «стоп», а ключ запроса).
    let other_key = format!("onb-{}", Uuid::now_v7().as_simple());
    let (status, other) = apply_with(&env, &body, Some(other_key.as_str()), None).await;
    assert_eq!(status, StatusCode::CREATED, "другой ключ: {other}");
    assert_ne!(
        other["application_id"].as_str(),
        Some(first_id.as_str()),
        "другой ключ — другая заявка"
    );
}

// ============================================================
// 5. Админский список: RBAC, фильтр, пагинация
// ============================================================

#[tokio::test]
async fn e2e_onboarding_admin_list_rbac_filter_and_pagination() {
    let repo = skip_without_db!();
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    let _guard = EnvGuard::set("TRUST_PROXY", "1");
    let (_, admin) = admin_token(&env, &pool, "superadmin").await;
    let club = club_token(&env);

    // RBAC: без токена 401, клубу 403.
    let (status, response) = list_applications(&env, None, "").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "без токена: {response}");
    assert_eq!(response["code"], "unauthorized");
    let (status, response) = list_applications(&env, Some(club.as_str()), "").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "клубу: {response}");
    assert_eq!(response["code"], "forbidden");

    // Три заявки (разные IP, чтобы не упереться в лимит 5/час).
    let mut created = Vec::new();
    for (index, name) in ["List One", "List Two", "List Three"].iter().enumerate() {
        let ip = format!("192.0.2.{}", index + 1);
        let body = apply_body(name, &unique_contact("list"));
        created.push((
            name.to_string(),
            create_application(&env, &body, Some(ip.as_str())).await,
        ));
    }

    // По умолчанию — только pending_moderation, новые первыми.
    let (status, page) = list_applications(&env, Some(admin.as_str()), "").await;
    assert_eq!(status, StatusCode::OK, "админу: {page}");
    let items = page["items"].as_array().expect("items").clone();
    assert_eq!(items.len(), 3, "по умолчанию три pending: {page}");
    assert!(items
        .iter()
        .all(|item| item["status"] == "pending_moderation"));
    assert_eq!(page["next_cursor"], Value::Null);

    // keyset-пагинация: две страницы, без дублей и потерь.
    let (status, first_page) = list_applications(&env, Some(admin.as_str()), "?limit=2").await;
    assert_eq!(status, StatusCode::OK, "limit=2: {first_page}");
    let first_items = first_page["items"].as_array().expect("items");
    assert_eq!(first_items.len(), 2);
    let cursor = first_page["next_cursor"].as_str().expect("next_cursor");
    let (status, second_page) = list_applications(
        &env,
        Some(admin.as_str()),
        &format!("?limit=2&cursor={cursor}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "вторая страница: {second_page}");
    let second_items = second_page["items"].as_array().expect("items");
    assert_eq!(second_items.len(), 1);
    assert_eq!(second_page["next_cursor"], Value::Null);

    let mut seen: Vec<String> = first_items
        .iter()
        .chain(second_items.iter())
        .map(|item| item["id"].as_str().expect("id").to_string())
        .collect();
    seen.sort();
    seen.dedup();
    let mut expected: Vec<String> = created.iter().map(|(_, id)| id.to_string()).collect();
    expected.sort();
    assert_eq!(seen, expected, "страницы покрывают все заявки ровно раз");

    // Фильтр по статусу: pending — 3, approved/rejected — пусто.
    let (status, pending) =
        list_applications(&env, Some(admin.as_str()), "?status=pending_moderation").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pending["items"].as_array().map(Vec::len), Some(3));
    let (status, approved) =
        list_applications(&env, Some(admin.as_str()), "?status=approved").await;
    assert_eq!(status, StatusCode::OK);
    assert!(approved["items"].as_array().expect("items").is_empty());

    // Мусорные параметры — 422 с полем, а не молчаливая дефолтная страница.
    for (query, field) in [
        ("?status=bogus", "status"),
        ("?limit=0", "limit"),
        ("?limit=101", "limit"),
        ("?limit=abc", "limit"),
        ("?cursor=not-a-cursor", "cursor"),
    ] {
        let (status, response) = list_applications(&env, Some(admin.as_str()), query).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{query}: {response}"
        );
        assert_eq!(response["code"], "validation_error");
        assert_eq!(response["details"]["field"], field, "{query}: {response}");
    }

    // Карточка: полный payload + решение; неизвестный и битый id — один 404.
    let (_, application_id) = created.first().expect("создана хотя бы одна");
    let (status, card) = get_application(&env, &admin, *application_id).await;
    assert_eq!(status, StatusCode::OK, "карточка: {card}");
    assert_eq!(card["id"], application_id.to_string());
    assert_eq!(card["club"]["name"], created[0].0);
    assert_eq!(card["venue"]["capacity"], 400);
    assert_eq!(card["qr"]["price_minor"], 200);
    assert_eq!(card["status"], "pending_moderation");
    assert_eq!(card["reviewed_by"], Value::Null);
    assert_eq!(card["reviewed_at"], Value::Null);
    assert_eq!(
        card["source_ip"], "192.0.2.1",
        "IP заявки отдаётся без inet-маски"
    );

    let missing = Uuid::now_v7();
    let (status, response) = get_application(&env, &admin, missing).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "неизвестная: {response}");
    assert_eq!(response["code"], "not_found");
    let uri = "/v1/admin/onboarding-applications/not-a-uuid";
    let (status, response) = call(&env, "GET", uri, None, Some(admin.as_str())).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "битый id: {response}");
}

// ============================================================
// 6. Approve: клуб, master-токен хешем, audit, конфликты
// ============================================================

#[tokio::test]
async fn e2e_onboarding_approve_creates_club_and_conflicts() {
    let repo = skip_without_db!();
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    let _guard = EnvGuard::set("TRUST_PROXY", "1");
    let (admin_id, admin) = admin_token(&env, &pool, "superadmin").await;
    let club = club_token(&env);

    let body = apply_body("Клуб Подвал", &unique_contact("approve"));
    let application_id = create_application(&env, &body, Some("203.0.113.21")).await;

    // Клубу approve недоступен (RBAC до валидации тела).
    let (status, response) = approve_application(&env, &club, application_id, json!({})).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "клубу: {response}");

    // Пароль больше не обязателен (инвайт-поток), но если передан —
    // прежняя граница 8 символов. Битый e-mail — 422 email.
    let email = format!("club-{}@approve.test", Uuid::now_v7().as_simple());
    let (status, response) =
        approve_application(&env, &admin, application_id, json!({ "password": "short" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response["details"]["field"], "password");
    let (status, response) = approve_application(
        &env,
        &admin,
        application_id,
        json!({ "email": "not-an-email" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response["details"]["field"], "email");

    // Успешный approve.
    let password = "club-pass-123";
    let (status, approved) = approve_application(
        &env,
        &admin,
        application_id,
        json!({ "email": email, "password": password }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "approve: {approved}");
    assert_eq!(approved["application_id"], application_id.to_string());
    assert_eq!(approved["status"], "approved");
    let club_id: Uuid = approved["club_id"]
        .as_str()
        .expect("club_id")
        .parse()
        .expect("club_id — uuid");
    let master_token = approved["qr_master_token"]
        .as_str()
        .expect("qr_master_token")
        .to_string();
    assert_eq!(master_token.len(), 32, "master-токен — 128 бит (32 hex)");

    // Даже на легаси-пути (password передан) выпускается invite-ссылка:
    // владелец может активировать аккаунт сам, а старый вход продолжает
    // работать. В ответе — email, invite_url и честный mail_queued.
    assert_eq!(approved["email"], email);
    assert_eq!(approved["mail_queued"], true, "письмо поставлено в очередь");
    let invite_url = approved["invite_url"].as_str().expect("invite_url");
    assert!(
        invite_url.contains("/club/invite/"),
        "путь страницы активации: {invite_url}"
    );
    let invite_token = invite_url.rsplit('/').next().expect("токен в ссылке");
    assert_eq!(invite_token.len(), 64, "invite-токен — 64 hex");
    let invite_expires_at = approved["invite_expires_at"]
        .as_str()
        .expect("invite_expires_at");
    let invite_expires_at = chrono::DateTime::parse_from_rfc3339(invite_expires_at)
        .expect("RFC3339")
        .with_timezone(&chrono::Utc);
    let ttl_seconds = (invite_expires_at - chrono::Utc::now()).num_seconds();
    assert!(
        (6 * 24 * 3600..=7 * 24 * 3600 + 60).contains(&ttl_seconds),
        "TTL ссылки ~7 дней: {ttl_seconds} c"
    );

    // Клуб в БД: pending_verification, хеш master-токена, сгенерированный slug.
    let (name, slug, email_db, status_db, master_hash, address, city): (
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT name, slug, email, status, qr_master_token_hash, address, city \
         FROM club WHERE id = $1",
    )
    .bind(club_id)
    .fetch_one(&pool)
    .await
    .expect("клуб должен быть создан");
    assert_eq!(name, "Клуб Подвал");
    assert_eq!(slug, "klub-podval", "slug сгенерирован транслитом");
    // Пароль задан (легаси-путь): хеш Argon2id, а не '!', и invite-токен
    // лежит только sha256-хешем.
    let (password_hash, invite_hash, invite_exp): (
        String,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT password_hash, invite_token_hash, invite_expires_at FROM club WHERE id = $1",
    )
    .bind(club_id)
    .fetch_one(&pool)
    .await
    .expect("клуб с паролем");
    assert!(
        password_hash.starts_with("$argon2"),
        "Argon2id-хеш: {password_hash}"
    );
    assert_eq!(
        invite_hash.as_deref(),
        Some(sha256_hex(invite_token).as_str()),
        "в БД только sha256(invite_token)"
    );
    assert!(invite_exp.is_some(), "срок ссылки сохранён");
    let (plain_invites,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM club WHERE invite_token_hash = $1")
            .bind(invite_token)
            .fetch_one(&pool)
            .await
            .expect("plaintext-токен не должен находиться по колонке хеша");
    assert_eq!(plain_invites, 0);
    // Заявка связана с клубом: по этой связи админка считает invite_state.
    let (linked_club,): (Option<Uuid>,) =
        sqlx::query_as("SELECT club_id FROM club_application WHERE id = $1")
            .bind(application_id)
            .fetch_one(&pool)
            .await
            .expect("заявка");
    assert_eq!(linked_club, Some(club_id));

    assert_eq!(email_db, email);
    assert_eq!(
        status_db, "pending_verification",
        "approve — модерация, а не подтверждение почты"
    );
    let expected_hash = sha256_hex(&master_token);
    assert_eq!(
        master_hash.as_deref(),
        Some(expected_hash.as_str()),
        "в БД только sha256(qr_master_token)"
    );
    assert_eq!(address.as_deref(), Some("Calle Mayor 1"));
    assert_eq!(city.as_deref(), Some("Мадрид"));
    let (plaintext_rows,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM club WHERE qr_master_token_hash = $1")
            .bind(&master_token)
            .fetch_one(&pool)
            .await
            .expect("plaintext не должен находиться по колонке хеша");
    assert_eq!(plaintext_rows, 0);

    // Заявка: approved + reviewed_by/at, причина отказа пуста.
    let (app_status, reviewed_by, reviewed_at, reject_reason): (
        String,
        Option<Uuid>,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT status, reviewed_by, reviewed_at, reject_reason \
         FROM club_application WHERE id = $1",
    )
    .bind(application_id)
    .fetch_one(&pool)
    .await
    .expect("заявка");
    assert_eq!(app_status, "approved");
    assert_eq!(reviewed_by, Some(admin_id));
    assert!(reviewed_at.is_some(), "reviewed_at выставлен");
    assert_eq!(reject_reason, None);

    // Audit есть и привязан к заявке и клубу.
    let (audit_rows, diff): (i64, Option<Value>) = sqlx::query_as(
        "SELECT count(*), min(diff::text)::jsonb \
         FROM audit_log WHERE action = 'club_application.approve' AND entity = $1",
    )
    .bind(application_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("audit");
    assert_eq!(audit_rows, 1, "одна запись approve в audit_log");
    let diff = diff.expect("diff есть");
    assert_eq!(diff["club_id"], club_id.to_string());
    assert_eq!(diff["status"], "approved");

    // Созданный клуб реально входит паролем (pending_verification пускают).
    let (status, login) = call(
        &env,
        "POST",
        "/v1/auth/club/login",
        Some(json!({ "email": email, "password": password })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "логин клуба: {login}");
    assert!(login["access_token"].as_str().is_some());

    // Повторный approve → 409 already_reviewed (второй клуб не создаётся).
    let (status, response) = approve_application(
        &env,
        &admin,
        application_id,
        json!({ "email": email, "password": password }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "повтор approve: {response}");
    assert_eq!(response["code"], "already_reviewed");
    let (clubs,): (i64,) = sqlx::query_as("SELECT count(*) FROM club")
        .fetch_one(&pool)
        .await
        .expect("count club");
    assert_eq!(clubs, 1, "повторный approve не создал второй клуб");

    // Reject после approve → тот же 409.
    let (status, response) =
        reject_application(&env, &admin, application_id, json!({ "reason": "поздно" })).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(response["code"], "already_reviewed");

    // Taken email → 409 email_taken (имя клуба то же, что у первой заявки,
    // чтобы ниже проверить и коллизию сгенерированного slug).
    let second_id = create_application(
        &env,
        &apply_body("Клуб Подвал", &unique_contact("approve2")),
        Some("203.0.113.22"),
    )
    .await;
    let (status, response) = approve_application(
        &env,
        &admin,
        second_id,
        json!({ "email": email, "password": password }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "занятый email: {response}");
    assert_eq!(response["code"], "email_taken");
    // Заявка не испорчена: осталась pending для повторной попытки.
    let (pending,): (String,) = sqlx::query_as("SELECT status FROM club_application WHERE id = $1")
        .bind(second_id)
        .fetch_one(&pool)
        .await
        .expect("заявка");
    assert_eq!(pending, "pending_moderation");

    // Коллизия сгенерированного slug → суффикс -2.
    let (status, second_approved) = approve_application(
        &env,
        &admin,
        second_id,
        json!({ "email": format!("second-{}@approve.test", Uuid::now_v7().as_simple()), "password": password }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "второй approve: {second_approved}");
    let second_club: Uuid = second_approved["club_id"]
        .as_str()
        .expect("club_id")
        .parse()
        .expect("uuid");
    let (second_slug,): (String,) = sqlx::query_as("SELECT slug FROM club WHERE id = $1")
        .bind(second_club)
        .fetch_one(&pool)
        .await
        .expect("slug");
    assert_eq!(second_slug, "klub-podval-2", "суффикс при коллизии slug");

    // Явный slug: сохраняется как есть; занятый → 409 slug_taken.
    let third_id = create_application(
        &env,
        &apply_body("Explicit Slug Club", &unique_contact("approve3")),
        Some("203.0.113.23"),
    )
    .await;
    let (status, third) = approve_application(
        &env,
        &admin,
        third_id,
        json!({ "email": format!("third-{}@approve.test", Uuid::now_v7().as_simple()), "password": password, "slug": "Explicit-Slug" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "явный slug: {third}");
    let third_club: Uuid = third["club_id"]
        .as_str()
        .expect("club_id")
        .parse()
        .expect("uuid");
    let (explicit_slug,): (String,) = sqlx::query_as("SELECT slug FROM club WHERE id = $1")
        .bind(third_club)
        .fetch_one(&pool)
        .await
        .expect("slug");
    assert_eq!(explicit_slug, "explicit-slug", "регистр нормализован");

    let fourth_id = create_application(
        &env,
        &apply_body("Another Club", &unique_contact("approve4")),
        Some("203.0.113.24"),
    )
    .await;
    let (status, response) = approve_application(
        &env,
        &admin,
        fourth_id,
        json!({ "email": format!("fourth-{}@approve.test", Uuid::now_v7().as_simple()), "password": password, "slug": "explicit-slug" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "занятый slug: {response}");
    assert_eq!(response["code"], "slug_taken");
}

// ============================================================
// 6b. Инвайт-поток: approve без пароля → превью → активация → вход
// ============================================================

#[tokio::test]
async fn e2e_onboarding_invite_activation_lifecycle() {
    let repo = skip_without_db!();
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    let _guard = EnvGuard::set("TRUST_PROXY", "1");
    let (_, admin) = admin_token(&env, &pool, "superadmin").await;

    let contact = unique_contact("invite");
    let application_id = create_application(
        &env,
        &apply_body("Barrio Latino", &contact),
        Some("203.0.113.51"),
    )
    .await;

    // Approve БЕЗ пароля и без явного e-mail: адрес берётся из заявки,
    // клуб создаётся нерабочим для входа (password_hash = '!').
    let (status, approved) = approve_application(&env, &admin, application_id, json!({})).await;
    assert_eq!(status, StatusCode::OK, "approve без пароля: {approved}");
    assert_eq!(approved["status"], "approved");
    assert_eq!(approved["email"], contact, "email из заявки");
    assert_eq!(approved["mail_queued"], true, "письмо поставлено в очередь");
    let club_id: Uuid = approved["club_id"]
        .as_str()
        .expect("club_id")
        .parse()
        .expect("uuid");
    let token = token_from_url(&approved);
    let expires_at = approved["invite_expires_at"]
        .as_str()
        .expect("invite_expires_at")
        .to_string();

    let (password_hash, status_db, invite_hash, invite_exp) =
        club_invite_state(&pool, club_id).await;
    assert_eq!(password_hash, "!", "до активации вход по паролю запрещён");
    assert_eq!(status_db, "pending_verification");
    assert_eq!(
        invite_hash.as_deref(),
        Some(sha256_hex(&token).as_str()),
        "в БД только sha256(invite_token)"
    );
    assert!(invite_exp.is_some(), "срок ссылки сохранён");
    let (plaintext_rows,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM club WHERE invite_token_hash = $1")
            .bind(&token)
            .fetch_one(&pool)
            .await
            .expect("plaintext не должен находиться по колонке хеша");
    assert_eq!(plaintext_rows, 0);

    // Превью отдаёт данные заявки и ничего сверх них; срок — тот же.
    let (status, preview) = invite_preview(&env, &token).await;
    assert_eq!(status, StatusCode::OK, "превью: {preview}");
    assert_eq!(preview["club_name"], "Barrio Latino");
    assert_eq!(preview["city"], "Мадрид");
    assert_eq!(preview["email"], contact);
    assert_eq!(preview["contact"], contact);
    assert_eq!(preview["slug"], "barrio-latino");
    assert_eq!(preview["expires_at"], expires_at);

    // До активации пароля нет: любой ввод — «invalid credentials».
    let (status, _) = call(
        &env,
        "POST",
        "/v1/auth/club/login",
        Some(json!({ "email": contact, "password": "not-set-yet" })),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "клуб без пароля не входит"
    );

    // Слабый/отсутствующий пароль → 422 validation_failed field=password.
    for body in [json!({ "password": "short" }), json!({})] {
        let (status, response) = invite_accept(&env, &token, body).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "слабый пароль: {response}"
        );
        assert_eq!(response["code"], "validation_failed");
        assert_eq!(response["details"]["field"], "password");
    }

    // Успешная активация: TokenPair + club, статус active.
    let password = "club-invite-pass";
    let (status, accepted) = invite_accept(&env, &token, json!({ "password": password })).await;
    assert_eq!(status, StatusCode::OK, "активация: {accepted}");
    assert!(accepted["access_token"].as_str().is_some());
    assert!(accepted["refresh_token"].as_str().is_some());
    assert_eq!(accepted["token_type"], "Bearer");
    assert!(accepted["expires_in"].as_i64().is_some());
    assert_eq!(accepted["club"]["id"], club_id.to_string());
    assert_eq!(accepted["club"]["name"], "Barrio Latino");
    assert_eq!(accepted["club"]["slug"], "barrio-latino");
    assert_eq!(accepted["club"]["email"], contact);

    let (password_hash, status_db, invite_hash, invite_exp) =
        club_invite_state(&pool, club_id).await;
    assert!(
        password_hash.starts_with("$argon2"),
        "Argon2id-хеш: {password_hash}"
    );
    assert_eq!(status_db, "active", "владение e-mail подтверждено ссылкой");
    assert_eq!(invite_hash, None, "токен погашен");
    assert_eq!(invite_exp, None, "срок снят вместе с токеном");

    let (audit_rows,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM audit_log \
         WHERE action = 'club.invite.accept' AND entity = $1",
    )
    .bind(club_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("audit");
    assert_eq!(audit_rows, 1, "одна запись club.invite.accept");

    // Клуб реально входит заданным паролем.
    let (status, login) = call(
        &env,
        "POST",
        "/v1/auth/club/login",
        Some(json!({ "email": contact, "password": password })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "логин после активации: {login}");
    assert!(login["access_token"].as_str().is_some());

    // Одноразовость: та же ссылка в превью и в accept → 404 invite_not_found.
    let (status, response) = invite_preview(&env, &token).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "повтор превью: {response}");
    assert_eq!(response["code"], "invite_not_found");
    let (status, response) =
        invite_accept(&env, &token, json!({ "password": "another-pass" })).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "повтор accept: {response}");
    assert_eq!(response["code"], "invite_not_found");

    // Админский список и карточка: клуб связан с заявкой, ссылка принята.
    let (status, page) = list_applications(&env, Some(&admin), "?status=approved").await;
    assert_eq!(status, StatusCode::OK, "список: {page}");
    let item = page["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|item| item["id"] == application_id.to_string())
        .expect("заявка в списке approved");
    assert_eq!(item["club_id"], club_id.to_string());
    assert_eq!(item["invite_state"], "accepted");

    let (status, card) = get_application(&env, &admin, application_id).await;
    assert_eq!(status, StatusCode::OK, "карточка: {card}");
    assert_eq!(card["club_id"], club_id.to_string());
    assert_eq!(card["invite_state"], "accepted");
}

// ============================================================
// 6c. Перевыпуск ссылки: истечение, новый токен, guard-ы
// ============================================================

#[tokio::test]
async fn e2e_onboarding_invite_reissue_expiry_and_guards() {
    let repo = skip_without_db!();
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    let _guard = EnvGuard::set("TRUST_PROXY", "1");
    let (_, admin) = admin_token(&env, &pool, "superadmin").await;
    let (_, readonly) = admin_token(&env, &pool, "readonly").await;

    let contact = unique_contact("reissue");
    let application_id = create_application(
        &env,
        &apply_body("Reissue Club", &contact),
        Some("203.0.113.61"),
    )
    .await;
    let (status, approved) = approve_application(&env, &admin, application_id, json!({})).await;
    assert_eq!(status, StatusCode::OK, "approve: {approved}");
    let club_id: Uuid = approved["club_id"]
        .as_str()
        .expect("club_id")
        .parse()
        .expect("uuid");
    let first_token = token_from_url(&approved);

    // Пока ссылка жива — invite_state = pending.
    let (status, card) = get_application(&env, &admin, application_id).await;
    assert_eq!(status, StatusCode::OK, "карточка: {card}");
    assert_eq!(card["club_id"], club_id.to_string());
    assert_eq!(card["invite_state"], "pending");

    // Истекаем ссылку (TTL проверяется по БД, а не по клиентским часам).
    sqlx::query("UPDATE club SET invite_expires_at = now() - interval '1 hour' WHERE id = $1")
        .bind(club_id)
        .execute(&pool)
        .await
        .expect("ссылку можно состарить");
    let (status, response) = invite_preview(&env, &first_token).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "истёкшая ссылка: {response}");
    assert_eq!(response["code"], "invite_not_found");
    let (status, response) =
        invite_accept(&env, &first_token, json!({ "password": "expired-pass-1" })).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "accept по истёкшей ссылке: {response}"
    );
    assert_eq!(response["code"], "invite_not_found");
    let (status, card) = get_application(&env, &admin, application_id).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(card["invite_state"], "expired");
    let (status, page) = list_applications(&env, Some(&admin), "?status=approved").await;
    assert_eq!(status, StatusCode::OK);
    let item = page["items"]
        .as_array()
        .expect("items")
        .iter()
        .find(|item| item["id"] == application_id.to_string())
        .expect("заявка");
    assert_eq!(item["invite_state"], "expired");

    // Перевыпуск: новый токен, новый TTL, письмо снова в очереди.
    let (status, reissued) = reissue_invite(&env, &admin, application_id).await;
    assert_eq!(status, StatusCode::OK, "перевыпуск: {reissued}");
    assert_eq!(reissued["email"], contact);
    assert_eq!(reissued["mail_queued"], true);
    let second_token = token_from_url(&reissued);
    assert_ne!(second_token, first_token, "перевыпуск меняет токен");
    let (status, response) = invite_preview(&env, &first_token).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "старая ссылка погашена: {response}"
    );
    let (status, preview) = invite_preview(&env, &second_token).await;
    assert_eq!(status, StatusCode::OK, "новая ссылка: {preview}");

    // В очереди почты — approve + перевыпуск (разные токены → разные ключи).
    let (mail_rows,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM mail_message \
         WHERE template = 'club_invite' AND aggregate_type = 'club' AND aggregate_id = $1",
    )
    .bind(club_id)
    .fetch_one(&pool)
    .await
    .expect("mail_message");
    assert_eq!(mail_rows, 2, "два письма club_invite: approve и reissue");
    // Письмо содержит рабочую ссылку и адрес владельца из заявки.
    let (mail_to, mail_url): (String, Option<String>) = sqlx::query_as(
        "SELECT to_email::text, vars->>'invite_url' FROM mail_message \
         WHERE template = 'club_invite' AND aggregate_id = $1 \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(club_id)
    .fetch_one(&pool)
    .await
    .expect("последнее письмо club_invite");
    assert_eq!(mail_to, contact, "письмо уходит владельцу из заявки");
    assert_eq!(
        mail_url.as_deref(),
        reissued["invite_url"].as_str(),
        "в письме та же ссылка, что вернул перевыпуск"
    );

    // Новая ссылка активирует клуб.
    let (status, accepted) =
        invite_accept(&env, &second_token, json!({ "password": "reissued-pass" })).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "активация новой ссылкой: {accepted}"
    );

    // После активации перевыпуск запрещён.
    let (status, response) = reissue_invite(&env, &admin, application_id).await;
    assert_eq!(status, StatusCode::CONFLICT, "уже активирован: {response}");
    assert_eq!(response["code"], "invite_already_accepted");

    // Неодобренная заявка → 409 not_approved.
    let pending_id = create_application(
        &env,
        &apply_body("Pending Reissue Club", &unique_contact("reissue-pending")),
        Some("203.0.113.62"),
    )
    .await;
    let (status, response) = reissue_invite(&env, &admin, pending_id).await;
    assert_eq!(status, StatusCode::CONFLICT, "pending: {response}");
    assert_eq!(response["code"], "not_approved");

    // readonly-админ не перевыпускает ссылку (мутация).
    let readonly_app = create_application(
        &env,
        &apply_body("Readonly Reissue Club", &unique_contact("reissue-readonly")),
        Some("203.0.113.63"),
    )
    .await;
    let (status, approved) = approve_application(&env, &admin, readonly_app, json!({})).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "approve для readonly-теста: {approved}"
    );
    let (status, response) = reissue_invite(&env, &readonly, readonly_app).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "readonly: {response}");
    assert_eq!(response["code"], "forbidden");

    // Неизвестная заявка — 404 not_found (не «уже активирована»).
    let (status, response) = reissue_invite(&env, &admin, Uuid::now_v7()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "неизвестная: {response}");
    assert_eq!(response["code"], "not_found");

    // Заявка без e-mail (телефон в contact): approve без email → 422 email.
    let phone_id = create_application(
        &env,
        &apply_body("Phone Club", "+34 600 000 001"),
        Some("203.0.113.64"),
    )
    .await;
    let (status, response) = approve_application(&env, &admin, phone_id, json!({})).await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "без e-mail: {response}"
    );
    assert_eq!(response["details"]["field"], "email");
    let (status, phone_approved) = approve_application(
        &env,
        &admin,
        phone_id,
        json!({ "email": "phone-club@approve.test" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "с явным email: {phone_approved}");
    assert_eq!(phone_approved["email"], "phone-club@approve.test");
}

// ============================================================
// 7. Reject: причина, audit, повтор
// ============================================================

#[tokio::test]
async fn e2e_onboarding_reject_records_reason_and_audit() {
    let repo = skip_without_db!();
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    let _guard = EnvGuard::set("TRUST_PROXY", "1");
    let (admin_id, admin) = admin_token(&env, &pool, "operator").await;

    let body = apply_body("Rejected Club", &unique_contact("reject"));
    let application_id = create_application(&env, &body, Some("203.0.113.31")).await;

    // Причина обязательна и ограничена 500 символами.
    let (status, response) = reject_application(&env, &admin, application_id, json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response["details"]["field"], "reason");
    let (status, response) =
        reject_application(&env, &admin, application_id, json!({ "reason": "  " })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response["details"]["field"], "reason");
    let (status, response) = reject_application(
        &env,
        &admin,
        application_id,
        json!({ "reason": "x".repeat(501) }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response["details"]["field"], "reason");

    // Успешный reject.
    let reason = "Не подтвердили адрес площадки";
    let (status, rejected) =
        reject_application(&env, &admin, application_id, json!({ "reason": reason })).await;
    assert_eq!(status, StatusCode::OK, "reject: {rejected}");
    assert_eq!(rejected["application_id"], application_id.to_string());
    assert_eq!(rejected["status"], "rejected");

    let (app_status, stored_reason, reviewed_by, reviewed_at): (
        String,
        Option<String>,
        Option<Uuid>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT status, reject_reason, reviewed_by, reviewed_at \
         FROM club_application WHERE id = $1",
    )
    .bind(application_id)
    .fetch_one(&pool)
    .await
    .expect("заявка");
    assert_eq!(app_status, "rejected");
    assert_eq!(stored_reason.as_deref(), Some(reason));
    assert_eq!(reviewed_by, Some(admin_id));
    assert!(reviewed_at.is_some());

    let (audit_rows,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM audit_log \
         WHERE action = 'club_application.reject' AND entity = $1",
    )
    .bind(application_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("audit");
    assert_eq!(audit_rows, 1, "одна запись reject в audit_log");

    // Клуб не создан.
    let (clubs,): (i64,) = sqlx::query_as("SELECT count(*) FROM club")
        .fetch_one(&pool)
        .await
        .expect("count club");
    assert_eq!(clubs, 0, "reject не создаёт клуб");

    // Повтор reject → 409; approve после reject → 409.
    let (status, response) =
        reject_application(&env, &admin, application_id, json!({ "reason": reason })).await;
    assert_eq!(status, StatusCode::CONFLICT, "повтор reject: {response}");
    assert_eq!(response["code"], "already_reviewed");
    let (status, response) = approve_application(
        &env,
        &admin,
        application_id,
        json!({ "email": "after-reject@approve.test", "password": "club-pass-123" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "approve после reject: {response}"
    );
    assert_eq!(response["code"], "already_reviewed");
}

// ============================================================
// 8. readonly-админ: читать можно, решать нельзя
// ============================================================

#[tokio::test]
async fn e2e_onboarding_readonly_admin_cannot_review() {
    let repo = skip_without_db!();
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    let _guard = EnvGuard::set("TRUST_PROXY", "1");
    let (_, readonly) = admin_token(&env, &pool, "readonly").await;

    let body = apply_body("Readonly Review Club", &unique_contact("readonly"));
    let application_id = create_application(&env, &body, Some("203.0.113.41")).await;

    let (status, response) = approve_application(
        &env,
        &readonly,
        application_id,
        json!({ "email": "readonly@approve.test", "password": "club-pass-123" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "readonly approve: {response}"
    );
    assert_eq!(response["code"], "forbidden");
    let (status, response) =
        reject_application(&env, &readonly, application_id, json!({ "reason": "нет" })).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "readonly reject: {response}");
    assert_eq!(response["code"], "forbidden");

    // Чтение readonly-админу доступно, заявка осталась pending.
    let (status, page) = list_applications(&env, Some(readonly.as_str()), "").await;
    assert_eq!(status, StatusCode::OK, "readonly list: {page}");
    assert_eq!(page["items"].as_array().map(Vec::len), Some(1));
    let (status, card) = get_application(&env, &readonly, application_id).await;
    assert_eq!(status, StatusCode::OK, "readonly card: {card}");
    let (app_status,): (String,) =
        sqlx::query_as("SELECT status FROM club_application WHERE id = $1")
            .bind(application_id)
            .fetch_one(&pool)
            .await
            .expect("заявка");
    assert_eq!(app_status, "pending_moderation");
    let (clubs,): (i64,) = sqlx::query_as("SELECT count(*) FROM club")
        .fetch_one(&pool)
        .await
        .expect("count club");
    assert_eq!(clubs, 0, "readonly ничего не создал");
}
