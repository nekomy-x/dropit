//! Публичный онбординг клуба: заявка с Connect-лендинга и её разбор админом.
//!
//! Контракт и обоснование — `apps/web-club/src/api/connect.ts`
//! (`TODO(club-onboarding-endpoint)`) и `apps/web-club/src/connect/types.ts`:
//! лендинг уже собирает черновик и отправляет snake_case-тело, но публичного
//! роута не было — форма честно деградировала в 501 «черновик сохранён».
//! Этот модуль закрывает оба конца сценария: публичную заявку и админский
//! разбор (список / карточка / approve / reject).
//!
//! # Публичный роут
//!
//! `POST /v1/club/onboarding/apply` — без токена (у посетителя лендинга его
//! нет и быть не может), поэтому поверх него стоят ровно те ограничители,
//! которые не дают форме стать каналом флуда:
//!
//! * **валидация до БД**: длины строк, http(s)-сайт, границы чисел
//!   (`capacity 1..100000`, `seats 0..capacity`, `area_m2 0..100000`,
//!   `price_minor 100..100000`, если цена пришла — деньги в минорных
//!   единицах, €1..€1000; компактная форма `/register` цену не спрашивает,
//!   её клуб задаёт в панели после активации).
//!   Первое нарушение → `422 validation_error`, `details.field` называет
//!   конкретное поле (`club.name`, `venue.capacity`, `qr.price_minor`, …),
//!   чтобы фронт подсветил форму, а не показал «что-то не так»;
//! * **лимит 5 заявок/час на IP** (`routes::client_ip`, `TRUST_PROXY`
//!   учитывается ровно так же, как у device-token: без флага заголовки
//!   прокси игнорируются и подмена `X-Forwarded-For` лимит не обходит);
//!   шестая → `429 rate_limited`;
//! * **не больше 3 pending на один contact**: один и тот же e-mail/телефон не
//!   должен занимать очередь модерации пачкой копий. После approve/reject
//!   счётчик pending освобождается — это про очередь, а не про человека;
//! * **идемпотентность**: `Idempotency-Key` (если заголовок есть) хранится в
//!   `club_application.idempotency_key` UNIQUE. Повтор возвращает ту же
//!   заявку и `200` вместо второй строки и `201`; гонка двух одновременных
//!   вставок ловится UNIQUE-нарушением и перечитывает строку.
//!
//! Секретов заявка не принимает и не хранит: пароль нового клуба задаёт
//! только владелец на странице активации. Наружу не уходит ни одного текста
//! ошибки БД: все сбои — статические `500 internal` /
//! `503 external_unavailable`, подробности — в `tracing`.
//!
//! # Одноразовая ссылка активации
//!
//! Approve больше не требует пароля: клуб создаётся с `password_hash = '!'`
//! (verify_password на таком хеше всегда падает, поэтому вход по паролю
//! невозможен) и invite-токеном. Токен — 32 случайных байта в hex (64
//! символа), в БД лежит только sha256; TTL 7 дней. Владелец открывает
//! `GET /v1/club/onboarding/invite/:token`, видит данные заявки и задаёт
//! пароль через `POST …/accept`: Argon2id, `status='active'`, токен
//! зануляется ОДНИМ UPDATE — поэтому повторная ссылка упирается в
//! `invite_expires_at > now()` и получает тот же `404 invite_not_found`,
//! что и чужая. Различать «просрочен», «использован» и «неизвестен» нельзя:
//! иначе ссылка становится оракулом существования клуба.
//!
//! Админ может перевыпустить ссылку (`POST …/:id/invite`) — новый токен,
//! новый TTL, письмо снова в очереди. Перевыпуск запрещён до одобрения
//! (`409 not_approved`) и после активации (`409 invite_already_accepted`).
//!
//! Письмо `club_invite` ставится в очередь best-effort ПОСЛЕ commit:
//! ошибка почты не отменяет решение, а `mail_queued` в ответе — честный
//! факт «строка письма есть в `mail_message`».
//!
//! # Админский разбор
//!
//! Все роуты разбора требуют роль Admin (JWT). Мутации дополнительно проходят
//! `rbac::require_admin_write`: админ с ролью `readonly` получает `403`, а
//! `GET`-ы ему доступны — читать разбор можно, решать нет.
//!
//! * `GET /v1/admin/onboarding-applications?status=&limit=&cursor=` — очередь
//!   модерации. По умолчанию `pending_moderation`, keyset-пагинация по
//!   `(created_at DESC, id DESC)` без OFFSET (`next_cursor` — непрозрачный
//!   base64url; мусор в курсоре даёт `422`, а не молча первую страницу).
//!   Каждая строка несёт `club_id` и `invite_state`
//!   (`none | pending | accepted | expired`), посчитанный JOIN-ом по
//!   `club_application.club_id`, — без N+1 и без расхождения с карточкой;
//! * `GET /v1/admin/onboarding-applications/:id` — полная заявка (payload +
//!   решение + тот же `invite_state`). Неизвестный/битый id — один и тот же
//!   `404`, чтобы id нельзя было перебирать по разнице ответов;
//! * `POST …/:id/approve` — ОДНА транзакция: `SELECT … FOR UPDATE` заявки →
//!   проверка `pending_moderation` → создание клуба → `status='approved'` +
//!   `reviewed_by/at` → `audit_log`. Повторный approve → `409
//!   already_reviewed`, а не второй клуб;
//! * `POST …/:id/reject` — та же транзакция с `reason` (1..500) →
//!   `status='rejected'` + `reject_reason` + audit. Повтор → `409`;
//! * `POST …/:id/invite` — перевыпуск ссылки активации (см. выше), audit
//!   `club_application.invite`.
//!
//! ## Решение: статус созданного клуба — `pending_verification`
//!
//! Публичная заявка не доказывает владение почтовым ящиком: адрес ввёл
//! посетитель, письмо никуда не уходило. Approve — это решение модератора
//! «заявка правдоподобна», а не подтверждение e-mail, поэтому клуб получает
//! тот же `pending_verification`, что и клуб, созданный админом
//! (`panel_routes::admin_clubs_create`). `qr_master_token` сразу пригоден —
//! мастер-вход пускает `pending_verification`/`active` (`routes.rs`), — а
//! перевод в `active` делает accept ссылки (владение e-mail подтверждено
//! самой ссылкой). Легаси-approve с паролем сохраняет старое поведение:
//! пароль хешируется, клуб входит им сразу, но invite-ссылка тоже
//! выпускается — владелец может активировать аккаунт сам.
//!
//! ## Решение: slug
//!
//! Явный `slug` из тела нормализуется (нижний регистр) и валидируется
//! (`[a-z0-9-]`, 2..60). Без него slug генерируется из названия клуба тем же
//! транслитом, что у открытой регистрации (`signup_routes::slugify`:
//! «Клуб Подвал» → `klub-podval`), а занятость проверяется внутри транзакции:
//! кандидат `base`, `base-2`, … `base-50`. Гонка с параллельным approve
//! закрыта UNIQUE-ограничением `club.slug` — нарушение маппится в `409
//! slug_taken`, как и занятый e-mail → `409 email_taken` (CITEXT, регистр не
//! важен).
//!
//! Роутер монтируется в `routes::router` БЕЗ `role_route`: публичный apply и
//! админские роуты живут в одном роутере, поэтому роль проверяется внутри
//! хендлеров через `AuthContext` — как у `panel_routes` и `admin_*`.

use std::net::SocketAddr;

use axum::{
    extract::{rejection::JsonRejection, ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use dropit_common::{ApiError, Role};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::types::Json as DbJson;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::api_key_auth::error_envelope;
use crate::auth;
use crate::rbac::{auth_error_response, require_admin_write, AuthContext};
use crate::routes::{api_error, client_ip, ApiEnv, TokenPair};

// ============================================================
// Лимиты и границы (одно место, чтобы валидация и тесты не разъезжались)
// ============================================================

/// Антифрод: 5 заявок в час с одного IP; шестая — `429`.
const APPLY_RATE_LIMIT_PER_HOUR: i64 = 5;

/// Очередь модерации на один contact: не больше 3 pending одновременно.
const CONTACT_PENDING_LIMIT: i64 = 3;

/// Длина `Idempotency-Key` (заголовок приходит от клиента, ограничиваем).
const IDEMPOTENCY_KEY_MAX_CHARS: usize = 128;

/// Пределы `limit` админского списка.
const DEFAULT_LIST_LIMIT: i64 = 50;
const MAX_LIST_LIMIT: i64 = 100;

/// `name` / `contact` / `venue.name`: непустые, 1..120.
const MAX_NAME_CHARS: usize = 120;
const MAX_CONTACT_CHARS: usize = 120;
/// `city` / `address` — до 200; `district` / `type` — короткие подписи.
const MAX_CITY_CHARS: usize = 200;
const MAX_ADDRESS_CHARS: usize = 200;
const MAX_DISTRICT_CHARS: usize = 120;
const MAX_VENUE_TYPE_CHARS: usize = 60;
/// Сайт: URL разумной длины (в URL-usage 300 символов — уже аномалия).
const MAX_WEBSITE_CHARS: usize = 300;

/// Жанры: не больше 8 по 40 символов; зоны: не больше 12 по 60.
const MAX_GENRES: usize = 8;
const MAX_GENRE_CHARS: usize = 40;
const MAX_ZONES: usize = 12;
const MAX_ZONE_CHARS: usize = 60;

/// Числовые границы заявки (деньги — минорные единицы).
const MAX_CAPACITY: i64 = 100_000;
const MAX_AREA_M2: i64 = 100_000;
const MIN_PRICE_MINOR: i64 = 100;
const MAX_PRICE_MINOR: i64 = 100_000;

/// Пароль нового клуба при approve/активации — те же границы, что у
/// регистрации.
const MIN_PASSWORD_CHARS: usize = 8;
const MAX_PASSWORD_CHARS: usize = 128;

/// TTL одноразовой ссылки активации клуба (7 дней, контракт).
const INVITE_TTL_DAYS: i64 = 7;

/// Длина invite-токена: 32 случайных байта в hex = 64 символа.
///
/// В БД — только sha256 (как у refresh- и master-токенов): утечка дампа не
/// даёт ссылку, а перебор 256-битного значения невозможен.
const INVITE_TOKEN_CHARS: usize = 64;

/// Префикс токена в `idempotency_key` письма: различает перевыпуски одной
/// ссылки, не раскрывая сам токен в журнале почты.
const INVITE_KEY_PREFIX_CHARS: usize = 8;

/// Сколько суффиксов пробуем, прежде чем считать slug-пространство занятым.
const SLUG_MAX_SUFFIX: u32 = 50;

/// Статусы заявки (совпадают с CHECK в миграции 0021).
const STATUS_PENDING: &str = "pending_moderation";
const STATUS_APPROVED: &str = "approved";
const STATUS_REJECTED: &str = "rejected";

/// Разделитель внутри открытого курсора (до base64url).
const CURSOR_SEP: char = '|';

// ============================================================
// Роутер
// ============================================================

/// Публичные роуты онбординга + админский разбор заявок.
pub fn onboarding_router() -> Router<ApiEnv> {
    Router::new()
        .route("/v1/club/onboarding/apply", post(apply))
        // Публичная активация: ссылка из письма — единственный секрет,
        // поэтому оба роута без токена и без RBAC.
        .route("/v1/club/onboarding/invite/:token", get(invite_preview))
        .route(
            "/v1/club/onboarding/invite/:token/accept",
            post(invite_accept),
        )
        .route("/v1/admin/onboarding-applications", get(list_applications))
        .route(
            "/v1/admin/onboarding-applications/:id",
            get(get_application),
        )
        .route(
            "/v1/admin/onboarding-applications/:id/approve",
            post(approve_application),
        )
        .route(
            "/v1/admin/onboarding-applications/:id/reject",
            post(reject_application),
        )
        .route(
            "/v1/admin/onboarding-applications/:id/invite",
            post(reissue_invite),
        )
}

// ============================================================
// DTO: публичная заявка
// ============================================================

/// Тело `POST /v1/club/onboarding/apply`.
///
/// Все поля — `Option`, чтобы отсутствие/`null` конкретного поля давало
/// `422` с ТОЧНЫМ `details.field`, а не безликий serde-отказ на всё тело.
/// Неизвестные ключи (в т.ч. гипотетический `password`) игнорируются: заявка
/// не принимает секретов, поэтому им некуда попасть — ни в payload, ни в БД.
#[derive(Deserialize)]
struct ApplyRequest {
    club: Option<ClubPayload>,
    venue: Option<VenuePayload>,
    qr: Option<QrPayload>,
}

/// Блок клуба (`ConnectApplicationRequest['club']`).
#[derive(Deserialize)]
struct ClubPayload {
    name: Option<String>,
    city: Option<String>,
    address: Option<String>,
    website: Option<String>,
    contact: Option<String>,
    genres: Option<Vec<String>>,
}

/// Блок площадки (`ConnectApplicationRequest['venue']`).
#[derive(Deserialize)]
struct VenuePayload {
    name: Option<String>,
    #[serde(rename = "type")]
    venue_type: Option<String>,
    address: Option<String>,
    city: Option<String>,
    district: Option<String>,
    capacity: Option<i64>,
    seats: Option<i64>,
    area_m2: Option<i64>,
}

/// QR-блок (`ConnectApplicationRequest['qr']`).
#[derive(Deserialize)]
struct QrPayload {
    price_minor: Option<i64>,
    zones: Option<Vec<String>>,
}

/// Ответ apply: `201` при создании, `200` при идемпотентном повторе.
#[derive(Serialize)]
struct ApplyResponse {
    application_id: Uuid,
    status: String,
    submitted_at: DateTime<Utc>,
}

/// Заявка после валидации и нормализации: готовые JSONB-блоки + колонки.
struct ValidatedApplication {
    club: Value,
    venue: Value,
    qr: Value,
    club_name: String,
    city: String,
    contact: String,
    email: Option<String>,
    /// `None`, когда заявка не называет цену (компактная форма `/register`).
    price_minor: Option<i64>,
}

// ============================================================
// DTO: админский разбор
// ============================================================

/// Query `GET /v1/admin/onboarding-applications`.
///
/// Всё — строки: битые `limit`/`cursor`/`status` разбираются вручную, чтобы
/// ответ оставался конвертом `422 validation_error` с `details.field`, а не
/// текстовым отказом axum.
#[derive(Deserialize)]
struct ListParams {
    status: Option<String>,
    limit: Option<String>,
    cursor: Option<String>,
}

/// Тело `POST /v1/admin/onboarding-applications/:id/approve`.
///
/// Все поля необязательны:
/// * `email` — по умолчанию берётся `club_application.email` (заполняется
///   при apply только если contact — валидный e-mail). Нет ни там, ни там →
///   `422 field=email`: без адреса нельзя ни пригласить владельца, ни войти;
/// * `password` — **deprecated**. Новый поток пароль не принимает: клуб
///   создаётся с `password_hash = '!'` и ссылкой активации. Поле сохранено
///   ради старых клиентов/e2e: если передано, работает прежнее поведение;
/// * `slug` — явный slug; без него генерируется из названия клуба.
#[derive(Deserialize)]
struct ApproveRequest {
    slug: Option<String>,
    email: Option<String>,
    password: Option<String>,
}

/// Тело `POST /v1/club/onboarding/invite/:token/accept`.
///
/// Единственное поле — пароль: данные заявки (клуб, город, e-mail) уже
/// известны из токена и показываются страницей активации только для чтения.
#[derive(Deserialize)]
struct InviteAcceptRequest {
    password: Option<String>,
}

/// Клуб в ответе активации: то, что панель кладёт в состояние сессии.
#[derive(Serialize)]
struct InviteClub {
    id: Uuid,
    name: String,
    slug: String,
    email: String,
}

/// Ответ активации: `TokenPair` (авто-логин) + `club`.
///
/// Поля пары развёрнуты в корень (`#[serde(flatten)]`) — ровно как у
/// открытой регистрации: панель не разбирает вложенность и не делает
/// второй запрос на логин.
#[derive(Serialize)]
struct InviteAcceptResponse {
    #[serde(flatten)]
    tokens: TokenPair,
    club: InviteClub,
}

/// Тело `POST /v1/admin/onboarding-applications/:id/reject`.
#[derive(Deserialize)]
struct RejectRequest {
    reason: Option<String>,
}

/// Строка админского списка.
#[derive(sqlx::FromRow)]
struct ApplicationListRow {
    id: Uuid,
    club_name: String,
    city: String,
    contact: String,
    price_minor: Option<i64>,
    status: String,
    created_at: DateTime<Utc>,
    reviewed_at: Option<DateTime<Utc>>,
    club_id: Option<Uuid>,
    invite_state: String,
}

/// Полная заявка для карточки.
#[derive(sqlx::FromRow)]
struct ApplicationFullRow {
    id: Uuid,
    club: DbJson<Value>,
    venue: DbJson<Value>,
    qr: DbJson<Value>,
    club_name: String,
    city: String,
    contact: String,
    email: Option<String>,
    price_minor: Option<i64>,
    status: String,
    reject_reason: Option<String>,
    reviewed_by: Option<Uuid>,
    reviewed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    source_ip: String,
    club_id: Option<Uuid>,
    invite_state: String,
}

/// Заявка, залоченная под решение (`SELECT … FOR UPDATE`).
#[derive(sqlx::FromRow)]
struct LockedApplication {
    status: String,
    club_name: String,
    city: String,
    email: Option<String>,
    club: DbJson<Value>,
    venue: DbJson<Value>,
}

/// Заявка, залоченная под перевыпуск ссылки.
#[derive(sqlx::FromRow)]
struct LockedReview {
    status: String,
    club_id: Option<Uuid>,
}

/// Публичные данные ссылки активации (без секретов и без пароля).
#[derive(sqlx::FromRow)]
struct InvitePreviewRow {
    name: String,
    city: Option<String>,
    email: String,
    contact: String,
    slug: String,
    invite_expires_at: DateTime<Utc>,
}

/// Клуб, найденный по хешу invite-токена (для активации).
#[derive(sqlx::FromRow)]
struct InviteClubRow {
    id: Uuid,
    name: String,
    slug: String,
    email: String,
}

/// Свежая одноразовая ссылка: plaintext живёт только в ответе/письме.
struct IssuedInvite {
    token: String,
    expires_at: DateTime<Utc>,
}

/// Строка клуба для перевыпуска: имя/почта для письма и текущий токен,
/// чтобы отличить «уже активирован» от «можно выпустить новый».
#[derive(sqlx::FromRow)]
struct ReissueClubRow {
    name: String,
    email: String,
    invite_token_hash: Option<String>,
}

// ============================================================
// Ошибки: единый конверт, без утечки внутренностей
// ============================================================

/// Нарушение правил валидации: `422 validation_error` + `details.field`.
#[derive(Debug)]
struct FieldError {
    field: &'static str,
    message: String,
}

impl FieldError {
    fn new(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }
}

/// `422 validation_error` с именем поля — единственный формат отказа
/// валидации во всём модуле.
fn field_error(err: FieldError) -> Response {
    error_envelope(
        StatusCode::UNPROCESSABLE_ENTITY,
        "validation_error",
        &err.message,
        json!({ "field": err.field }),
    )
}

/// Тело не разобралось как JSON нужной формы. Текст отказа axum НЕ
/// пересказываем: публичному роуту незачем отражать ввод.
fn body_rejected(_rejection: JsonRejection) -> Response {
    error_envelope(
        StatusCode::UNPROCESSABLE_ENTITY,
        "validation_error",
        "The request failed validation.",
        json!({
            "field": "body",
            "detail": "request body must be valid JSON matching the application schema",
        }),
    )
}

/// `404` на неизвестную и битую ссылку — один ответ, чтобы id нельзя было
/// перебирать по разнице «битый uuid vs несуществующий».
fn application_not_found() -> Response {
    error_envelope(
        StatusCode::NOT_FOUND,
        "not_found",
        "Onboarding application not found.",
        json!({ "resource": "club_application" }),
    )
}

/// `409 already_reviewed`: заявку уже разобрали (approve или reject).
fn already_reviewed(status: &str) -> Response {
    error_envelope(
        StatusCode::CONFLICT,
        "already_reviewed",
        "This application has already been reviewed.",
        json!({ "status": status }),
    )
}

/// `409 email_taken` — адрес уже занят другим клубом (CITEXT).
fn email_taken() -> Response {
    error_envelope(
        StatusCode::CONFLICT,
        "email_taken",
        "This e-mail is already registered.",
        json!({ "field": "email" }),
    )
}

/// `409 slug_taken` — slug занят другим клубом.
fn slug_taken(slug: &str) -> Response {
    error_envelope(
        StatusCode::CONFLICT,
        "slug_taken",
        "This slug is already in use.",
        json!({ "field": "slug", "slug": slug }),
    )
}

/// `404 invite_not_found` — один ответ на неизвестный, использованный,
/// просроченный и битый токен.
///
/// Различать случаи нельзя: иначе ссылка становится оракулом «этот клуб
/// существует и уже активирован». Формат токена проверяется до БД, но ответ
/// тот же.
fn invite_not_found() -> Response {
    error_envelope(
        StatusCode::NOT_FOUND,
        "invite_not_found",
        "This invitation link is invalid or has expired.",
        json!({ "resource": "club_invite" }),
    )
}

/// `422 validation_failed` для публичной активации.
///
/// Код именно `validation_failed` (контракт §«Клуб: активация»), а не
/// `validation_error`: так поле пароля отличает страницу активации от
/// страницы заявки, у которой свой код.
fn validation_failed(field: &'static str, message: impl Into<String>) -> Response {
    error_envelope(
        StatusCode::UNPROCESSABLE_ENTITY,
        "validation_failed",
        &message.into(),
        json!({ "field": field }),
    )
}

/// `409 not_approved` — перевыпуск ссылки до одобрения заявки.
fn not_approved() -> Response {
    error_envelope(
        StatusCode::CONFLICT,
        "not_approved",
        "This application is not approved yet.",
        json!({ "resource": "club_application" }),
    )
}

/// `409 invite_already_accepted` — клуб уже активирован, ссылка не нужна.
fn invite_already_accepted() -> Response {
    error_envelope(
        StatusCode::CONFLICT,
        "invite_already_accepted",
        "This club has already set a password.",
        json!({ "resource": "club_invite" }),
    )
}

/// `429 rate_limited` — исчерпан IP-лимит или лимит pending на contact.
fn rate_limited(detail: &str) -> Response {
    api_error(ApiError::RateLimited, detail)
}

/// `503 external_unavailable` — БД не сконфигурирована.
fn database_not_configured() -> Response {
    api_error(
        ApiError::ExternalServiceUnavailable,
        "database not configured",
    )
}

/// Пул БД или `None` (хендлер сам решает, каким кодом ответить).
fn database_pool(state: &ApiEnv) -> Option<PgPool> {
    state.repo.as_ref().map(|repo| repo.pool())
}

// ============================================================
// Хендлер: публичная заявка
// ============================================================

/// `POST /v1/club/onboarding/apply` → `201` (или `200` на повтор ключа).
///
/// Порядок проверок выбран так, чтобы ни один дешёвый отказ не трогал БД, а
/// идемпотентный повтор не упирался в собственный лимит:
/// тело → валидация → ключ идемпотентности (повтор → `200`) → лимит по IP →
/// лимит pending по contact → вставка.
async fn apply(
    State(state): State<ApiEnv>,
    headers: HeaderMap,
    conn: Option<ConnectInfo<SocketAddr>>,
    body: Result<Json<ApplyRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match body {
        Ok(json) => json,
        Err(rejection) => return body_rejected(rejection),
    };
    let application = match validate_apply(request) {
        Ok(application) => application,
        Err(err) => return field_error(err),
    };

    let Some(pool) = database_pool(&state) else {
        return database_not_configured();
    };
    let ip = client_ip(&headers, conn.map(|info| info.0));
    let idempotency = match idempotency_key(&headers) {
        Ok(key) => key,
        Err(err) => return field_error(err),
    };

    // Идемпотентный повтор: ключ найден — возвращаем ту же заявку (200).
    // Проверяем ДО лимитера: честный retry клиента не должен получать 429.
    if let Some(key) = idempotency.as_deref() {
        match find_by_idempotency_key(&pool, key).await {
            Ok(Some(existing)) => return (StatusCode::OK, Json(existing)).into_response(),
            Ok(None) => {}
            Err(_) => return api_error(ApiError::Internal, "db error"),
        }
    }

    // Антифрод по IP. Ошибка счётчика не блокирует заявку (fail-open, как у
    // сброса пароля: лимитер — антифрод, а не условие корректности), но
    // обязана быть видна в логе.
    match count_by_ip(&pool, &ip).await {
        Ok(count) if count >= APPLY_RATE_LIMIT_PER_HOUR => {
            return rate_limited("onboarding application limit per IP reached")
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(error = %error, "onboarding apply: ip rate-limit counter failed")
        }
    }
    match count_pending_by_contact(&pool, &application.contact).await {
        Ok(count) if count >= CONTACT_PENDING_LIMIT => {
            return rate_limited("pending onboarding applications limit per contact reached")
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(error = %error, "onboarding apply: contact counter failed")
        }
    }

    let application_id = Uuid::now_v7();
    let submitted_at = Utc::now();
    let inserted = sqlx::query(
        "INSERT INTO club_application \
         (id, club, venue, qr, club_name, city, contact, email, price_minor, \
          status, created_at, idempotency_key, source_ip) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13::inet)",
    )
    .bind(application_id)
    .bind(DbJson(application.club))
    .bind(DbJson(application.venue))
    .bind(DbJson(application.qr))
    .bind(&application.club_name)
    .bind(&application.city)
    .bind(&application.contact)
    .bind(application.email.as_deref())
    .bind(application.price_minor)
    .bind(STATUS_PENDING)
    .bind(submitted_at)
    .bind(idempotency.as_deref())
    .bind(&ip)
    .execute(&pool)
    .await;

    if let Err(error) = inserted {
        // Гонка двух одновременных POST с одним Idempotency-Key: UNIQUE
        // нарушен — перечитываем строку, которую успел вставить сосед.
        if unique_constraint(&error).is_some() {
            if let Some(key) = idempotency.as_deref() {
                if let Ok(Some(existing)) = find_by_idempotency_key(&pool, key).await {
                    return (StatusCode::OK, Json(existing)).into_response();
                }
            }
        }
        tracing::warn!(error = %error, "onboarding apply: insert failed");
        return api_error(ApiError::Internal, "db error");
    }

    tracing::info!(
        application_id = %application_id,
        "club_application_created"
    );
    (
        StatusCode::CREATED,
        Json(ApplyResponse {
            application_id,
            status: STATUS_PENDING.to_string(),
            submitted_at,
        }),
    )
        .into_response()
}

// ============================================================
// Хендлеры: админский разбор
// ============================================================

/// `GET /v1/admin/onboarding-applications` — очередь модерации.
///
/// Фильтр по статусу обязателен по смыслу: по умолчанию отдаётся
/// `pending_moderation` (то, что реально ждёт решения). Keyset-пагинация
/// `(created_at DESC, id DESC)` устойчива к вставкам во время листания.
async fn list_applications(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Query(params): Query<ListParams>,
) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let status = match parse_status_filter(params.status.as_deref()) {
        Ok(status) => status,
        Err(err) => return field_error(err),
    };
    let limit = match parse_limit(params.limit.as_deref()) {
        Ok(limit) => limit,
        Err(err) => return field_error(err),
    };
    let cursor = match decode_cursor(params.cursor.as_deref()) {
        Ok(cursor) => cursor,
        Err(err) => return field_error(err),
    };
    let Some(pool) = database_pool(&state) else {
        return database_not_configured();
    };

    let cursor_at = cursor.map(|(at, _)| at);
    let cursor_id = cursor.map(|(_, id)| id);
    // LEFT JOIN, а не INNER: pending/rejected заявки клуба не имеют.
    // invite_state выводится из клуба ровно по контракту:
    //   club_id NULL → none; хеш NULL → accepted; срок в будущем →
    //   pending; иначе → expired. CASE считает это одним запросом, чтобы
    //   список не делал N+1 и не расходился с карточкой.
    let rows = sqlx::query_as::<_, ApplicationListRow>(
        "SELECT a.id, a.club_name, a.city, a.contact, a.price_minor, a.status, \
                a.created_at, a.reviewed_at, c.id AS club_id, \
                CASE WHEN c.id IS NULL THEN 'none' \
                     WHEN c.invite_token_hash IS NULL THEN 'accepted' \
                     WHEN c.invite_expires_at > now() THEN 'pending' \
                     ELSE 'expired' END AS invite_state \
         FROM club_application a \
         LEFT JOIN club c ON c.id = a.club_id \
         WHERE a.status = $1 \
           AND ($2::timestamptz IS NULL \
                OR a.created_at < $2 \
                OR (a.created_at = $2 AND a.id < $3)) \
         ORDER BY a.created_at DESC, a.id DESC \
         LIMIT $4",
    )
    .bind(&status)
    .bind(cursor_at)
    .bind(cursor_id)
    .bind(limit + 1)
    .fetch_all(&pool)
    .await;
    let rows = match rows {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(error = %error, "onboarding list: db error");
            return api_error(ApiError::Internal, "db error");
        }
    };

    let has_more = rows.len() as i64 > limit;
    let page = &rows[..rows.len().min(limit as usize)];
    let next_cursor = if has_more {
        page.last().map(|row| encode_cursor(row.created_at, row.id))
    } else {
        None
    };
    let items: Vec<Value> = page
        .iter()
        .map(|row| {
            json!({
                "id": row.id,
                "club_name": row.club_name,
                "city": row.city,
                "contact": row.contact,
                "price_minor": row.price_minor,
                "status": row.status,
                "created_at": row.created_at,
                "reviewed_at": row.reviewed_at,
                "club_id": row.club_id,
                "invite_state": row.invite_state,
            })
        })
        .collect();

    (
        StatusCode::OK,
        Json(json!({ "items": items, "next_cursor": next_cursor })),
    )
        .into_response()
}

/// `GET /v1/admin/onboarding-applications/:id` — полная заявка.
async fn get_application(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(raw_id): Path<String>,
) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let Some(application_id) = parse_application_id(&raw_id) else {
        return application_not_found();
    };
    let Some(pool) = database_pool(&state) else {
        return database_not_configured();
    };

    let row = sqlx::query_as::<_, ApplicationFullRow>(
        "SELECT a.id, a.club, a.venue, a.qr, a.club_name, a.city, a.contact, \
                a.email, a.price_minor, a.status, a.reject_reason, a.reviewed_by, \
                a.reviewed_at, a.created_at, host(a.source_ip) AS source_ip, \
                c.id AS club_id, \
                CASE WHEN c.id IS NULL THEN 'none' \
                     WHEN c.invite_token_hash IS NULL THEN 'accepted' \
                     WHEN c.invite_expires_at > now() THEN 'pending' \
                     ELSE 'expired' END AS invite_state \
         FROM club_application a \
         LEFT JOIN club c ON c.id = a.club_id \
         WHERE a.id = $1",
    )
    .bind(application_id)
    .fetch_optional(&pool)
    .await;
    let row = match row {
        Ok(Some(row)) => row,
        Ok(None) => return application_not_found(),
        Err(error) => {
            tracing::warn!(error = %error, "onboarding get: db error");
            return api_error(ApiError::Internal, "db error");
        }
    };

    (
        StatusCode::OK,
        Json(json!({
            "id": row.id,
            "club": row.club.0,
            "venue": row.venue.0,
            "qr": row.qr.0,
            "club_name": row.club_name,
            "city": row.city,
            "contact": row.contact,
            "email": row.email,
            "price_minor": row.price_minor,
            "status": row.status,
            "reject_reason": row.reject_reason,
            "reviewed_by": row.reviewed_by,
            "reviewed_at": row.reviewed_at,
            "created_at": row.created_at,
            "source_ip": row.source_ip,
            "club_id": row.club_id,
            "invite_state": row.invite_state,
        })),
    )
        .into_response()
}

/// `POST /v1/admin/onboarding-applications/:id/approve` — заявка → клуб.
///
/// Одна транзакция: строка заявки лочится `FOR UPDATE` (параллельный approve
/// ждёт и затем видит `approved` → `409 already_reviewed`), создаётся клуб в
/// `pending_verification` с master-токеном (в БД только sha256-хеш) и
/// одноразовой invite-ссылкой, заявка помечается `approved` и связывается с
/// клубом (`club_application.club_id`), пишется `audit_log`. Если любая
/// запись не удалась — откат целиком: клуб без решения или решение без
/// клуба недопустимы.
///
/// Пароль здесь больше не обязателен: `password_hash = '!'` означает, что
/// вход по паролю невозможен до активации. Переданный `password` (старые
/// клиенты) сохраняет прежнее поведение и хешируется Argon2id. Ссылка
/// выпускается в обоих случаях: владелец может активировать/переустановить
/// пароль сам, а легаси-клуб с паролем продолжает работать как раньше.
///
/// Письмо `club_invite` ставится в очередь ПОСЛЕ commit и best-effort:
/// почта не может отменить уже принятое решение, а `mail_queued` честно
/// говорит панели, ушло ли письмо на самом деле.
async fn approve_application(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(raw_id): Path<String>,
    body: Result<Json<ApproveRequest>, JsonRejection>,
) -> Response {
    if let Err(response) = require_admin_write(&ctx, &state).await {
        return response;
    }
    let Some(application_id) = parse_application_id(&raw_id) else {
        return application_not_found();
    };
    let Json(request) = match body {
        Ok(json) => json,
        Err(rejection) => return body_rejected(rejection),
    };

    // Сначала — тело (дешёвые отказы), затем заявка в БД (404/409), затем
    // хеши: Argon2id ~ десятки миллисекунд, незачем держать блокировку
    // строки. E-mail в теле проверяется по формату, но не требуется:
    // fallback на `club_application.email` делается после чтения заявки.
    let request_email = match optional_email(request.email.as_deref()) {
        Ok(email) => email,
        Err(err) => return field_error(err),
    };
    let password = match optional_password(request.password.as_deref()) {
        Ok(password) => password,
        Err(err) => return field_error(err),
    };
    let explicit_slug = match parse_slug(request.slug.as_deref()) {
        Ok(slug) => slug,
        Err(err) => return field_error(err),
    };
    let password_hash = match password {
        Some(password) => match auth::hash_password(password) {
            Ok(hash) => hash,
            Err(_) => return api_error(ApiError::Internal, "password hashing failed"),
        },
        // '!' — заведомо не Argon2id: verify_password вернёт ошибку, поэтому
        // до активации клуб не может войти паролем. Секрет входа — ссылка.
        None => "!".to_string(),
    };
    let master_token = generate_master_token();
    let master_hash = auth::hash_refresh_token(&master_token);
    let invite = issue_invite();

    let Some(pool) = database_pool(&state) else {
        return database_not_configured();
    };
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    let locked = sqlx::query_as::<_, LockedApplication>(
        "SELECT status, club_name, city, email, club, venue \
         FROM club_application WHERE id = $1 FOR UPDATE",
    )
    .bind(application_id)
    .fetch_optional(&mut *tx)
    .await;
    let locked = match locked {
        Ok(Some(row)) => row,
        Ok(None) => return application_not_found(),
        Err(error) => {
            tracing::warn!(error = %error, "onboarding approve: lock failed");
            return api_error(ApiError::Internal, "db error");
        }
    };
    if locked.status != STATUS_PENDING {
        return already_reviewed(&locked.status);
    }

    // E-mail: явный из тела → иначе из заявки. Оба пусты — 422 email:
    // без адреса владелец не получит ссылку и не сможет войти.
    let email = match request_email.or_else(|| {
        locked
            .email
            .as_deref()
            .map(str::trim)
            .filter(|email| !email.is_empty())
            .map(|email| email.to_lowercase())
    }) {
        Some(email) => email,
        None => {
            return field_error(FieldError::new(
                "email",
                "email is required to create the club",
            ))
        }
    };

    let slug = match resolve_slug(&mut tx, &locked.club_name, explicit_slug.as_deref()).await {
        Ok(slug) => slug,
        Err(SlugError::Taken) => {
            return match explicit_slug.as_deref() {
                Some(slug) => slug_taken(slug),
                None => slug_taken("(generated)"),
            }
        }
        Err(SlugError::Db) => return api_error(ApiError::Internal, "db error"),
    };

    let club_id = Uuid::now_v7();
    let address = payload_text(&locked.club.0, "address")
        .or_else(|| payload_text(&locked.venue.0, "address"));
    let city = Some(locked.city.as_str()).filter(|city| !city.is_empty());
    let inserted = sqlx::query(
        "INSERT INTO club \
         (id, name, slug, email, password_hash, address, city, \
          qr_master_token_hash, status, invite_token_hash, invite_expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending_verification', $9, $10)",
    )
    .bind(club_id)
    .bind(&locked.club_name)
    .bind(&slug)
    .bind(&email)
    .bind(&password_hash)
    .bind(address)
    .bind(city)
    .bind(&master_hash)
    .bind(auth::hash_refresh_token(&invite.token))
    .bind(invite.expires_at)
    .execute(&mut *tx)
    .await;
    if let Err(error) = inserted {
        // UNIQUE (email) / (slug): гонка с параллельным approve или
        // регистрацией. Нарушение не отменяет заявку — она остаётся pending.
        if let Some(constraint) = unique_constraint(&error) {
            if constraint.contains("email") {
                return email_taken();
            }
            if constraint.contains("slug") {
                return slug_taken(&slug);
            }
        }
        tracing::warn!(error = %error, "onboarding approve: club insert failed");
        return api_error(ApiError::Internal, "db error");
    }

    let reviewed_at = Utc::now();
    // club_id — связь заявки с клубом: по ней админский список считает
    // invite_state, а перевыпуск ссылки находит клуб без поиска по e-mail.
    if sqlx::query(
        "UPDATE club_application \
         SET status = $2, reject_reason = NULL, reviewed_by = $3, reviewed_at = $4, \
             club_id = $5 \
         WHERE id = $1",
    )
    .bind(application_id)
    .bind(STATUS_APPROVED)
    .bind(ctx.subject_id())
    .bind(reviewed_at)
    .bind(club_id)
    .execute(&mut *tx)
    .await
    .is_err()
    {
        return api_error(ApiError::Internal, "db error");
    }
    if write_audit(
        &mut tx,
        ctx.subject_id(),
        "club_application.approve",
        application_id,
        json!({
            "application_id": application_id,
            "club_id": club_id,
            "slug": slug,
            "email": email,
            "status": STATUS_APPROVED,
            "invite_expires_at": invite.expires_at,
        }),
    )
    .await
    .is_err()
    {
        return api_error(ApiError::Internal, "db error");
    }
    if tx.commit().await.is_err() {
        return api_error(ApiError::Internal, "db error");
    }

    // Best effort: письмо не отменяет решение. `mail_queued` — честный факт
    // «строка письма есть в очереди», а не «почта настроена».
    let mail_queued = enqueue_club_invite_mail(
        &state,
        club_id,
        &locked.club_name,
        &email,
        &invite.token,
        invite.expires_at,
    )
    .await;

    tracing::info!(
        application_id = %application_id,
        club_id = %club_id,
        mail_queued,
        "club_application_approved"
    );
    (
        StatusCode::OK,
        Json(json!({
            "application_id": application_id,
            "club_id": club_id,
            "status": STATUS_APPROVED,
            "email": email,
            "invite_url": invite_url(&invite.token),
            "invite_expires_at": invite.expires_at,
            "mail_queued": mail_queued,
            "qr_master_token": master_token,
        })),
    )
        .into_response()
}

/// `POST /v1/admin/onboarding-applications/:id/reject` — отказ с причиной.
///
/// Причина (1..500) сохраняется в `reject_reason` и в audit: она объясняет
/// решение и попадает в карточку заявки. Повторный reject/approve — `409`.
async fn reject_application(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(raw_id): Path<String>,
    body: Result<Json<RejectRequest>, JsonRejection>,
) -> Response {
    if let Err(response) = require_admin_write(&ctx, &state).await {
        return response;
    }
    let Some(application_id) = parse_application_id(&raw_id) else {
        return application_not_found();
    };
    let Json(request) = match body {
        Ok(json) => json,
        Err(rejection) => return body_rejected(rejection),
    };
    let reason = match required_reason(request.reason.as_deref()) {
        Ok(reason) => reason,
        Err(err) => return field_error(err),
    };

    let Some(pool) = database_pool(&state) else {
        return database_not_configured();
    };
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    let status = sqlx::query_scalar::<_, String>(
        "SELECT status FROM club_application WHERE id = $1 FOR UPDATE",
    )
    .bind(application_id)
    .fetch_optional(&mut *tx)
    .await;
    let status = match status {
        Ok(Some(status)) => status,
        Ok(None) => return application_not_found(),
        Err(error) => {
            tracing::warn!(error = %error, "onboarding reject: lock failed");
            return api_error(ApiError::Internal, "db error");
        }
    };
    if status != STATUS_PENDING {
        return already_reviewed(&status);
    }

    let reviewed_at = Utc::now();
    if sqlx::query(
        "UPDATE club_application \
         SET status = $2, reject_reason = $3, reviewed_by = $4, reviewed_at = $5 \
         WHERE id = $1",
    )
    .bind(application_id)
    .bind(STATUS_REJECTED)
    .bind(&reason)
    .bind(ctx.subject_id())
    .bind(reviewed_at)
    .execute(&mut *tx)
    .await
    .is_err()
    {
        return api_error(ApiError::Internal, "db error");
    }
    if write_audit(
        &mut tx,
        ctx.subject_id(),
        "club_application.reject",
        application_id,
        json!({
            "application_id": application_id,
            "reason": reason,
            "status": STATUS_REJECTED,
        }),
    )
    .await
    .is_err()
    {
        return api_error(ApiError::Internal, "db error");
    }
    if tx.commit().await.is_err() {
        return api_error(ApiError::Internal, "db error");
    }

    tracing::info!(
        application_id = %application_id,
        "club_application_rejected"
    );
    (
        StatusCode::OK,
        Json(json!({
            "application_id": application_id,
            "status": STATUS_REJECTED,
            "reviewed_at": reviewed_at,
        })),
    )
        .into_response()
}

// ============================================================
// Хендлеры: активация клуба по одноразовой ссылке
// ============================================================

/// `GET /v1/club/onboarding/invite/:token` — публичные данные ссылки.
///
/// Страница активации показывает то, что владелец уже отправлял в заявке
/// (клуб, город, e-mail), и НЕ показывает ничего сверх этого: ни id, ни
/// цену, ни статус модерации. Единственный секрет ссылки — сам токен,
/// поэтому ответ одинаков и для валидной ссылки, и для чужой: иначе по
/// разнице ответов можно было бы перебирать клубы.
async fn invite_preview(State(state): State<ApiEnv>, Path(raw_token): Path<String>) -> Response {
    let Some(token_hash) = invite_token_hash(&raw_token) else {
        return invite_not_found();
    };
    let Some(pool) = database_pool(&state) else {
        return database_not_configured();
    };
    let row = sqlx::query_as::<_, InvitePreviewRow>(
        "SELECT c.name, c.city, c.email, COALESCE(a.contact, c.email::text) AS contact, \
                c.slug, c.invite_expires_at \
         FROM club c \
         LEFT JOIN club_application a ON a.club_id = c.id \
         WHERE c.invite_token_hash = $1 AND c.invite_expires_at > now()",
    )
    .bind(&token_hash)
    .fetch_optional(&pool)
    .await;
    let row = match row {
        Ok(Some(row)) => row,
        Ok(None) => return invite_not_found(),
        Err(error) => {
            tracing::warn!(error = %error, "club invite preview: db error");
            return api_error(ApiError::Internal, "db error");
        }
    };
    (
        StatusCode::OK,
        Json(json!({
            "club_name": row.name,
            "city": row.city,
            "email": row.email,
            "contact": row.contact,
            "slug": row.slug,
            "expires_at": row.invite_expires_at,
        })),
    )
        .into_response()
}

/// `POST /v1/club/onboarding/invite/:token/accept` — задать пароль и войти.
///
/// Пароль выбирает только владелец: он приходит в теле, хешируется Argon2id
/// и пишется в `club.password_hash`. В том же UPDATE токен зануляется —
/// поэтому ссылка одноразовая без отдельного флага, а повторный accept
/// упирается в `invite_expires_at IS NULL` и отвечает тем же 404.
///
/// `status='active'`: владение e-mail подтверждено самой ссылкой, одобрение
/// уже дал админ. Аудит `club.invite.accept` пишется в той же транзакции:
/// активация без следа в журнале недопустима.
async fn invite_accept(
    State(state): State<ApiEnv>,
    Path(raw_token): Path<String>,
    body: Result<Json<InviteAcceptRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match body {
        Ok(json) => json,
        Err(_) => {
            return validation_failed(
                "password",
                "request body must be valid JSON with a password",
            )
        }
    };
    let password = match accept_password(request.password.as_deref()) {
        Ok(password) => password,
        Err(message) => return validation_failed("password", message),
    };
    let Some(token_hash) = invite_token_hash(&raw_token) else {
        return invite_not_found();
    };
    let password_hash = match auth::hash_password(password) {
        Ok(hash) => hash,
        Err(_) => return api_error(ApiError::Internal, "password hashing failed"),
    };
    // Пул и репозиторий берутся из одного места: если БД не настроена,
    // ни активация, ни выдача токенов невозможны — 503, а не 500.
    let Some(repo) = state.repo.clone() else {
        return database_not_configured();
    };
    let pool = repo.pool();
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    let updated = sqlx::query_as::<_, InviteClubRow>(
        "UPDATE club \
         SET password_hash = $2, invite_token_hash = NULL, invite_expires_at = NULL, \
             status = 'active' \
         WHERE invite_token_hash = $1 AND invite_expires_at > now() \
         RETURNING id, name, slug, email",
    )
    .bind(&token_hash)
    .bind(&password_hash)
    .fetch_optional(&mut *tx)
    .await;
    let club = match updated {
        Ok(Some(club)) => club,
        Ok(None) => return invite_not_found(),
        Err(error) => {
            tracing::warn!(error = %error, "club invite accept: update failed");
            return api_error(ApiError::Internal, "db error");
        }
    };
    let audited = sqlx::query(
        "INSERT INTO audit_log (actor_type, actor_id, action, entity, diff) \
         VALUES ('club', $1, 'club.invite.accept', $2, $3)",
    )
    .bind(club.id)
    .bind(club.id.to_string())
    .bind(json!({ "club_id": club.id, "status": "active" }))
    .execute(&mut *tx)
    .await;
    if audited.is_err() {
        return api_error(ApiError::Internal, "db error");
    }
    if tx.commit().await.is_err() {
        return api_error(ApiError::Internal, "db error");
    }

    tracing::info!(club_id = %club.id, "club_invite_accepted");
    let tokens = match crate::routes::issue_pair_parts(&state, Role::Club, club.id, repo).await {
        Ok(tokens) => tokens,
        Err(response) => return response,
    };
    (
        StatusCode::OK,
        Json(InviteAcceptResponse {
            tokens,
            club: InviteClub {
                id: club.id,
                name: club.name,
                slug: club.slug,
                email: club.email,
            },
        }),
    )
        .into_response()
}

/// `POST /v1/admin/onboarding-applications/:id/invite` — перевыпуск ссылки.
///
/// Нужен, когда письмо потерялось или ссылка истекла: заявка уже одобрена,
/// клуб создан, но пароль не задан. Выпускается НОВЫЙ токен (старый
/// перестаёт работать сразу), новый TTL, письмо ставится в очередь.
///
/// `409 not_approved` — заявку ещё не одобрили (клуба нет);
/// `409 invite_already_accepted` — клуб уже активирован: ссылка не нужна и
/// не должна давать второй способ смены пароля. Проверка обеих веток идёт
/// под `FOR UPDATE` заявки, поэтому гонка двух перевыпусков даёт один
/// победивший токен, а не два рабочих.
async fn reissue_invite(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(raw_id): Path<String>,
) -> Response {
    if let Err(response) = require_admin_write(&ctx, &state).await {
        return response;
    }
    let Some(application_id) = parse_application_id(&raw_id) else {
        return application_not_found();
    };
    let Some(pool) = database_pool(&state) else {
        return database_not_configured();
    };
    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    let locked = sqlx::query_as::<_, LockedReview>(
        "SELECT status, club_id FROM club_application WHERE id = $1 FOR UPDATE",
    )
    .bind(application_id)
    .fetch_optional(&mut *tx)
    .await;
    let locked = match locked {
        Ok(Some(row)) => row,
        Ok(None) => return application_not_found(),
        Err(error) => {
            tracing::warn!(error = %error, "club invite reissue: lock failed");
            return api_error(ApiError::Internal, "db error");
        }
    };
    if locked.status != STATUS_APPROVED {
        return not_approved();
    }
    // APPROVED без club_id — историческая строка (или клуб удалён):
    // перевыпускать нечего, это не «ошибка клиента», а состояние заявки.
    let Some(club_id) = locked.club_id else {
        return not_approved();
    };
    let club = sqlx::query_as::<_, ReissueClubRow>(
        "SELECT name, email, invite_token_hash FROM club WHERE id = $1 FOR UPDATE",
    )
    .bind(club_id)
    .fetch_optional(&mut *tx)
    .await;
    let club = match club {
        Ok(Some(club)) => club,
        Ok(None) => return not_approved(),
        Err(error) => {
            tracing::warn!(error = %error, "club invite reissue: club lock failed");
            return api_error(ApiError::Internal, "db error");
        }
    };
    // NULL-хеш = ссылка уже использована (accept его занулил) либо клуб
    // активирован легаси-путём. Второй ссылки не выдаём.
    if club.invite_token_hash.is_none() {
        return invite_already_accepted();
    }

    let invite = issue_invite();
    let stored =
        sqlx::query("UPDATE club SET invite_token_hash = $2, invite_expires_at = $3 WHERE id = $1")
            .bind(club_id)
            .bind(auth::hash_refresh_token(&invite.token))
            .bind(invite.expires_at)
            .execute(&mut *tx)
            .await;
    if stored.is_err() {
        return api_error(ApiError::Internal, "db error");
    }
    if write_audit(
        &mut tx,
        ctx.subject_id(),
        "club_application.invite",
        application_id,
        json!({
            "application_id": application_id,
            "club_id": club_id,
            "invite_expires_at": invite.expires_at,
        }),
    )
    .await
    .is_err()
    {
        return api_error(ApiError::Internal, "db error");
    }
    if tx.commit().await.is_err() {
        return api_error(ApiError::Internal, "db error");
    }

    let mail_queued = enqueue_club_invite_mail(
        &state,
        club_id,
        &club.name,
        &club.email,
        &invite.token,
        invite.expires_at,
    )
    .await;

    tracing::info!(
        application_id = %application_id,
        club_id = %club_id,
        mail_queued,
        "club_invite_reissued"
    );
    (
        StatusCode::OK,
        Json(json!({
            "invite_url": invite_url(&invite.token),
            "invite_expires_at": invite.expires_at,
            "email": club.email,
            "mail_queued": mail_queued,
        })),
    )
        .into_response()
}

// ============================================================
// Валидация публичной заявки
// ============================================================

/// Проверяет и нормализует тело заявки. Первое нарушение — [`FieldError`].
fn validate_apply(request: ApplyRequest) -> Result<ValidatedApplication, FieldError> {
    let club = request
        .club
        .ok_or_else(|| FieldError::new("club", "club is required"))?;
    let venue = request
        .venue
        .ok_or_else(|| FieldError::new("venue", "venue is required"))?;
    let qr = request
        .qr
        .ok_or_else(|| FieldError::new("qr", "qr is required"))?;

    let club_name = required_text(club.name.as_deref(), "club.name", MAX_NAME_CHARS)?;
    let club_city = optional_text(club.city.as_deref(), "club.city", MAX_CITY_CHARS)?;
    let club_address = optional_text(club.address.as_deref(), "club.address", MAX_ADDRESS_CHARS)?;
    let website = optional_text(club.website.as_deref(), "club.website", MAX_WEBSITE_CHARS)?;
    if !website.is_empty() && !is_http_url(&website) {
        return Err(FieldError::new(
            "club.website",
            "club.website must be an http(s) URL or empty",
        ));
    }
    let contact = required_text(club.contact.as_deref(), "club.contact", MAX_CONTACT_CHARS)?;
    let genres = text_list(club.genres, "club.genres", MAX_GENRES, MAX_GENRE_CHARS)?;

    let venue_name = required_text(venue.name.as_deref(), "venue.name", MAX_NAME_CHARS)?;
    let venue_type = optional_text(
        venue.venue_type.as_deref(),
        "venue.type",
        MAX_VENUE_TYPE_CHARS,
    )?;
    let venue_address =
        optional_text(venue.address.as_deref(), "venue.address", MAX_ADDRESS_CHARS)?;
    let venue_city = optional_text(venue.city.as_deref(), "venue.city", MAX_CITY_CHARS)?;
    let venue_district = optional_text(
        venue.district.as_deref(),
        "venue.district",
        MAX_DISTRICT_CHARS,
    )?;
    let capacity = required_int(venue.capacity, "venue.capacity", 1, MAX_CAPACITY)?;
    let seats = optional_int(venue.seats, "venue.seats", 0, capacity)?;
    let area_m2 = optional_int(venue.area_m2, "venue.area_m2", 0, MAX_AREA_M2)?;

    // Цена необязательна: компактная форма `/register` её не спрашивает —
    // тариф клуб задаёт в панели после активации. Если цена пришла (визард
    // `/connect`), она проверяется по тем же границам €1..€1000.
    let price_minor = optional_int(
        qr.price_minor,
        "qr.price_minor",
        MIN_PRICE_MINOR,
        MAX_PRICE_MINOR,
    )?;
    let zones = text_list(qr.zones, "qr.zones", MAX_ZONES, MAX_ZONE_CHARS)?;

    // Город в списке — город площадки (это адрес вечеринки); клубный —
    // запасной вариант, если площадку завели без города.
    let city = if venue_city.is_empty() {
        club_city.clone()
    } else {
        venue_city.clone()
    };
    let email = contact_email(&contact);

    Ok(ValidatedApplication {
        club: json!({
            "name": club_name.clone(),
            "city": club_city,
            "address": club_address,
            "website": website,
            "contact": contact.clone(),
            "genres": genres,
        }),
        venue: json!({
            "name": venue_name,
            "type": venue_type,
            "address": venue_address,
            "city": venue_city,
            "district": venue_district,
            "capacity": capacity,
            "seats": seats,
            "area_m2": area_m2,
        }),
        qr: json!({
            "price_minor": price_minor,
            "zones": zones,
        }),
        club_name,
        city,
        contact,
        email,
        price_minor,
    })
}

/// Обязательная строка: trim, непустая, не длиннее `max` символов.
fn required_text(raw: Option<&str>, field: &'static str, max: usize) -> Result<String, FieldError> {
    let value = raw.map(str::trim).unwrap_or_default();
    if value.is_empty() {
        return Err(FieldError::new(field, format!("{field} must not be empty")));
    }
    if value.chars().count() > max {
        return Err(FieldError::new(
            field,
            format!("{field} must be at most {max} characters"),
        ));
    }
    Ok(value.to_string())
}

/// Необязательная строка: trim; пустая разрешена, длина ограничена.
fn optional_text(raw: Option<&str>, field: &'static str, max: usize) -> Result<String, FieldError> {
    let value = raw.map(str::trim).unwrap_or_default();
    if value.chars().count() > max {
        return Err(FieldError::new(
            field,
            format!("{field} must be at most {max} characters"),
        ));
    }
    Ok(value.to_string())
}

/// Список коротких строк (жанры/зоны): trim, пустые отбрасываются,
/// ограничено количество и длина элемента.
fn text_list(
    raw: Option<Vec<String>>,
    field: &'static str,
    max_items: usize,
    max_item_chars: usize,
) -> Result<Vec<String>, FieldError> {
    let items: Vec<String> = raw
        .unwrap_or_default()
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect();
    if items.len() > max_items {
        return Err(FieldError::new(
            field,
            format!("{field} must contain at most {max_items} items"),
        ));
    }
    if items
        .iter()
        .any(|item| item.chars().count() > max_item_chars)
    {
        return Err(FieldError::new(
            field,
            format!("{field} items must be at most {max_item_chars} characters"),
        ));
    }
    Ok(items)
}

/// Обязательное целое в диапазоне `min..=max`.
fn required_int(
    raw: Option<i64>,
    field: &'static str,
    min: i64,
    max: i64,
) -> Result<i64, FieldError> {
    let Some(value) = raw else {
        return Err(FieldError::new(field, format!("{field} is required")));
    };
    if !(min..=max).contains(&value) {
        return Err(FieldError::new(
            field,
            format!("{field} must be between {min} and {max}"),
        ));
    }
    Ok(value)
}

/// Необязательное целое в диапазоне `min..=max` (`None` остаётся `None`).
fn optional_int(
    raw: Option<i64>,
    field: &'static str,
    min: i64,
    max: i64,
) -> Result<Option<i64>, FieldError> {
    match raw {
        None => Ok(None),
        Some(value) if (min..=max).contains(&value) => Ok(Some(value)),
        Some(_) => Err(FieldError::new(
            field,
            format!("{field} must be between {min} and {max}"),
        )),
    }
}

/// http(s)-URL или пусто. Проверяется схема и непустой хост без пробелов:
/// полноценный URL-парсер здесь не нужен, а `ftp://…`/`club.test` — не сайт.
fn is_http_url(value: &str) -> bool {
    let Some((scheme, rest)) = value.split_once("://") else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return false;
    }
    !rest.is_empty() && !rest.starts_with('/') && !rest.chars().any(char::is_whitespace)
}

/// E-mail из `contact`, если contact им является (для колонки `email`).
fn contact_email(contact: &str) -> Option<String> {
    let candidate = contact.trim().to_lowercase();
    if validate_email(&candidate).is_ok() {
        Some(candidate)
    } else {
        None
    }
}

/// E-mail: одна `@`, непустые части, домен с точкой, без пробелов.
///
/// Дублирует (а не импортирует) `reset_routes::validate_email`: правило одно
/// и то же — адрес нигде не подтверждается, нужен лишь вменяемый формат, —
/// но чужой модуль под этот роут не реструктурируется.
fn validate_email(email: &str) -> Result<(), String> {
    if email.is_empty() {
        return Err("email must not be empty".to_string());
    }
    if email.chars().count() > 254 {
        return Err("email is too long".to_string());
    }
    if email.chars().any(char::is_whitespace) {
        return Err("email must not contain spaces".to_string());
    }
    let mut parts = email.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if local.is_empty() || domain.is_empty() || parts.next().is_some() {
        return Err("email is malformed".to_string());
    }
    if !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.') {
        return Err("email domain is malformed".to_string());
    }
    Ok(())
}

/// E-mail из тела approve: необязателен. Пустое значение = «не передан»
/// (fallback на `club_application.email`), кривой формат — сразу `422`.
/// Регистр нормализуется: CITEXT в БД сравнивает без регистра, а письмо
/// должно уйти на тот адрес, который ввёл админ.
fn optional_email(raw: Option<&str>) -> Result<Option<String>, FieldError> {
    let email = raw.map(str::trim).unwrap_or_default().to_lowercase();
    if email.is_empty() {
        return Ok(None);
    }
    if let Err(message) = validate_email(&email) {
        return Err(FieldError::new("email", message));
    }
    Ok(Some(email))
}

/// Проверка длины пароля: 8..=128 символов. `None`/пустая строка — «не
/// передан» (`Ok(None)`); сам пароль НЕ trim-ится: пробелы могут быть
/// значимой частью секрета.
fn validate_password(raw: Option<&str>) -> Result<Option<&str>, String> {
    let Some(password) = raw.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let length = password.chars().count();
    if length < MIN_PASSWORD_CHARS {
        return Err(format!(
            "password must be at least {MIN_PASSWORD_CHARS} characters"
        ));
    }
    if length > MAX_PASSWORD_CHARS {
        return Err(format!(
            "password must be at most {MAX_PASSWORD_CHARS} characters"
        ));
    }
    Ok(Some(password))
}

/// `password` в approve — deprecated и необязательный: если передан, должен
/// быть валиден (прежнее поведение), если нет — `password_hash = '!'`.
fn optional_password(raw: Option<&str>) -> Result<Option<&str>, FieldError> {
    validate_password(raw).map_err(|message| FieldError::new("password", message))
}

/// Пароль активации обязателен: без него аккаунт останется неактивным.
fn accept_password(raw: Option<&str>) -> Result<&str, String> {
    match validate_password(raw)? {
        Some(password) => Ok(password),
        None => Err("password is required".to_string()),
    }
}

/// Явный slug: нормализация в нижний регистр + `[a-z0-9-]`, 2..60, без
/// краевых и двойных дефисов. Пустая строка = «сгенерируй сам».
fn parse_slug(raw: Option<&str>) -> Result<Option<String>, FieldError> {
    let Some(slug) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let slug = slug.to_ascii_lowercase();
    let length = slug.chars().count();
    if !(2..=60).contains(&length) {
        return Err(FieldError::new("slug", "slug must be 2..60 characters"));
    }
    if slug.starts_with('-') || slug.ends_with('-') || slug.contains("--") {
        return Err(FieldError::new(
            "slug",
            "slug must not start/end with '-' or contain '--'",
        ));
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(FieldError::new(
            "slug",
            "slug may contain only lowercase latin letters, digits and '-'",
        ));
    }
    Ok(Some(slug))
}

/// Причина отказа: 1..500 после trim.
fn required_reason(raw: Option<&str>) -> Result<String, FieldError> {
    let reason = raw.map(str::trim).unwrap_or_default();
    if reason.is_empty() {
        return Err(FieldError::new("reason", "reason must not be empty"));
    }
    if reason.chars().count() > 500 {
        return Err(FieldError::new(
            "reason",
            "reason must be at most 500 characters",
        ));
    }
    Ok(reason.to_string())
}

// ============================================================
// Админский список: статус, limit, курсор
// ============================================================

/// Статус фильтра: пусто/`None` → `pending_moderation`; иначе одно из трёх
/// значений CHECK-а миграции.
fn parse_status_filter(raw: Option<&str>) -> Result<String, FieldError> {
    let value = raw
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(STATUS_PENDING);
    if matches!(value, STATUS_PENDING | STATUS_APPROVED | STATUS_REJECTED) {
        return Ok(value.to_string());
    }
    Err(FieldError::new(
        "status",
        "status must be one of pending_moderation, approved, rejected",
    ))
}

/// `limit`: по умолчанию 50, допустимо 1..=100. Мусор → `422`, а не
/// молча дефолт: иначе клиент «теряет» страницы и не понимает почему.
fn parse_limit(raw: Option<&str>) -> Result<i64, FieldError> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(DEFAULT_LIST_LIMIT);
    };
    match raw.parse::<i64>() {
        Ok(limit) if (1..=MAX_LIST_LIMIT).contains(&limit) => Ok(limit),
        _ => Err(FieldError::new(
            "limit",
            format!("limit must be between 1 and {MAX_LIST_LIMIT}"),
        )),
    }
}

/// Непрозрачный курсор: base64url(`микросекунды|uuid`).
fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> String {
    use base64::Engine as _;
    let raw = format!("{}{CURSOR_SEP}{id}", created_at.timestamp_micros());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw)
}

/// Разбирает курсор. `None` — первая страница; мусор → `422 field=cursor`
/// (молча отдавать первую страницу нельзя: пагинация зацикливается).
fn decode_cursor(raw: Option<&str>) -> Result<Option<(DateTime<Utc>, Uuid)>, FieldError> {
    use base64::Engine as _;
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let invalid = || FieldError::new("cursor", "cursor is not valid");
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| invalid())?;
    let text = String::from_utf8(bytes).map_err(|_| invalid())?;
    let (micros, id) = text.rsplit_once(CURSOR_SEP).ok_or_else(invalid)?;
    let micros = micros.parse::<i64>().map_err(|_| invalid())?;
    let created_at = DateTime::<Utc>::from_timestamp_micros(micros).ok_or_else(invalid)?;
    let id = Uuid::parse_str(id).map_err(|_| invalid())?;
    Ok(Some((created_at, id)))
}

/// Разбор id из пути: битый uuid — не `400` axum, а тот же `404`, что и
/// несуществующая заявка.
fn parse_application_id(raw: &str) -> Option<Uuid> {
    Uuid::parse_str(raw.trim()).ok()
}

// ============================================================
// Idempotency-Key
// ============================================================

/// `Idempotency-Key` из заголовков. Отсутствие/пустое значение — `None`
/// (заявка без ключа), слишком длинный — `422`. Сам ключ не секрет и не
/// интерпретируется, только хранится.
fn idempotency_key(headers: &HeaderMap) -> Result<Option<String>, FieldError> {
    let Some(raw) = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
    else {
        return Ok(None);
    };
    let key = raw.trim();
    if key.is_empty() {
        return Ok(None);
    }
    if key.chars().count() > IDEMPOTENCY_KEY_MAX_CHARS {
        return Err(FieldError::new(
            "Idempotency-Key",
            format!("Idempotency-Key must be at most {IDEMPOTENCY_KEY_MAX_CHARS} characters"),
        ));
    }
    Ok(Some(key.to_string()))
}

/// Находит заявку по ключу идемпотентности (для replay-ответа).
async fn find_by_idempotency_key(
    pool: &PgPool,
    key: &str,
) -> Result<Option<ApplyResponse>, sqlx::Error> {
    let row = sqlx::query_as::<_, (Uuid, String, DateTime<Utc>)>(
        "SELECT id, status, created_at FROM club_application WHERE idempotency_key = $1",
    )
    .bind(key)
    .fetch_optional(pool)
    .await?;
    Ok(
        row.map(|(application_id, status, submitted_at)| ApplyResponse {
            application_id,
            status,
            submitted_at,
        }),
    )
}

// ============================================================
// Счётчики антифрода
// ============================================================

/// Сколько заявок пришло с этого IP за последний час.
async fn count_by_ip(pool: &PgPool, ip: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM club_application \
         WHERE source_ip = $1::inet AND created_at > now() - interval '1 hour'",
    )
    .bind(ip)
    .fetch_one(pool)
    .await
}

/// Сколько pending-заявок уже висит на этот contact (регистр не важен).
async fn count_pending_by_contact(pool: &PgPool, contact: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM club_application \
         WHERE lower(contact) = lower($1) AND status = $2",
    )
    .bind(contact)
    .bind(STATUS_PENDING)
    .fetch_one(pool)
    .await
}

// ============================================================
// Approve: slug, master-токен, audit
// ============================================================

/// Ошибка выбора slug внутри транзакции.
enum SlugError {
    /// Все кандидаты (или явный slug) заняты.
    Taken,
    Db,
}

/// Свободен ли slug (внутри транзакции approve).
async fn slug_is_free(tx: &mut Transaction<'_, Postgres>, slug: &str) -> Result<bool, SlugError> {
    let found = sqlx::query_scalar::<_, i32>("SELECT 1 FROM club WHERE slug = $1 LIMIT 1")
        .bind(slug)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|_| SlugError::Db)?;
    Ok(found.is_none())
}

/// Явный slug — как есть (после проверки занятости); без него — транслит
/// названия клуба + первый свободный суффикс (`-2`, `-3`, …).
async fn resolve_slug(
    tx: &mut Transaction<'_, Postgres>,
    name: &str,
    explicit: Option<&str>,
) -> Result<String, SlugError> {
    if let Some(slug) = explicit {
        if slug_is_free(tx, slug).await? {
            return Ok(slug.to_string());
        }
        return Err(SlugError::Taken);
    }
    let base = crate::signup_routes::slugify(name);
    for suffix in 1..=SLUG_MAX_SUFFIX {
        let candidate = if suffix <= 1 {
            base.clone()
        } else {
            format!("{base}-{suffix}")
        };
        if slug_is_free(tx, &candidate).await? {
            return Ok(candidate);
        }
    }
    Err(SlugError::Taken)
}

/// Свежая одноразовая ссылка: 64 hex-символа, TTL 7 дней.
///
/// `auth::generate_refresh_token` уже даёт 32 случайных байта в hex — тот же
/// источник энтропии и та же sha256-схема хранения, что у refresh-токенов.
fn issue_invite() -> IssuedInvite {
    IssuedInvite {
        token: auth::generate_refresh_token(),
        expires_at: Utc::now() + Duration::days(INVITE_TTL_DAYS),
    }
}

/// Публичный URL страницы активации. База — `MAIL_APP_BASE_URL` (та же, что
/// у писем DJ/сброса пароля), путь — маршрут панели `/club/invite/:token`.
fn invite_url(token: &str) -> String {
    format!("{}/club/invite/{token}", crate::infra::mail_app_base_url())
}

/// sha256 сырого токена, если он вообще похож на invite-токен.
///
/// Битый формат не идёт в БД: 64 hex-символа — дешёвая проверка, после
/// которой запрос к индексу имеет смысл. Возврат `None` трактуется так же,
/// как «не найден» — внешне это один 404.
fn invite_token_hash(raw: &str) -> Option<String> {
    let token = raw.trim();
    if token.len() != INVITE_TOKEN_CHARS || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(auth::hash_refresh_token(token))
}

/// Ставит в очередь письмо `club_invite` (best effort).
///
/// Возвращает `true` только если строка письма реально оказалась в очереди:
/// вызывающий отдаёт это значение как `mail_queued`. Ошибка рендера,
/// парсинга адреса или БД логируется и NOT отменяет approve/перевыпуск:
/// клуб уже создан/ссылка уже работает, письмо — уведомление.
async fn enqueue_club_invite_mail(
    state: &ApiEnv,
    club_id: Uuid,
    club_name: &str,
    email: &str,
    invite_token: &str,
    expires_at: DateTime<Utc>,
) -> bool {
    let Some(mail_repo) = state.mail.as_ref() else {
        return false;
    };
    let to = match dropit_infra_mail::MailAddress::parse(email) {
        Ok(address) => address,
        Err(error) => {
            tracing::warn!(error = %error, "club invite mail: адрес не разобран");
            return false;
        }
    };
    let prefix: String = invite_token.chars().take(INVITE_KEY_PREFIX_CHARS).collect();
    crate::mail_routes::enqueue_best_effort(
        mail_repo.as_ref(),
        crate::mail_routes::MailIntent {
            template: dropit_infra_mail::TemplateKey::ClubInvite,
            to: &to,
            vars: json!({
                "club_name": club_name,
                "invite_url": invite_url(invite_token),
                "expires_at": expires_at.to_rfc3339(),
            }),
            // Префикс токена различает перевыпуски, не раскрывая ссылку:
            // ключ попадает в журнал почты и в логи воркера.
            idempotency_key: format!("club_invite:{club_id}:{prefix}"),
            aggregate_type: Some("club"),
            aggregate_id: Some(club_id),
        },
    )
    .await
    .is_some()
}

/// Master-токен входа клуба: 128 бит (32 hex). В БД — только sha256-хеш
/// (`auth::hash_refresh_token`), plaintext отдаётся один раз в ответе.
fn generate_master_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Запись в `audit_log` внутри транзакции решения.
async fn write_audit(
    tx: &mut Transaction<'_, Postgres>,
    actor_id: Uuid,
    action: &str,
    application_id: Uuid,
    diff: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO audit_log (actor_type, actor_id, action, entity, diff) \
         VALUES ('admin', $1, $2, $3, $4)",
    )
    .bind(actor_id)
    .bind(action)
    .bind(application_id.to_string())
    .bind(diff)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Строка из JSONB-payload по ключу (trim; пустая → `None`).
fn payload_text<'a>(payload: &'a Value, key: &str) -> Option<&'a str> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Имя нарушенного UNIQUE-ограничения, если ошибка — unique violation.
///
/// Внутренние тексты наружу не уходят: имя ограничения нужно только для
/// выбора стабильного бизнес-кода (`email_taken` / `slug_taken`).
fn unique_constraint(error: &sqlx::Error) -> Option<String> {
    let database_error = error.as_database_error()?;
    if !database_error.is_unique_violation() {
        return None;
    }
    database_error.constraint().map(str::to_string)
}

// ============================================================
// Unit-тесты чистых помощников (без БД)
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_url_accepts_only_web_schemes() {
        for good in [
            "https://club.test",
            "http://club.test/events",
            "HTTPS://CLUB.TEST",
        ] {
            assert!(is_http_url(good), "должен пройти: {good:?}");
        }
        for bad in [
            "",
            "club.test",
            "ftp://club.test",
            "https://",
            "https:///x",
            "https://a b",
        ] {
            assert!(!is_http_url(bad), "должен быть отвергнут: {bad:?}");
        }
    }

    #[test]
    fn slug_validation_normalizes_and_rejects_garbage() {
        assert_eq!(parse_slug(None).unwrap(), None);
        assert_eq!(parse_slug(Some("  ")).unwrap(), None);
        assert_eq!(
            parse_slug(Some(" Barrio-Latino ")).unwrap().as_deref(),
            Some("barrio-latino")
        );
        assert_eq!(
            parse_slug(Some("club-24-7")).unwrap().as_deref(),
            Some("club-24-7")
        );
        for bad in ["a", "-club", "club-", "cl--ub", "club_1", "клуб"] {
            assert!(
                parse_slug(Some(bad)).is_err(),
                "должен быть отвергнут: {bad:?}"
            );
        }
        assert!(
            parse_slug(Some(&"a".repeat(61))).is_err(),
            ">60 символов — отказ"
        );
    }

    #[test]
    fn numbers_follow_contract_bounds() {
        assert!(required_int(Some(100), "price", 100, 100_000).is_ok());
        assert!(required_int(Some(99), "price", 100, 100_000).is_err());
        assert!(required_int(Some(100_001), "price", 100, 100_000).is_err());
        assert!(required_int(None, "price", 100, 100_000).is_err());
        assert_eq!(optional_int(None, "seats", 0, 10).unwrap(), None);
        assert_eq!(optional_int(Some(0), "seats", 0, 10).unwrap(), Some(0));
        assert!(optional_int(Some(11), "seats", 0, 10).is_err());
    }

    #[test]
    fn cursor_roundtrip_and_garbage() {
        let at = DateTime::<Utc>::from_timestamp_micros(1_700_000_000_123_456).unwrap();
        let id = Uuid::now_v7();
        let encoded = encode_cursor(at, id);
        let decoded = decode_cursor(Some(&encoded)).unwrap().unwrap();
        assert_eq!(decoded.0, at);
        assert_eq!(decoded.1, id);
        assert!(decode_cursor(None).unwrap().is_none());
        assert!(decode_cursor(Some("not-base64!")).is_err());
        assert!(
            decode_cursor(Some("bm9wZQ")).is_err(),
            "без разделителя — мусор"
        );
    }

    #[test]
    fn email_detection_separates_contacts() {
        assert_eq!(
            contact_email(" Owner@Club.TEST "),
            Some("owner@club.test".to_string())
        );
        assert_eq!(contact_email("+34 600 000 000"), None);
    }

    #[test]
    fn idempotency_key_is_trimmed_and_bounded() {
        let mut headers = HeaderMap::new();
        assert_eq!(idempotency_key(&headers).unwrap(), None);
        headers.insert("idempotency-key", "  key-1  ".parse().unwrap());
        assert_eq!(idempotency_key(&headers).unwrap().as_deref(), Some("key-1"));
        headers.insert("idempotency-key", "x".repeat(129).parse().unwrap());
        assert!(idempotency_key(&headers).is_err());
    }
}
