//! E2e админского каталога пользователей (`admin_users_routes`).
//!
//! Покрывает §7.1/§7.6 `docs/audit/web-admin-gaps.md`:
//!   * RBAC-гейт (401 без токена, 403 чужим ролям и readonly-админу),
//!   * объединённый список 4 ролей, фильтры, экранирование `q`, keyset-
//!     пагинацию по (created_at, id),
//!   * создание админа с одноразовым TOTP-enrollment (секрет шифруется,
//!     пароль — Argon2id), `role_not_creatable` для не-админов,
//!   * PATCH: смена статуса отзывает refresh, self-role/self-block 422,
//!     последний superadmin 409, email-конфликт 409,
//!   * DELETE: soft-block + revoke, self-delete 422, последний superadmin 409,
//!   * readonly-админ: GET 200, мутации 403 (строка в `admin_user` создаётся
//!     напрямую — другого способа выдать readonly в тесте нет),
//!   * bulk: affected/ids, одна запись аудита, отзыв refresh, `role`.
//!
//! Все тесты используют общий харнесс `crate::e2e` и живую Postgres; без
//! `DATABASE_URL` e2e ранний-выходит (как остальные файлы), запуск —
//! `./scripts/db-test.sh cargo test -p dropit-gateway-api --bin dropit-api
//! e2e_admin_users -- --test-threads=1`.

#![cfg(test)]

use axum::http::StatusCode;
use dropit_common::Role;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::e2e::{app_env, call, test_repo};
use crate::routes::ApiEnv;

// ============================================================
// Фикстуры
// ============================================================

/// Создаёт строку `admin_user` и валидный access-JWT для неё.
async fn admin_fixture(
    env: &ApiEnv,
    pool: &PgPool,
    admin_role: &str,
    email: &str,
) -> (Uuid, String) {
    let id = Uuid::now_v7();
    let secret = b"JBSWY3DPEHPK3PXP".to_vec();
    sqlx::query(
        "INSERT INTO admin_user \
         (id, email, password_hash, totp_secret_enc, totp_confirmed_at, role, status) \
         VALUES ($1, $2, 'x', $3, now(), $4, 'active')",
    )
    .bind(id)
    .bind(email)
    .bind(secret)
    .bind(admin_role)
    .execute(pool)
    .await
    .unwrap();
    let token = env
        .keys
        .issue_access(id, Role::Admin, Uuid::now_v7())
        .unwrap();
    (id, token)
}

async fn create_club(pool: &PgPool, name: &str, email: &str, status: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO club (id, name, slug, email, password_hash, status) \
         VALUES ($1, $2, $3, $4, 'x', $5)",
    )
    .bind(id)
    .bind(name)
    .bind(format!("club-{id}"))
    .bind(email)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn create_dj(pool: &PgPool, display_name: &str, email: &str, status: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO dj (id, email, password_hash, display_name, status) \
         VALUES ($1, $2, 'x', $3, $4)",
    )
    .bind(id)
    .bind(email)
    .bind(display_name)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn create_guest(pool: &PgPool, email: &str, verified: bool) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO guest_account (id, email_normalized, email_display, verified) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(email.to_lowercase())
    .bind(email)
    .bind(verified)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn add_guest_device(pool: &PgPool, account_id: Uuid, display_name: &str) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO guest_device (id, account_id, platform, display_name, last_seen) \
         VALUES ($1, $2, 'ios', $3, now())",
    )
    .bind(id)
    .bind(account_id)
    .bind(display_name)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// Живой refresh-токен субъекта (для проверки отзыва при блокировке).
async fn add_refresh(pool: &PgPool, subject_type: &str, subject_id: Uuid) {
    sqlx::query(
        "INSERT INTO refresh_token (id, family_id, subject_type, subject_id, token_hash, expires_at) \
         VALUES ($1, $2, $3, $4, $5, now() + interval '30 days')",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .bind(subject_type)
    .bind(subject_id)
    // Уникальный хеш: один субъект может получить новую семью после ревока.
    .bind(format!("hash-{subject_id}-{}", Uuid::now_v7()))
    .execute(pool)
    .await
    .unwrap();
}

async fn live_refresh_count(pool: &PgPool, subject_type: &str, subject_id: Uuid) -> i64 {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM refresh_token \
         WHERE subject_type = $1 AND subject_id = $2 AND revoked_at IS NULL",
    )
    .bind(subject_type)
    .bind(subject_id)
    .fetch_one(pool)
    .await
    .unwrap();
    count
}

/// `e2e::test_repo` чистит `guest_device`, но не `guest_account` (её не было
/// в списке TRUNCATE харнесса), поэтому аккаунты гостей от прошлых прогонов
/// копятся. Свои тесты чистим сами — прогон и так сериализован `db-test.sh`.
async fn reset_guest_accounts(pool: &PgPool) {
    sqlx::query("TRUNCATE TABLE guest_account CASCADE")
        .execute(pool)
        .await
        .expect("truncate guest_account");
}

async fn audit_count(pool: &PgPool, action: &str) -> i64 {
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM audit_log WHERE action = $1")
        .bind(action)
        .fetch_one(pool)
        .await
        .unwrap();
    count
}

fn roles_of(body: &Value) -> Vec<String> {
    body["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["role"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Percent-encode курсора для подстановки в query (курсор содержит `\x1f`
/// и `+00:00`, которые нельзя класть в URL как есть).
fn encode_cursor(raw: &str) -> String {
    let mut out = String::new();
    for byte in raw.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

// ============================================================
// 1. RBAC-гейт и матрица ролей
// ============================================================

#[tokio::test]
async fn e2e_admin_users_access_and_roles_matrix() {
    let repo = match test_repo().await {
        Some(repo) => repo,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e_admin_users пропущен");
            return;
        }
    };
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    reset_guest_accounts(&pool).await;
    let (_admin_id, admin_token) =
        admin_fixture(&env, &pool, "superadmin", "gate-admin@dropit.test").await;

    // --- 401 без токена ---
    for uri in ["/v1/admin/users", "/v1/admin/roles"] {
        let (status, body) = call(&env, "GET", uri, None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri} без токена: {body}");
        assert_eq!(body["code"], "unauthorized");
    }

    // --- 403 чужим ролям (клуб, DJ, гость) ---
    let club_token = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Club, Uuid::now_v7())
        .unwrap();
    let dj_token = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Dj, Uuid::now_v7())
        .unwrap();
    let guest_token = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Guest, Uuid::now_v7())
        .unwrap();
    for (label, token) in [
        ("club", &club_token),
        ("dj", &dj_token),
        ("guest", &guest_token),
    ] {
        for uri in ["/v1/admin/users", "/v1/admin/roles"] {
            let (status, body) = call(&env, "GET", uri, None, Some(token)).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{label} на {uri}: {body}");
            assert_eq!(body["code"], "forbidden");
        }
        // Запись тоже закрыта ролью (403, а не «нет БД»).
        let (status, body) = call(
            &env,
            "POST",
            "/v1/admin/users",
            Some(json!({ "role": "admin" })),
            Some(token),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{label} создаёт админа: {body}"
        );
        assert_eq!(body["code"], "forbidden");
    }

    // --- 200 админу: список (пустой — валидный ответ) ---
    let (status, body) = call(&env, "GET", "/v1/admin/users", None, Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK, "admin list: {body}");
    assert!(body["items"].is_array());
    assert!(body["next_cursor"].is_null());

    // --- матрица ролей: 4 principal + 3 admin-подроли ---
    let (status, body) = call(&env, "GET", "/v1/admin/roles", None, Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK, "roles: {body}");
    let roles = body["roles"].as_array().unwrap();
    assert_eq!(roles.len(), 4);
    let codes: Vec<&str> = roles.iter().filter_map(|r| r["code"].as_str()).collect();
    for expected in ["guest", "dj", "club", "admin"] {
        assert!(codes.contains(&expected), "нет роли {expected}: {codes:?}");
    }
    for role in roles {
        let capabilities = role["capabilities"].as_array().unwrap();
        assert!(
            !capabilities.is_empty(),
            "роль {} без возможностей",
            role["code"]
        );
        for capability in capabilities {
            assert!(capability["code"].is_string());
            assert!(capability["allowed"].is_boolean());
            assert!(capability["enforced_by"].is_string());
        }
    }
    let admin_roles = body["admin_roles"].as_array().unwrap();
    assert_eq!(admin_roles.len(), 3);
    let readonly = admin_roles
        .iter()
        .find(|r| r["code"] == "readonly")
        .expect("readonly в матрице");
    assert_eq!(readonly["can_write"], false);
    let operator = admin_roles
        .iter()
        .find(|r| r["code"] == "operator")
        .expect("operator в матрице");
    assert_eq!(operator["can_write"], true);
    assert_eq!(operator["can_manage_users"], false);
    let superadmin = admin_roles
        .iter()
        .find(|r| r["code"] == "superadmin")
        .expect("superadmin в матрице");
    assert_eq!(superadmin["can_manage_users"], true);

    // --- без БД: чтение и запись отвечают 503 в конверте, не паникой ---
    let no_db = app_env(None, None, None).await;
    let (status, body) = call(&no_db, "GET", "/v1/admin/users", None, Some(&admin_token)).await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "no db list: {body}"
    );
    assert_eq!(body["code"], "external_unavailable");
    let (status, body) = call(
        &no_db,
        "POST",
        "/v1/admin/users",
        Some(json!({ "role": "admin" })),
        Some(&admin_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "no db create: {body}"
    );
}

// ============================================================
// 2. Список: 4 роли, фильтры, q, keyset-пагинация
// ============================================================

#[tokio::test]
async fn e2e_admin_users_directory_filters_and_pagination() {
    let repo = match test_repo().await {
        Some(repo) => repo,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e_admin_users пропущен");
            return;
        }
    };
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    reset_guest_accounts(&pool).await;
    let (admin_id, admin_token) =
        admin_fixture(&env, &pool, "superadmin", "list-admin@dropit.test").await;
    sqlx::query("UPDATE admin_user SET display_name = 'Root Admin' WHERE id = $1")
        .bind(admin_id)
        .execute(&pool)
        .await
        .unwrap();

    let club_active = create_club(&pool, "Alpha Club", "alpha@club.test", "active").await;
    let club_pending = create_club(
        &pool,
        "100% Night",
        "percent@club.test",
        "pending_verification",
    )
    .await;
    let dj_id = create_dj(&pool, "Beta DJ", "beta@dj.test", "invited").await;
    let guest_id = create_guest(&pool, "gamma@guest.test", false).await;
    add_guest_device(&pool, guest_id, "Gamma Guest").await;

    // --- все 4 роли в списке ---
    let (status, body) = call(
        &env,
        "GET",
        "/v1/admin/users?limit=200",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "list: {body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 5, "admin+2 клуба+dj+гость: {body}");
    let roles = roles_of(&body);
    for expected in ["admin", "club_staff", "dj", "guest"] {
        assert!(
            roles.contains(&expected.to_string()),
            "нет роли {expected}: {roles:?}"
        );
    }
    // Поля контракта присутствуют и заполнены.
    let guest_item = items
        .iter()
        .find(|item| item["id"] == guest_id.to_string())
        .expect("гость в каталоге");
    assert_eq!(guest_item["role"], "guest");
    assert_eq!(guest_item["status"], "invited");
    assert_eq!(guest_item["display_name"], "Gamma Guest");
    assert_eq!(guest_item["email"], "gamma@guest.test");
    assert!(guest_item["club_id"].is_null());
    assert!(guest_item["last_seen_at"].is_string());
    let club_item = items
        .iter()
        .find(|item| item["id"] == club_active.to_string())
        .expect("клуб в каталоге");
    assert_eq!(club_item["role"], "club_staff");
    assert_eq!(club_item["club_id"], club_active.to_string());
    assert_eq!(club_item["status"], "active");
    let dj_item = items
        .iter()
        .find(|item| item["id"] == dj_id.to_string())
        .expect("dj в каталоге");
    assert_eq!(dj_item["role"], "dj");
    assert_eq!(dj_item["status"], "invited");

    // --- фильтр role ---
    let (status, body) = call(
        &env,
        "GET",
        "/v1/admin/users?role=dj",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "role=dj: {body}");
    assert_eq!(items[0]["id"], dj_id.to_string());

    // --- фильтр status=invited: pending-клуб + invited DJ + невериф. гость ---
    let (status, body) = call(
        &env,
        "GET",
        "/v1/admin/users?status=invited",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let invited: Vec<String> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(invited.len(), 3, "status=invited: {body}");
    assert!(invited.contains(&club_pending.to_string()));
    assert!(invited.contains(&dj_id.to_string()));
    assert!(invited.contains(&guest_id.to_string()));

    // --- комбинация role+status ---
    let (status, body) = call(
        &env,
        "GET",
        "/v1/admin/users?role=club_staff&status=invited",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["id"], club_pending.to_string());

    // --- q: простой поиск ---
    let (status, body) = call(
        &env,
        "GET",
        "/v1/admin/users?q=Alpha",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["id"], club_active.to_string());

    // --- q с `%`: экранирование, а не «всё подряд» ---
    let (status, body) = call(
        &env,
        "GET",
        "/v1/admin/users?q=100%25",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "q=100%: {body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(
        items.len(),
        1,
        "q=100% должен найти только «100% Night»: {body}"
    );
    assert_eq!(items[0]["id"], club_pending.to_string());

    // --- q=`%`: ровно один литеральный процент, не весь каталог ---
    let (status, body) = call(
        &env,
        "GET",
        "/v1/admin/users?q=%25",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "q=%: {body}");
    assert_eq!(
        body["items"].as_array().unwrap().len(),
        1,
        "literal % не должен матчить всех: {body}"
    );

    // --- q=`_`: литеральное подчёркивание, совпадений нет ---
    let (status, body) = call(&env, "GET", "/v1/admin/users?q=_", None, Some(&admin_token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["items"].as_array().unwrap().is_empty(),
        "literal _ не должен быть wildcard: {body}"
    );

    // --- keyset-пагинация: 5 записей, limit=3 → 2 страницы без пересечений ---
    let (status, page1) = call(
        &env,
        "GET",
        "/v1/admin/users?limit=3",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let page1_items = page1["items"].as_array().unwrap().clone();
    assert_eq!(page1_items.len(), 3);
    let cursor = page1["next_cursor"]
        .as_str()
        .expect("next_cursor на первой странице")
        .to_string();
    assert!(cursor.contains('\u{1f}'));

    let (status, page2) = call(
        &env,
        "GET",
        &format!("/v1/admin/users?limit=3&cursor={}", encode_cursor(&cursor)),
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "page2: {page2}");
    let page2_items = page2["items"].as_array().unwrap();
    assert_eq!(page2_items.len(), 2, "вторая страница: {page2}");
    assert!(page2["next_cursor"].is_null());

    let mut seen: Vec<String> = page1_items
        .iter()
        .chain(page2_items.iter())
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect();
    let total = seen.len();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), total, "страницы пересекаются: {seen:?}");
    assert_eq!(total, 5, "обе страницы вместе = весь каталог");

    // --- одна запись по id ---
    let (status, body) = call(
        &env,
        "GET",
        &format!("/v1/admin/users/{club_active}"),
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "get one: {body}");
    assert_eq!(body["id"], club_active.to_string());
    assert_eq!(body["role"], "club_staff");
    assert_eq!(body["display_name"], "Alpha Club");

    let (status, body) = call(
        &env,
        "GET",
        &format!("/v1/admin/users/{}", Uuid::now_v7()),
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "нет записи: {body}");
    assert_eq!(body["code"], "not_found");

    // --- мусорные query → 400 в конверте, а не text/plain ---
    for bad in [
        "/v1/admin/users?limit=abc",
        "/v1/admin/users?role=owner",
        "/v1/admin/users?status=deleted",
        "/v1/admin/users?cursor=garbage",
    ] {
        let (status, body) = call(&env, "GET", bad, None, Some(&admin_token)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}: {body}");
        assert_eq!(body["code"], "bad_request", "{bad}: {body}");
    }
    // limit клампится (0 → 1), пустой cursor — отсутствие курсора: 200.
    let (status, body) = call(
        &env,
        "GET",
        "/v1/admin/users?limit=0&cursor=",
        None,
        Some(&admin_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "limit=0: {body}");
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
}

// ============================================================
// 3. Создание администратора (TOTP-enrollment + Argon2id)
// ============================================================

#[tokio::test]
async fn e2e_admin_users_create_admin_with_enrollment() {
    let repo = match test_repo().await {
        Some(repo) => repo,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e_admin_users пропущен");
            return;
        }
    };
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    reset_guest_accounts(&pool).await;
    let (_actor_id, actor_token) =
        admin_fixture(&env, &pool, "superadmin", "create-actor@dropit.test").await;

    let password = "very-strong-pass-42";
    let email = "created-admin@dropit.test";
    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/users",
        Some(json!({
            "role": "admin",
            "email": email,
            "display_name": "Created Admin",
            "password": password,
        })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create admin: {body}");
    let id = body["id"].as_str().expect("id").parse::<Uuid>().unwrap();
    assert_eq!(body["role"], "admin");
    assert_eq!(body["admin_role"], "operator", "дефолт — не superadmin");
    assert_eq!(body["display_name"], "Created Admin");
    assert_eq!(body["email"], email);
    assert_eq!(body["status"], "active");
    assert!(body["created_at"].is_string());
    // Пароль и хеш наружу не уходят.
    assert!(body.get("password").is_none());
    assert!(body.get("password_hash").is_none());

    // Enrollment: otpauth-URL и base32-секрет.
    let otpauth = body["otpauth_url"].as_str().expect("otpauth_url");
    assert!(
        otpauth.starts_with("otpauth://totp/"),
        "otpauth_url: {otpauth}"
    );
    assert!(otpauth.contains("issuer=DROP%20IT"), "issuer: {otpauth}");
    assert!(otpauth.contains("digits=6"));
    let secret = body["totp_secret"]
        .as_str()
        .expect("totp_secret")
        .to_string();
    assert!(!secret.is_empty());
    assert!(
        totp_rs::Secret::Encoded(secret.clone()).to_bytes().is_ok(),
        "totp_secret не base32: {secret}"
    );
    assert!(otpauth.contains(&format!("secret={secret}")));

    // БД: Argon2id-хеш проверяется верификатором логина, TOTP-секрет
    // расшифровывается тем же путём, что и на входе.
    let (password_hash, totp_enc, role, admin_display_name): (
        String,
        Vec<u8>,
        String,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT password_hash, totp_secret_enc, role, display_name FROM admin_user WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        crate::auth::verify_password(password, &password_hash).unwrap(),
        "пароль должен проверяться Argon2id-верификатором"
    );
    assert_eq!(role, "operator");
    assert_eq!(admin_display_name.as_deref(), Some("Created Admin"));
    let decrypted = dropit_common::decrypt_totp_secret(&totp_enc).unwrap();
    assert_eq!(decrypted, secret, "TOTP-секрет должен расшифровываться");

    // Аудит создания — одна запись с before=null/after.
    let (audit_action, diff): (String, Value) = sqlx::query_as(
        "SELECT action, diff FROM audit_log WHERE entity = $1 ORDER BY at DESC LIMIT 1",
    )
    .bind(id.to_string())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_action, "admin.user.create");
    assert!(diff["before"].is_null());
    assert_eq!(diff["after"]["admin_role"], "operator");
    // Секрет в аудит не попадает.
    assert!(!diff.to_string().contains(&secret));

    // Явная подроль readonly.
    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/users",
        Some(json!({
            "role": "admin",
            "email": "readonly-created@dropit.test",
            "display_name": "Read Only",
            "password": "readonly-pass-42",
            "admin_role": "readonly",
        })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create readonly: {body}");
    assert_eq!(body["admin_role"], "readonly");

    // --- role != admin → 422 role_not_creatable с подсказкой ---
    for role in ["dj", "club_staff", "guest", "owner"] {
        let (status, body) = call(
            &env,
            "POST",
            "/v1/admin/users",
            Some(json!({
                "role": role,
                "email": format!("{role}-nope@dropit.test"),
                "display_name": "Nope",
                "password": "some-password-1",
            })),
            Some(&actor_token),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{role}: {body}");
        assert_eq!(body["code"], "role_not_creatable", "{role}: {body}");
        assert_eq!(body["details"]["creatable_roles"], json!(["admin"]));
    }

    // --- валидация тела ---
    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/users",
        Some(json!({
            "role": "admin", "display_name": "No Email", "password": "some-password-1",
        })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "no email: {body}");
    assert_eq!(body["code"], "validation_error");

    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/users",
        Some(json!({
            "role": "admin", "email": "short@dropit.test",
            "display_name": "Short", "password": "1234567",
        })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "short pass: {body}"
    );

    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/users",
        Some(json!({
            "role": "admin", "email": "bad-email", "display_name": "Bad",
            "password": "some-password-1",
        })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "bad email: {body}"
    );

    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/users",
        Some(json!({
            "role": "admin", "email": "role-check@dropit.test", "display_name": "Role",
            "password": "some-password-1", "admin_role": "root",
        })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "bad admin_role: {body}"
    );
    assert_eq!(body["code"], "unknown_admin_role");

    // --- дубликат e-mail → 409 email_taken ---
    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/users",
        Some(json!({
            "role": "admin", "email": email, "display_name": "Dup",
            "password": "some-password-1",
        })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "duplicate email: {body}");
    assert_eq!(body["code"], "email_taken");
}

// ============================================================
// 4. PATCH/DELETE: статусы, refresh, self-запреты, last superadmin
// ============================================================

#[tokio::test]
async fn e2e_admin_users_patch_and_delete_rules() {
    let repo = match test_repo().await {
        Some(repo) => repo,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e_admin_users пропущен");
            return;
        }
    };
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    reset_guest_accounts(&pool).await;

    // Актор — operator (может писать), единственный superadmin — цель.
    let (actor_id, actor_token) =
        admin_fixture(&env, &pool, "operator", "patch-actor@dropit.test").await;
    let (superadmin_id, _) =
        admin_fixture(&env, &pool, "superadmin", "sole-superadmin@dropit.test").await;

    let club_id = create_club(&pool, "Patch Club", "patch-club@club.test", "active").await;
    let other_club_id = create_club(&pool, "Other Club", "other-club@club.test", "active").await;
    let dj_id = create_dj(&pool, "Patch DJ", "patch-dj@test", "active").await;
    add_refresh(&pool, "dj", dj_id).await;
    let guest_id = create_guest(&pool, "patch-guest@test", true).await;
    let device_id = add_guest_device(&pool, guest_id, "Patch Guest").await;

    // --- PATCH dj: статус suspended отзывает refresh ---
    let (status, body) = call(
        &env,
        "PATCH",
        &format!("/v1/admin/users/{dj_id}"),
        Some(json!({ "status": "suspended" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "patch dj status: {body}");
    assert_eq!(body["status"], "suspended");
    assert_eq!(body["role"], "dj");
    let (db_status,): (String,) = sqlx::query_as("SELECT status FROM dj WHERE id = $1")
        .bind(dj_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(db_status, "suspended");
    assert_eq!(
        live_refresh_count(&pool, "dj", dj_id).await,
        0,
        "refresh DJ должен быть отозван"
    );

    // --- PATCH dj: имя ---
    let (status, body) = call(
        &env,
        "PATCH",
        &format!("/v1/admin/users/{dj_id}"),
        Some(json!({ "display_name": "Renamed DJ" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "patch dj name: {body}");
    assert_eq!(body["display_name"], "Renamed DJ");

    // --- PATCH club: имя и e-mail ---
    let (status, body) = call(
        &env,
        "PATCH",
        &format!("/v1/admin/users/{club_id}"),
        Some(json!({ "display_name": "Renamed Club", "email": "renamed-club@club.test" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "patch club: {body}");
    assert_eq!(body["display_name"], "Renamed Club");
    let (db_name, db_email): (String, String) =
        sqlx::query_as("SELECT name, email::text FROM club WHERE id = $1")
            .bind(club_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(db_name, "Renamed Club");
    assert_eq!(db_email, "renamed-club@club.test");

    // --- email-конфликт → 409 ---
    let (status, body) = call(
        &env,
        "PATCH",
        &format!("/v1/admin/users/{club_id}"),
        Some(json!({ "email": "other-club@club.test" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "email conflict: {body}");
    assert_eq!(body["code"], "email_taken");
    let (other_email,): (String,) = sqlx::query_as("SELECT email::text FROM club WHERE id = $1")
        .bind(other_club_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(other_email, "other-club@club.test");

    // --- self-запреты ---
    let (status, body) = call(
        &env,
        "PATCH",
        &format!("/v1/admin/users/{actor_id}"),
        Some(json!({ "role": "readonly" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "self role: {body}"
    );
    assert_eq!(body["code"], "cannot_change_own_role");

    let (status, body) = call(
        &env,
        "PATCH",
        &format!("/v1/admin/users/{actor_id}"),
        Some(json!({ "status": "blocked" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "self block: {body}"
    );
    assert_eq!(body["code"], "cannot_block_self");

    // --- последний superadmin: разжаловать/заблокировать нельзя ---
    let (status, body) = call(
        &env,
        "PATCH",
        &format!("/v1/admin/users/{superadmin_id}"),
        Some(json!({ "role": "operator" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "last superadmin demote: {body}"
    );
    assert_eq!(body["code"], "last_superadmin");

    let (status, body) = call(
        &env,
        "PATCH",
        &format!("/v1/admin/users/{superadmin_id}"),
        Some(json!({ "status": "blocked" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "last superadmin block: {body}"
    );
    assert_eq!(body["code"], "last_superadmin");

    // --- DELETE: себя нельзя, последнего superadmin нельзя ---
    let (status, body) = call(
        &env,
        "DELETE",
        &format!("/v1/admin/users/{actor_id}"),
        None,
        Some(&actor_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "self delete: {body}"
    );
    assert_eq!(body["code"], "cannot_delete_self");

    let (status, body) = call(
        &env,
        "DELETE",
        &format!("/v1/admin/users/{superadmin_id}"),
        None,
        Some(&actor_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "last superadmin delete: {body}"
    );
    assert_eq!(body["code"], "last_superadmin");

    // --- гость: blocked банит устройства, active снимает бан ---
    let (status, body) = call(
        &env,
        "PATCH",
        &format!("/v1/admin/users/{guest_id}"),
        Some(json!({ "status": "blocked", "display_name": "Blocked Guest" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "guest block: {body}");
    assert_eq!(body["status"], "blocked");
    assert_eq!(body["display_name"], "Blocked Guest");
    let (banned,): (bool,) = sqlx::query_as("SELECT banned FROM guest_device WHERE id = $1")
        .bind(device_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(banned, "устройство гостя должно быть забанено");

    let (status, body) = call(
        &env,
        "PATCH",
        &format!("/v1/admin/users/{guest_id}"),
        Some(json!({ "status": "active" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "guest unban: {body}");
    assert_eq!(body["status"], "active");

    // --- запрещённые значения/поля ---
    let (status, body) = call(
        &env,
        "PATCH",
        &format!("/v1/admin/users/{dj_id}"),
        Some(json!({ "status": "invited" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "status invited: {body}"
    );
    assert_eq!(body["code"], "status_not_supported");

    let (status, body) = call(
        &env,
        "PATCH",
        &format!("/v1/admin/users/{dj_id}"),
        Some(json!({ "role": "club_staff" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "dj→club: {body}");
    assert_eq!(body["code"], "role_immutable");

    let (status, body) = call(
        &env,
        "PATCH",
        &format!("/v1/admin/users/{dj_id}"),
        Some(json!({})),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "empty patch: {body}");

    let (status, body) = call(
        &env,
        "PATCH",
        &format!("/v1/admin/users/{}", Uuid::now_v7()),
        Some(json!({ "display_name": "Ghost" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unknown patch: {body}");

    // --- DELETE dj: soft-block + revoke ---
    add_refresh(&pool, "dj", dj_id).await;
    let (status, _body) = call(
        &env,
        "DELETE",
        &format!("/v1/admin/users/{dj_id}"),
        None,
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (db_status,): (String,) = sqlx::query_as("SELECT status FROM dj WHERE id = $1")
        .bind(dj_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(db_status, "blocked");
    assert_eq!(live_refresh_count(&pool, "dj", dj_id).await, 0);

    // --- аудит: записи update/delete с before/after ---
    assert!(audit_count(&pool, "admin.user.update").await >= 3);
    assert!(audit_count(&pool, "admin.user.delete").await >= 1);
    let (diff,): (Value,) = sqlx::query_as(
        "SELECT diff FROM audit_log WHERE action = 'admin.user.delete' ORDER BY at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(diff["before"].is_object(), "before-image в аудите: {diff}");
    assert_eq!(diff["after"]["status"], "blocked");
}

// ============================================================
// 5. readonly-админ: GET 200, мутации 403
// ============================================================

#[tokio::test]
async fn e2e_admin_users_readonly_admin_cannot_write() {
    let repo = match test_repo().await {
        Some(repo) => repo,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e_admin_users пропущен");
            return;
        }
    };
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    reset_guest_accounts(&pool).await;
    let (readonly_id, readonly_token) =
        admin_fixture(&env, &pool, "readonly", "readonly@dropit.test").await;
    let club_id = create_club(&pool, "RO Club", "ro-club@test", "active").await;

    // GET — разрешён.
    for uri in [
        "/v1/admin/users".to_string(),
        "/v1/admin/roles".to_string(),
        format!("/v1/admin/users/{club_id}"),
        format!("/v1/admin/users/{readonly_id}"),
    ] {
        let (status, body) = call(&env, "GET", &uri, None, Some(&readonly_token)).await;
        assert_eq!(status, StatusCode::OK, "readonly GET {uri}: {body}");
    }

    // Мутации — 403 (гейт require_admin_write).
    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/users",
        Some(json!({
            "role": "admin", "email": "readonly-created@dropit.test",
            "display_name": "Nope", "password": "some-password-1",
        })),
        Some(&readonly_token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "readonly POST: {body}");
    assert_eq!(body["code"], "forbidden");

    let (status, body) = call(
        &env,
        "PATCH",
        &format!("/v1/admin/users/{readonly_id}"),
        Some(json!({ "display_name": "Hacked" })),
        Some(&readonly_token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "readonly PATCH: {body}");
    assert_eq!(body["code"], "forbidden");

    let (status, body) = call(
        &env,
        "DELETE",
        &format!("/v1/admin/users/{club_id}"),
        None,
        Some(&readonly_token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "readonly DELETE: {body}");

    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/users/bulk",
        Some(json!({ "ids": [club_id], "action": "block" })),
        Some(&readonly_token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "readonly bulk: {body}");

    // Ни одна мутация не применилась.
    let (db_name,): (String,) = sqlx::query_as("SELECT name FROM club WHERE id = $1")
        .bind(club_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(db_name, "RO Club");
    let (db_status,): (String,) = sqlx::query_as("SELECT status FROM club WHERE id = $1")
        .bind(club_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(db_status, "active");

    // JWT с ролью Admin, но без активной строки admin_user: читать может
    // (контракт: чтение под ролью JWT), писать — нет.
    let ghost_token = env
        .keys
        .issue_access(Uuid::now_v7(), Role::Admin, Uuid::now_v7())
        .unwrap();
    let (status, _) = call(&env, "GET", "/v1/admin/users", None, Some(&ghost_token)).await;
    assert_eq!(status, StatusCode::OK, "GET без строки админа");
    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/users",
        Some(json!({
            "role": "admin", "email": "ghost-created@dropit.test",
            "display_name": "Ghost", "password": "some-password-1",
        })),
        Some(&ghost_token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "admin без строки: {body}");
}

// ============================================================
// 6. bulk: одна транзакция, один аудит
// ============================================================

#[tokio::test]
async fn e2e_admin_users_bulk_one_transaction_one_audit() {
    let repo = match test_repo().await {
        Some(repo) => repo,
        None => {
            eprintln!("DATABASE_URL недоступен — e2e_admin_users пропущен");
            return;
        }
    };
    let env = app_env(Some(repo.clone()), None, None).await;
    let pool = repo.pool();
    reset_guest_accounts(&pool).await;
    let (actor_id, actor_token) =
        admin_fixture(&env, &pool, "superadmin", "bulk-actor@dropit.test").await;
    let (second_superadmin_id, _) =
        admin_fixture(&env, &pool, "superadmin", "bulk-superadmin@dropit.test").await;
    let (plain_admin_id, _) =
        admin_fixture(&env, &pool, "readonly", "bulk-readonly@dropit.test").await;

    let club1 = create_club(&pool, "Bulk Club 1", "bulk1@test", "active").await;
    let club2 = create_club(&pool, "Bulk Club 2", "bulk2@test", "active").await;
    let dj_id = create_dj(&pool, "Bulk DJ", "bulk-dj@test", "active").await;
    add_refresh(&pool, "dj", dj_id).await;
    let guest_id = create_guest(&pool, "bulk-guest@test", true).await;
    add_guest_device(&pool, guest_id, "Bulk Guest").await;

    let ids = vec![club1, club2, dj_id, guest_id];
    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/users/bulk",
        Some(json!({ "ids": ids, "action": "block" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "bulk block: {body}");
    assert_eq!(body["affected"], 4);
    assert_eq!(body["ids"].as_array().unwrap().len(), 4);
    assert_eq!(
        audit_count(&pool, "admin.user.bulk").await,
        1,
        "один аудит на bulk"
    );

    // before-снимок и after в единственной записи аудита.
    let (diff,): (Value,) = sqlx::query_as(
        "SELECT diff FROM audit_log WHERE action = 'admin.user.bulk' ORDER BY at DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(diff["after"]["action"], "block");
    assert_eq!(diff["before"]["clubs"].as_array().unwrap().len(), 2);
    assert_eq!(diff["before"]["djs"][0]["status"], "active");

    // Применилось ко всем четырём таблицам + refresh отозван.
    let (club_status,): (String,) = sqlx::query_as("SELECT status FROM club WHERE id = $1")
        .bind(club1)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(club_status, "blocked");
    let (dj_status,): (String,) = sqlx::query_as("SELECT status FROM dj WHERE id = $1")
        .bind(dj_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(dj_status, "blocked");
    assert_eq!(live_refresh_count(&pool, "dj", dj_id).await, 0);
    let (banned,): (bool,) =
        sqlx::query_as("SELECT bool_or(banned) FROM guest_device WHERE account_id = $1")
            .bind(guest_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(banned);

    // --- approve возвращает статусы и снимает бан ---
    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/users/bulk",
        Some(json!({ "ids": ids, "action": "approve" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "bulk approve: {body}");
    assert_eq!(body["affected"], 4);
    for id in [club1, club2] {
        let (s,): (String,) = sqlx::query_as("SELECT status FROM club WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(s, "active");
    }
    let (dj_status,): (String,) = sqlx::query_as("SELECT status FROM dj WHERE id = $1")
        .bind(dj_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(dj_status, "active");

    // --- role: только админы, affected = числу совпавших ---
    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/users/bulk",
        Some(json!({
            "ids": [plain_admin_id, second_superadmin_id],
            "action": "role",
            "role": "readonly",
        })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "bulk role: {body}");
    assert_eq!(body["affected"], 2);
    let (role,): (String,) = sqlx::query_as("SELECT role FROM admin_user WHERE id = $1")
        .bind(plain_admin_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(role, "readonly");

    // --- role: последний superadmin → 409, транзакция не применяется ---
    // second_superadmin уже разжалован предыдущим вызовом, поэтому активный
    // superadmin ровно один — actor; его разжалование оставило бы платформу
    // без владельца.
    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/users/bulk",
        Some(json!({
            "ids": [actor_id, second_superadmin_id],
            "action": "role",
            "role": "operator",
        })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "bulk last superadmin: {body}");
    assert_eq!(body["code"], "last_superadmin");
    let (role,): (String,) = sqlx::query_as("SELECT role FROM admin_user WHERE id = $1")
        .bind(actor_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(role, "superadmin", "отказ не должен менять роль");

    // --- себя в bulk-блокировку нельзя ---
    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/users/bulk",
        Some(json!({ "ids": [actor_id], "action": "block" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "bulk self block: {body}"
    );
    assert_eq!(body["code"], "cannot_block_self");

    // --- мусорные тела и неизвестные id ---
    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/users/bulk",
        Some(json!({ "ids": [club1], "action": "explode" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], "unsupported_action");

    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/users/bulk",
        Some(json!({ "ids": [], "action": "block" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    let (status, body) = call(
        &env,
        "POST",
        "/v1/admin/users/bulk",
        Some(json!({ "ids": [Uuid::now_v7(), Uuid::now_v7()], "action": "block" })),
        Some(&actor_token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "unknown ids: {body}");
    assert_eq!(body["affected"], 0);
    assert!(body["ids"].as_array().unwrap().is_empty());
}
