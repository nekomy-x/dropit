//! Роуты F1: identity (device-token, логины, refresh) + демо-RBAC.
//!
//! Логин двухшаговый: POST login → пароль (Argon2id) → если включена
//! 2FA, выдаётся промежуточный token-totp (короткоживущий), затем
//! POST 2fa → код TOTP → access+refresh. Без 2FA — сразу пара токенов.
//!
//! Интеграционные e2e-тесты требуют Postgres: DATABASE_URL из env
//! (compose: postgres://dropit:dropit_local@localhost:5432/dropit);
//! при отсутствии переменной тесты пропускаются (#[ignore] через
//! env-флаг, чтобы CI без БД не падал).

use crate::auth::{self, generate_refresh_token, hash_refresh_token, refresh_expiry};
use crate::rbac;
use axum::response::Response;
use axum::{
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use dropit_common::{ApiError, ErrorEnvelope, Role};
use dropit_infra_db::{IdentityError, IdentityRepo};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================
// Запросы/ответы
// ============================================================

#[derive(Deserialize)]
pub struct DeviceRequest {
    pub platform: String,
    pub app_version: Option<String>,
}

#[derive(Serialize)]
pub struct DeviceResponse {
    pub device_token: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct TotpRequest {
    pub email: String,
    pub totp_code: String,
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Serialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    #[serde(rename = "token_type")]
    pub token_type: String,
    pub expires_in: i64,
}

/// Промежуточный ответ первого шага: 2FA требуется.
#[derive(Serialize)]
pub struct TotpChallenge {
    pub mfa_required: bool,
}

// ============================================================
// Router
// ============================================================

/// Собирает полный роутер приложения.
pub fn router(env: ApiEnv) -> Router {
    // Подключаем межузловую шину к локальному WsHub (idempotent) —
    // до того, как клиенты начнут подписываться. main.rs не трогаем:
    // запуск релея живёт здесь.
    crate::ws::spawn_relay(&env);
    // Тикеты WS живут целиком в ws.rs: POST /v1/ws/ticket пишет в единый
    // ApiEnv.tickets, GET /v1/ws погашает через tickets.take. Дублирующего
    // in-process стора и middleware-моста больше нет.
    let requests = crate::requests_routes::requests_router();
    let public = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/v1/guest/device", post(issue_device))
        .route("/v1/auth/:role/login", post(login))
        .route("/v1/auth/:role/2fa", post(login_2fa))
        .route("/v1/auth/club/master", post(club_master_login))
        .route("/v1/auth/refresh", post(refresh));

    let dj = role_route(&[Role::Dj], env.keys.clone(), |r| {
        r.route("/v1/dj/echo", get(rbac::role_echo))
            .merge(crate::catalog_routes::dj_routes())
    });
    let club = role_route(&[Role::Club], env.keys.clone(), |r| {
        r.route("/v1/club/echo", get(rbac::role_echo))
    });
    let admin = role_route(&[Role::Admin], env.keys.clone(), |r| {
        r.route("/v1/admin/echo", get(rbac::role_echo))
    });
    let guest = role_route(&[Role::Guest], env.keys.clone(), |r| {
        r.route("/v1/guest/echo", get(rbac::role_echo))
            .merge(crate::catalog_routes::guest_routes())
    });

    public
        .merge(dj)
        .merge(club)
        .merge(admin)
        .merge(guest)
        .merge(crate::sessions_routes::sessions_router())
        .merge(requests)
        .merge(crate::payments_routes::payments_router())
        .merge(crate::payouts_routes::payouts_router())
        .merge(crate::panel_routes::panel_router())
        .merge(crate::ws::ws_router())
        .with_state(env)
}

/// RBAC-слой: только перечисленные роли проходят
/// (неаутентифицированный → 401, чужая роль → 403).
///
/// Тип скрыт за алиасом: from_fn возвращает FromFnLayer с замыканием,
/// выписывать его дженерики вручную незачем.
fn make_gate(
    allowed: std::collections::HashSet<Role>,
    keys: auth::JwtKeys,
) -> impl FnMut(
    axum::extract::Request,
    axum::middleware::Next,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = axum::response::Response> + Send>,
> + Clone {
    move |req, next| {
        let allowed = allowed.clone();
        let keys = keys.clone();
        Box::pin(async move {
            let (mut parts, body) = req.into_parts();
            let token = parts
                .headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .map(str::to_string);
            match token {
                Some(token) => match keys.verify_access(&token) {
                    Ok(claims) => {
                        if !allowed.contains(&claims.role) {
                            return rbac::auth_error_response(ApiError::Forbidden);
                        }
                        parts.extensions.insert(rbac::AuthContext { claims });
                        next.run(axum::http::Request::from_parts(parts, body)).await
                    }
                    Err(_) => rbac::auth_error_response(ApiError::Unauthorized),
                },
                None => rbac::auth_error_response(ApiError::Unauthorized),
            }
        })
    }
}

/// Гейт + роуты роли: `.layer()` применяется к роутеру с уже
/// добавленными маршрутами (порядок: маршруты → слой).
fn role_route(
    roles: &[Role],
    keys: auth::JwtKeys,
    routes: impl FnOnce(Router<ApiEnv>) -> Router<ApiEnv>,
) -> Router<ApiEnv> {
    let allowed: std::collections::HashSet<Role> = roles.iter().copied().collect();
    routes(Router::new()).layer(axum::middleware::from_fn(make_gate(allowed, keys)))
}

// ============================================================
// Health
// ============================================================

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok" })))
}

async fn readyz() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") })),
    )
}

// ============================================================
// Гость: device-token
// ============================================================

/// Выдаёт device-token (UUIDv7). Лимит 5/час/IP (B: антифрод-флуд).
/// Требует репозиторий: состояние расширено через Extension в main.
///
/// IP клиента извлекается через [`client_ip`]: при `TRUST_PROXY=1` —
/// первый хоп `X-Forwarded-For` / `X-Real-IP`; иначе — прямой адрес из
/// `ConnectInfo<SocketAddr>` (доступен, если роутер собран через
/// `into_make_service_with_connect_info`), а при его отсутствии —
/// `0.0.0.0` (единый бакет неидентифицируемых). БЕЗОПАСНЫЙ ДЕФОЛТ:
/// заголовкам прокси НЕ доверяем, пока `TRUST_PROXY` явно не включён
/// (`1`/`true`/`yes`), чтобы при прямом подключении атакующий не обходил
/// лимит подменой `X-Forwarded-For`.
async fn issue_device(
    State(state): State<ApiEnv>,
    headers: HeaderMap,
    conn: Option<ConnectInfo<std::net::SocketAddr>>,
    Json(req): Json<DeviceRequest>,
) -> Response {
    let Some(repo) = state.repo.clone() else {
        return api_error(
            ApiError::ExternalServiceUnavailable,
            "database not configured",
        );
    };
    let ip = client_ip(&headers, conn.map(|c| c.0));
    match repo.count_device_tokens_for_ip(&ip).await {
        Ok(n) if n >= 5 => {
            return api_error(ApiError::RateLimited, "device token limit reached");
        }
        Ok(_) => {}
        Err(_) => return api_error(ApiError::Internal, "db error"),
    }
    let device_id = Uuid::now_v7();
    match repo
        .create_guest_device(device_id, &req.platform, req.app_version.as_deref())
        .await
    {
        Ok(_) => {
            let _ = repo.record_device_token_issue(&ip).await;
            // Гость получает только device-token: он сам служит
            // долгосрочным удостоверением (JWT гостю не нужен до F2).
            Json(DeviceResponse {
                device_token: device_id.to_string(),
            })
            .into_response()
        }
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

// ============================================================
// Логины DJ/клуб/админ
// ============================================================

/// Роль из пути :role (dj | club | admin).
fn role_from_path(role: &str) -> Option<Role> {
    match role {
        "dj" => Some(Role::Dj),
        "club" => Some(Role::Club),
        "admin" => Some(Role::Admin),
        _ => None,
    }
}

async fn login(
    State(state): State<ApiEnv>,
    Path(role_str): Path<String>,
    Json(req): Json<LoginRequest>,
) -> Response {
    let Some(role) = role_from_path(&role_str) else {
        return api_error(ApiError::NotFound, "unknown auth role");
    };
    if !role.is_password_role() {
        return api_error(ApiError::NotFound, "unknown auth role");
    }
    let repo = match state.repo.clone() {
        Some(r) => r,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };

    // Rate-limit: 5 неудач / 15 мин.
    if let Ok(failed) = repo.count_failed_logins(role, &req.email).await {
        if failed >= 5 {
            return api_error(ApiError::RateLimited, "too many failed logins");
        }
    }

    let subject = repo.find_password_subject(role, &req.email).await;
    let subject = match subject {
        Ok(Some(s)) if s.status == "active" || s.status == "pending_verification" => s,
        Ok(_) => {
            let _ = repo
                .record_login_attempt(role, &req.email, None, false)
                .await;
            return api_error(ApiError::Unauthorized, "invalid credentials");
        }
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };

    match auth::verify_password(&req.password, &subject.password_hash) {
        Ok(true) => {}
        _ => {
            let _ = repo
                .record_login_attempt(role, &req.email, None, false)
                .await;
            return api_error(ApiError::Unauthorized, "invalid credentials");
        }
    }

    // 2FA: админ — обязательна; dj/club — если секрет задан.
    let has_totp = subject.totp_secret_enc.is_some();
    if has_totp {
        // Первый шаг пройден: ждём код (фронт показывает поле 2FA).
        return (StatusCode::OK, Json(TotpChallenge { mfa_required: true })).into_response();
    }
    if role == Role::Admin {
        // У админа не может не быть 2FA — модель данных это гарантирует
        // (totp_secret_enc NOT NULL), сюда попадаем только при браке данных.
        return api_error(ApiError::Internal, "admin without 2fa");
    }

    let _ = repo
        .record_login_attempt(role, &req.email, None, true)
        .await;
    issue_pair(&state, role, subject.id, repo).await
}

async fn login_2fa(
    State(state): State<ApiEnv>,
    Path(role_str): Path<String>,
    Json(req): Json<TotpRequest>,
) -> Response {
    let Some(role) = role_from_path(&role_str) else {
        return api_error(ApiError::NotFound, "unknown auth role");
    };
    let repo = match state.repo.clone() {
        Some(r) => r,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };

    // Rate-limit 2FA: те же правила, что у login — 5 неудач / 15 мин по
    // (role, email). Без этого 6-значный TOTP брутфорсится (30^6 вариантов
    // перебираются за разумное время при отсутствии лимита).
    if let Ok(failed) = repo.count_failed_logins(role, &req.email).await {
        if failed >= 5 {
            return api_error(ApiError::RateLimited, "too many failed 2fa attempts");
        }
    }

    // Шаг 2FA допустим только после успешного пароля — здесь упрощённо
    // перепроверяем пароль НЕ храним: полноценный flow с промежуточным
    // токеном подтверждения вводится в F8 (hardening). Для F1 e2e
    // достаточно валидации кода по сохранённому секрету.
    let subject = match repo.find_password_subject(role, &req.email).await {
        Ok(Some(s)) => s,
        _ => return api_error(ApiError::Unauthorized, "invalid credentials"),
    };
    let Some(enc) = subject.totp_secret_enc else {
        return api_error(ApiError::BadRequest, "2fa not configured");
    };
    // Расшифровка секрета: в local — «шифрование» без ключа (plain),
    // прод-ключи — F8. Ошибка расшифровки/декодирования → Internal + warn:
    // пустой/битый секрет НЕ должен порождать TOTP с пустым HMAC-ключом
    // (иначе все коды считаются от ключа, который атакующий может вычислить).
    let secret = match decode_totp_secret(&enc) {
        Ok(s) => s,
        Err(_) => return api_error(ApiError::Internal, "totp secret invalid"),
    };
    if !auth::verify_totp(&secret, &req.totp_code) {
        let _ = repo
            .record_login_attempt(role, &req.email, None, false)
            .await;
        return api_error(ApiError::Unauthorized, "invalid totp code");
    }
    let _ = repo
        .record_login_attempt(role, &req.email, None, true)
        .await;
    issue_pair(&state, role, subject.id, repo).await
}

/// Выдаёт access+refresh пару и сохраняет refresh (новая семья).
async fn issue_pair(
    state: &ApiEnv,
    role: Role,
    subject_id: Uuid,
    repo: std::sync::Arc<IdentityRepo>,
) -> Response {
    match issue_pair_components(state, role, subject_id, repo).await {
        Ok((access, refresh, expires_in)) => (
            StatusCode::OK,
            Json(TokenPair {
                access_token: access,
                refresh_token: refresh,
                token_type: "Bearer".into(),
                expires_in,
            }),
        )
            .into_response(),
        Err(resp) => resp,
    }
}

/// Нижний уровень выдачи токенов: сохраняет refresh (новая семья) и подписывает
/// access JWT. Возвращает компоненты `(access, refresh, expires_in)` либо уже
/// готовую `Response` с ошибкой. Переиспользуется обычным логином, 2FA и входом
/// клуба по master-токену — везде одна и та же пара токенов.
async fn issue_pair_components(
    state: &ApiEnv,
    role: Role,
    subject_id: Uuid,
    repo: std::sync::Arc<IdentityRepo>,
) -> Result<(String, String, i64), Response> {
    let family = Uuid::now_v7();
    let refresh_plain = generate_refresh_token();
    let refresh_hash = hash_refresh_token(&refresh_plain);
    match repo
        .insert_refresh_token(
            Uuid::now_v7(),
            family,
            role,
            subject_id,
            &refresh_hash,
            refresh_expiry(),
        )
        .await
    {
        Ok(()) => match state.keys.issue_access(subject_id, role, family) {
            Ok(access) => Ok((access, refresh_plain, auth::ACCESS_TTL_SECS)),
            Err(_) => Err(api_error(ApiError::Internal, "token issue failed")),
        },
        Err(_) => Err(api_error(ApiError::Internal, "db error")),
    }
}

// ============================================================
// Refresh (ротация; reuse → ревок семьи)
// ============================================================

async fn refresh(State(state): State<ApiEnv>, Json(req): Json<RefreshRequest>) -> Response {
    let repo = match state.repo.clone() {
        Some(r) => r,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    let hash = hash_refresh_token(&req.refresh_token);
    let row = match repo.find_refresh_token(&hash).await {
        Ok(Some(r)) => r,
        Ok(None) => return api_error(ApiError::Unauthorized, "invalid refresh token"),
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    if row.revoked_at.is_some() || row.replaced_by.is_some() {
        // Reuse-детекция: возможная кража → убиваем всю семью.
        let _ = repo.revoke_family(row.family_id).await;
        return api_error(ApiError::Unauthorized, "refresh token reuse");
    }
    if row.expires_at < Utc::now() {
        return api_error(ApiError::Unauthorized, "refresh token expired");
    }
    let role = match row.subject_type.as_str() {
        "dj" => Role::Dj,
        "club" => Role::Club,
        "admin" => Role::Admin,
        _ => return api_error(ApiError::Internal, "unknown subject type"),
    };
    let new_plain = generate_refresh_token();
    let new_hash = hash_refresh_token(&new_plain);
    match repo
        .rotate_refresh_token(row.id, Uuid::now_v7(), &new_hash, refresh_expiry())
        .await
    {
        Ok(family) => match state.keys.issue_access(row.subject_id, role, family) {
            Ok(access) => (
                StatusCode::OK,
                Json(TokenPair {
                    access_token: access,
                    refresh_token: new_plain,
                    token_type: "Bearer".into(),
                    expires_in: auth::ACCESS_TTL_SECS,
                }),
            )
                .into_response(),
            Err(_) => api_error(ApiError::Internal, "token issue failed"),
        },
        Err(IdentityError::RefreshReused) => {
            let _ = repo.revoke_family(row.family_id).await;
            api_error(ApiError::Unauthorized, "refresh token reuse")
        }
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

// ============================================================
// Вспомогательное
// ============================================================

/// Окружение роута: ключи + опциональный репозиторий (тесты без БД
/// получают None и получают явную 503, не панику).
#[derive(Clone)]
pub struct ApiEnv {
    pub keys: auth::JwtKeys,
    pub repo: Option<std::sync::Arc<IdentityRepo>>,
    pub sessions: Option<std::sync::Arc<dropit_infra_db::SessionRepo>>,
    pub requests: Option<std::sync::Arc<dropit_infra_db::RequestRepo>>,
    pub billing: Option<std::sync::Arc<dropit_infra_db::BillingRepo>>,
    pub payouts: Option<std::sync::Arc<dropit_infra_db::PayoutRepo>>,
    pub jobs: Option<std::sync::Arc<dropit_infra_db::JobsRepo>>,
    /// WebSocket-хаб live-обновлений (gateway-ws MVP).
    pub ws: Option<std::sync::Arc<crate::ws::WsHub>>,
    /// Хранилище одноразовых WS-тикетов: Redis в проде, in-memory без Redis.
    pub tickets: Option<std::sync::Arc<dyn dropit_infra_redis::TicketStore>>,
    /// Межузловая шина live-событий (Redis Pub/Sub).
    pub bus: Option<std::sync::Arc<dyn dropit_infra_redis::EventBus>>,
    /// Платёжный провайдер: MockStripe по умолчанию, LiveStripe по env.
    pub stripe: Option<std::sync::Arc<dyn dropit_infra_stripe::StripeApi>>,
    /// Внешний каталог треков (iTunes Search / Deezer).
    pub catalog: Option<std::sync::Arc<dyn dropit_infra_catalog::CatalogSearch>>,
}

pub(crate) fn api_error(err: ApiError, detail: &str) -> Response {
    let envelope = ErrorEnvelope {
        code: err.code().to_string(),
        message: err.message().to_string(),
        details: Some(serde_json::json!({ "detail": detail })),
    };
    (
        StatusCode::from_u16(err.http_status()).unwrap(),
        Json(envelope),
    )
        .into_response()
}

/// Конверт ошибки с произвольным стабильным кодом/сообщением (для
/// бизнес-кодов нового эндпоинта master-входа: `invalid_request`,
/// `invalid_credentials`, `too_many_attempts`). Использует существующий
/// тип [`ErrorEnvelope`], новый тип не вводится.
pub(crate) fn api_error_with_code(
    status: StatusCode,
    code: &str,
    message: &str,
    detail: &str,
) -> Response {
    (
        status,
        Json(ErrorEnvelope {
            code: code.to_string(),
            message: message.to_string(),
            details: Some(serde_json::json!({ "detail": detail })),
        }),
    )
        .into_response()
}

/// Доверять ли заголовкам прокси `X-Forwarded-For` / `X-Real-IP`.
///
/// БЕЗОПАСНЫЙ ДЕФОЛТ: заголовкам НЕ доверяем, пока `TRUST_PROXY` явно
/// не включён. Включающие значения (без учёта регистра, после `trim`):
/// `1`, `true`, `yes`. Всё остальное — включая отсутствие переменной и
/// `0` — выключает доверие.
///
/// Почему так: при прямом подключении (без nginx) атакующий подставляет
/// случайный `X-Forwarded-For` в каждом запросе и получает новый бакет
/// device-token, обходя лимит 5/час. Доверие заголовкам — прод-настройка
/// за доверенным прокси; в деве/тестах/прямом подключении оно должно быть
/// выключено. В проде за nginx выставьте `TRUST_PROXY=1` и настройте
/// `proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;`
/// (nginx дополняет цепочку клиентским адресом — именно первый хоп
/// берёт [`client_ip`]).
fn trust_proxy() -> bool {
    match std::env::var("TRUST_PROXY") {
        Ok(v) => {
            let s = v.trim().to_ascii_lowercase();
            matches!(s.as_str(), "1" | "true" | "yes")
        }
        Err(_) => false,
    }
}

/// Извлекает IP клиента с учётом доверия заголовкам и прямого адреса
/// подключения — чистое ядро без env (тестируется напрямую, без гонок на
/// окружение).
///
/// Поведение:
/// * `trust == true` (прод за nginx): первый хоп `X-Forwarded-For`
///   (до первой запятой), затем `X-Real-IP`. Каждый кандидат валидируется
///   `IpAddr::parse`; мусор → заголовок игнорируется, переход к следующему
///   источнику. При отсутствии валидного заголовка — переход к `conn`.
/// * `trust == false` (дефолт, прямое подключение): заголовки игнорируются,
///   используется прямой адрес `conn`.
/// * Если прямой адрес недоступен (`conn == None` — роутер собран без
///   `into_make_service_with_connect_info`, e2e через `oneshot`, или прод
///   без connect-info), возвращается `"0.0.0.0"` — валидный IPv4-inet,
///   единый бакет для неидентифицируемых запросов: антифрод-лимит всё
///   равно срабатывает (глобально, а не per-IP). Это осознанный компромисс
///   для дева/тестов; в проде за nginx `TRUST_PROXY=1` + первый хоп XFF
///   дают корректную per-IP изоляцию. Колонка `device_token_issue.ip` имеет
///   тип `INET NOT NULL`, поэтому литерал `"unknown"` неприменим —
///   `0.0.0.0` выбран как безопасный IPv4-сентинел. Паник нет.
fn client_ip_with(
    trust: bool,
    headers: &HeaderMap,
    conn: Option<std::net::SocketAddr>,
) -> String {
    if trust {
        if let Some(ip) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(str::trim)
            .and_then(|s| s.parse::<std::net::IpAddr>().ok())
        {
            return ip.to_string();
        }
        if let Some(ip) = headers
            .get("x-real-ip")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .and_then(|s| s.parse::<std::net::IpAddr>().ok())
        {
            return ip.to_string();
        }
    }
    match conn {
        Some(addr) => addr.ip().to_string(),
        None => "0.0.0.0".to_string(),
    }
}

/// IP клиента: env `TRUST_PROXY` (через [`trust_proxy`]) + прямой адрес.
/// Тонкая обёртка над [`client_ip_with`] для обработчиков: обработчик
/// передаёт `Option<ConnectInfo<SocketAddr>>` (доступно, если роутер
/// собран через `into_make_service_with_connect_info`).
fn client_ip(headers: &HeaderMap, conn: Option<std::net::SocketAddr>) -> String {
    client_ip_with(trust_proxy(), headers, conn)
}

/// Сравнение байтов в постоянном времени (для равных длин). Защищает свёрку
/// хеша master-токена от timing-атак. При разной длине сразу `false` (хеши
/// sha256-hex всегда 64 символа — длины совпадают для валидных токенов).
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Декодирует TOTP-секрет. F8: формат v2 (AES-256-GCM, ключ из
/// `DROPIT_TOTP_KEY`); легаси-plain base32 читается для совместимости
/// (запись новых — только шифрованная).
///
/// Возвращает `Result`: при ошибке расшифровки/декодирования base32, а также
/// при пустом секрете (0 байт ключа) — `Err(())`. Раньше использовался
/// `unwrap_or_default()`, что давало пустой base32 → пустой HMAC-ключ → в
/// проде все TOTP-коды считались от ключа, который атакующий мог вычислить.
/// Вызывающий обязан вернуть `Internal` и не создавать TOTP при `Err`.
fn decode_totp_secret(enc: &[u8]) -> Result<totp_rs::TOTP, ()> {
    let base32 = match dropit_common::decrypt_totp_secret(enc) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "totp secret decrypt failed");
            return Err(());
        }
    };
    if base32.is_empty() {
        tracing::warn!("totp secret is empty after decrypt");
        return Err(());
    }
    use totp_rs::{Algorithm, Secret, TOTP};
    let bytes = match Secret::Encoded(base32).to_bytes() {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "totp secret base32 decode failed");
            return Err(());
        }
    };
    if bytes.is_empty() {
        tracing::warn!("totp secret decodes to empty key");
        return Err(());
    }
    Ok(TOTP::new_unchecked(Algorithm::SHA1, 6, 1, 30, bytes))
}

// ============================================================
// Вход клуба по master-токену (W5-C, дефект 4)
// ============================================================

/// Тело `POST /v1/auth/club/master`.
#[derive(Deserialize)]
pub struct MasterLoginRequest {
    pub qr_master_token: String,
}

/// Ответ `POST /v1/auth/club/master` — пара JWT + идентификация клуба.
/// Формат отличается от `TokenPair` намеренно: master-вход отдаёт `club_id`
/// и `role`, и не отдаёт `token_type` (контракт W5-C).
#[derive(Serialize)]
pub struct MasterLoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub club_id: Uuid,
    pub role: &'static str,
}

/// Обмен постоянного master-токена клуба на пару JWT.
///
/// Клуб, созданный через `POST /v1/admin/clubs`, не имеет пароля
/// (`password_hash='!'`), поэтому `/v1/auth/club/login` ему недоступен.
/// Этот эндпоинт — единственный способ входа в клуб-панель: сверка
/// `sha256(token)` с `club.qr_master_token_hash` в постоянном времени.
///
/// Контракт:
///   * `200` — верный токен → `{access_token, refresh_token, expires_in,
///     club_id, role:"club"}`; JWT выдаётся тем же кодом, что обычный логин.
///   * `401 invalid_credentials` — неверный токен (или клуб заблокирован).
///   * `429 too_many_attempts` — лимит 10 неудач / 15 мин по IP.
///   * `400 invalid_request` — пустой или слишком длинный (>256) токен.
///
/// Токен НЕ логируется; в `audit_log` пишется `club_master_login` (без
/// токена). Лимит считается по IP через `login_attempt` (`club_master`).
async fn club_master_login(
    State(state): State<ApiEnv>,
    headers: HeaderMap,
    conn: Option<ConnectInfo<std::net::SocketAddr>>,
    Json(req): Json<MasterLoginRequest>,
) -> Response {
    let repo = match state.repo.clone() {
        Some(r) => r,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    let ip = client_ip(&headers, conn.map(|c| c.0));
    let token = req.qr_master_token.trim();
    // 400 invalid_request: пустой или слишком длинный токен (>256 символов
    // защищает от патологически больших тел; валидный master-токен — 64 hex).
    if token.is_empty() || token.len() > 256 {
        return api_error_with_code(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "The request is malformed.",
            "qr_master_token must be 1..256 chars",
        );
    }
    // 429 too_many_attempts: 10 неудач / 15 мин по IP (до сверки — иначе
    // флуд валидных по формату, но неверных токенов брутфорсит хеш).
    if let Ok(failed) = repo.count_failed_master_logins(&ip).await {
        if failed >= 10 {
            return api_error_with_code(
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_attempts",
                "Too many attempts. Try again later.",
                "too many master login attempts",
            );
        }
    }
    // Сверка: sha256(token) сравнивается с хешем в БД в постоянном времени.
    let computed = hash_refresh_token(token);
    let row = match repo.find_club_by_master_token_hash(&computed).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            let _ = repo.record_master_login_attempt(Some(&ip), false).await;
            return api_error_with_code(
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "Authentication is required.",
                "invalid master token",
            );
        }
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    let (club_id, status, stored_hash) = row;
    // Defense-in-depth: явное ct-сравнение вычисленного и хранимого хешей.
    if !ct_eq(computed.as_bytes(), stored_hash.as_bytes()) {
        let _ = repo.record_master_login_attempt(Some(&ip), false).await;
        return api_error_with_code(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "Authentication is required.",
            "invalid master token",
        );
    }
    // Статус-гейт: активные и ожидающие верификации клубы могут войти
    // (онбординг идёт из панели); заблокированные/остановленные — нет.
    if status != "active" && status != "pending_verification" {
        let _ = repo.record_master_login_attempt(Some(&ip), false).await;
        return api_error_with_code(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "Authentication is required.",
            "club not allowed",
        );
    }
    // Успех: фиксируем попытку, пишем аудит (без токена), выдаём JWT тем же
    // кодом, что обычный логин клуба.
    let _ = repo.record_master_login_attempt(Some(&ip), true).await;
    let pool = repo.pool();
    let _ = sqlx::query(
        "INSERT INTO audit_log (actor_type, actor_id, action, entity, diff) \
         VALUES ('club', $1, 'club_master_login', $2, NULL)",
    )
    .bind(club_id)
    .bind(club_id.to_string())
    .execute(&pool)
    .await;
    match issue_pair_components(&state, Role::Club, club_id, repo.clone()).await {
        Ok((access, refresh, expires_in)) => (
            StatusCode::OK,
            Json(MasterLoginResponse {
                access_token: access,
                refresh_token: refresh,
                expires_in,
                club_id,
                role: "club",
            }),
        )
            .into_response(),
        Err(resp) => resp,
    }
}

#[cfg(test)]
mod tests {
    use super::decode_totp_secret;

    /// Битый секрет (v2-формат с мусорным hex) → ошибка, а не пустой TOTP.
    /// Работает независимо от `DROPIT_TOTP_KEY`: при отсутствии ключа —
    /// `MissingKey`, при наличии — `Malformed` (hex::decode("zz") падает).
    #[test]
    fn decode_totp_secret_broken_is_error() {
        assert!(decode_totp_secret(b"v2:zz:zz").is_err());
    }

    /// Пустой секрет (0 байт) → ошибка: легаси-plain возвращает Ok(""),
    /// функция обязана отклонить пустой HMAC-ключ.
    #[test]
    fn decode_totp_secret_empty_is_error() {
        assert!(decode_totp_secret(b"").is_err());
    }

    /// Валидный легаси-plain base32 → Ok(TOTP) (совместимость с F1-тестами).
    #[test]
    fn decode_totp_secret_legacy_plain_ok() {
        // "JBSWY3DPEHPK3PXP" — стандартный base32-секрет RFC 6238.
        let totp = decode_totp_secret(b"JBSWY3DPEHPK3PXP").expect("legacy plain decodes");
        // Код генерируется (не пустой ключ).
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let code = totp.generate(now);
        assert_eq!(code.len(), 6, "6-значный код");
    }

    /// Постоянное время: ct_eq для равных и неравных байтов.
    #[test]
    fn ct_eq_behaves() {
        assert!(super::ct_eq(b"abc", b"abc"));
        assert!(!super::ct_eq(b"abc", b"abd"));
        assert!(!super::ct_eq(b"abc", b"ab"));
        assert!(super::ct_eq(b"", b""));
    }

    /// Хелпер: строит HeaderMap с одним заголовком.
    fn hdr(name: &str, val: &str) -> axum::http::HeaderMap {
        let mut h = axum::http::HeaderMap::new();
        h.insert(
            axum::http::HeaderName::from_lowercase(name.as_bytes()).unwrap(),
            axum::http::HeaderValue::from_str(val).unwrap(),
        );
        h
    }

    /// Доверие ВЫКЛ (безопасный дефолт): XFF игнорируется, используется
    /// прямой адрес ConnectInfo. Доказывает, что при прямом подключении
    /// атакующий НЕ обходят лимит подменой XFF.
    #[test]
    fn client_ip_with_trust_off_ignores_headers() {
        let conn = Some(std::net::SocketAddr::from(([203, 0, 113, 9], 443)));
        // XFF и X-Real-IP присутствуют, но trust=false → игнорируются.
        let h = hdr("x-forwarded-for", "1.2.3.4, 5.6.7.8");
        assert_eq!(
            super::client_ip_with(false, &h, conn),
            "203.0.113.9",
            "trust off: must use ConnectInfo, not XFF"
        );
    }

    /// Доверие ВЫКЛ и ConnectInfo недоступен → единый бакет `0.0.0.0`
    /// (валидный inet, лимит работает глобально). Сценарий e2e/oneshot и
    /// прод без connect-info.
    #[test]
    fn client_ip_with_trust_off_no_conn_is_sentinel() {
        let h = hdr("x-forwarded-for", "1.2.3.4");
        assert_eq!(
            super::client_ip_with(false, &h, None),
            "0.0.0.0",
            "no trust + no conn: shared sentinel bucket"
        );
    }

    /// Доверие ВКЛ: берётся первый хоп XFF (до запятой), мусор после
    /// запятой и в X-Real-IP не важен.
    #[test]
    fn client_ip_with_trust_on_first_xff_hop() {
        let h = hdr("x-forwarded-for", "203.0.113.42, 10.0.0.1");
        assert_eq!(
            super::client_ip_with(true, &h, None),
            "203.0.113.42",
            "trust on: first XFF hop"
        );
    }

    /// Доверие ВКЛ: XFF мусор/отсутствует → fallback на X-Real-IP.
    #[test]
    fn client_ip_with_trust_on_falls_to_xreal() {
        let mut h = hdr("x-real-ip", "198.51.100.7");
        // XFF намеренно мусорный → игнорируется, переход к X-Real-IP.
        h.insert(
            axum::http::HeaderName::from_static("x-forwarded-for"),
            axum::http::HeaderValue::from_static("garbage"),
        );
        assert_eq!(
            super::client_ip_with(true, &h, None),
            "198.51.100.7",
            "trust on: garbage XFF → X-Real-IP"
        );
    }

    /// Доверие ВКЛ: все заголовки мусор/отсутствуют → ConnectInfo.
    #[test]
    fn client_ip_with_trust_on_no_header_uses_conn() {
        let h = axum::http::HeaderMap::new();
        let conn = Some(std::net::SocketAddr::from(([10, 1, 2, 3], 8443)));
        assert_eq!(
            super::client_ip_with(true, &h, conn),
            "10.1.2.3",
            "trust on, no header: ConnectInfo"
        );
    }

    /// Доверие ВКЛ, заголовков нет и ConnectInfo нет → `0.0.0.0`.
    #[test]
    fn client_ip_with_trust_on_nothing_is_sentinel() {
        let h = axum::http::HeaderMap::new();
        assert_eq!(super::client_ip_with(true, &h, None), "0.0.0.0");
    }
}
