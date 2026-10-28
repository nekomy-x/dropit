//! Почтовые роуты: вебхук доставки Resend (Svix) и админ-журнал писем.
//!
//! # Границы
//!
//! HTTP-слой ничего не отправляет. Он умеет три вещи: принять подписанный
//! вебхук провайдера, показать журнал `mail_message` и поставить письмо в
//! очередь. Фактическая отправка — задача `mail` воркера `dropit-workers`
//! (ADR-008): провайдерский таймаут не имеет права задерживать ответ API.
//!
//! # Вебхук
//!
//! `POST /webhooks/resend` — публичный роут без Bearer-токена. Доверие даёт
//! только Svix-подпись над СЫРЫМ телом: тело разбирается после проверки
//! подписи, иначе подделанный `email.delivered` мог бы «подтвердить» письмо,
//! которого не было. Без `RESEND_WEBHOOK_SECRET` роут отвечает `503`, а не
//! принимает неподписанное: «не настроено» не должно означать «доверяем всем».
//!
//! # Админ-роуты
//!
//! Все `/v1/admin/mail*` — только роль `Admin` (гейт в `routes::router`).
//! Порядок регистрации важен: `/v1/admin/mail/stats` и
//! `/v1/admin/mail/suppressions` объявлены ДО `/v1/admin/mail/:id`, иначе
//! axum разобрал бы `stats` как идентификатор.

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use dropit_common::{ApiError, ErrorEnvelope};
use dropit_infra_db::mail::{MailFilter, MailRepo};
use dropit_infra_mail::{
    parse_event, render, webhook_verifier_from_env, MailAddress, TemplateKey, WebhookHeaders,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::rbac::AuthContext;
use crate::routes::{api_error, ApiEnv};

/// Максимальный размер тела вебхука: события Resend — единицы килобайт,
/// мегабайт с запасом покрывает любые вложения в метаданных и при этом
/// ограничивает память на публичном роуте.
pub const WEBHOOK_MAX_BODY_BYTES: usize = 1024 * 1024;

/// Предел страницы журнала писем.
const MAIL_PAGE_MAX: i64 = 200;
/// Дефолт страницы журнала писем.
const MAIL_PAGE_DEFAULT: i64 = 50;

/// Роутер админ-журнала писем (монтируется под гейтом роли `Admin`).
pub fn admin_mail_router() -> Router<ApiEnv> {
    Router::new()
        .route("/v1/admin/mail", get(admin_mail_list))
        // ВАЖНО: статические сегменты раньше `:id`.
        .route("/v1/admin/mail/stats", get(admin_mail_stats))
        .route("/v1/admin/mail/suppressions", get(admin_mail_suppressions))
        .route(
            "/v1/admin/mail/suppressions/:email",
            delete(admin_mail_suppression_delete),
        )
        .route("/v1/admin/mail/test", post(admin_mail_test))
        .route("/v1/admin/mail/:id", get(admin_mail_detail))
        .route("/v1/admin/mail/:id/resend", post(admin_mail_resend))
}

/// Публичный роутер вебхука доставки.
pub fn webhook_router() -> Router<ApiEnv> {
    Router::new()
        .route("/webhooks/resend", post(resend_webhook))
        .layer(axum::extract::DefaultBodyLimit::max(WEBHOOK_MAX_BODY_BYTES))
}

// ============================================================
// Вебхук Resend
// ============================================================

/// `POST /webhooks/resend` — приём события доставки.
///
/// Порядок: секрет → подпись → разбор тела → запись события → переход
/// состояния. Любая ошибка проверки — `400 mail_webhook_invalid`, отсутствие
/// секрета — `503 mail_webhook_disabled`. Повтор известного `svix-id`
/// отвечает `200` с `duplicate: true` и ничего не меняет.
async fn resend_webhook(State(state): State<ApiEnv>, headers: HeaderMap, body: Bytes) -> Response {
    let verifier = match webhook_verifier_from_env() {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "mail webhook: секрет не настроен, событие отклонено");
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "mail_webhook_disabled",
                "Resend webhook is not configured",
                json!({}),
            );
        }
    };

    let Some(webhook_headers) = WebhookHeaders::from_pairs(header_pairs(&headers)) else {
        return error_response(
            StatusCode::BAD_REQUEST,
            "mail_webhook_invalid",
            "Missing svix-id/svix-timestamp/svix-signature headers",
            json!({}),
        );
    };

    if let Err(e) = verifier.verify(&body, &webhook_headers, chrono::Utc::now().timestamp()) {
        tracing::warn!(error = %e, "mail webhook: подпись не принята");
        return error_response(
            StatusCode::BAD_REQUEST,
            "mail_webhook_invalid",
            "Webhook signature verification failed",
            json!({}),
        );
    }

    let event = match parse_event(&body) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, "mail webhook: тело не разобрано");
            return error_response(
                StatusCode::BAD_REQUEST,
                "mail_webhook_invalid",
                "Webhook payload is not a Resend event",
                json!({}),
            );
        }
    };

    let Some(repo) = state.mail.as_ref() else {
        // Подпись верна, но журнала нет — принять событие некуда. 503, чтобы
        // провайдер повторил, а не потерял его навсегда.
        return api_error(
            ApiError::ExternalServiceUnavailable,
            "database not configured",
        );
    };

    let raw: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let is_new = match repo
        .record_event(
            &webhook_headers.id,
            event.kind.as_str(),
            event.provider_id.as_deref(),
            &raw,
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "mail webhook: не записать событие");
            return api_error(ApiError::Internal, "db error");
        }
    };

    if !is_new {
        return (
            StatusCode::OK,
            Json(json!({ "received": true, "duplicate": true })),
        )
            .into_response();
    }

    // Переход состояния — только при наличии provider_id: без него событие
    // нельзя связать с письмом (оно остаётся в журнале как есть).
    if let Some(provider_id) = event.provider_id.as_deref() {
        if let Err(e) = repo.apply_event(provider_id, event.kind.as_str()).await {
            tracing::warn!(error = %e, "mail webhook: не применить переход состояния");
        }
        // Bounce/complaint — адрес больше не должен получать письма.
        if matches!(
            event.kind,
            dropit_infra_mail::MailEventKind::Bounced
                | dropit_infra_mail::MailEventKind::Complained
        ) {
            if let Some(email) = event.email.as_deref() {
                let reason = if matches!(event.kind, dropit_infra_mail::MailEventKind::Bounced) {
                    "bounce"
                } else {
                    "complaint"
                };
                if let Err(e) = repo
                    .add_suppression(email, reason, event.reason.as_deref())
                    .await
                {
                    tracing::warn!(error = %e, "mail webhook: не добавить подавление");
                }
            }
        }
    }

    (
        StatusCode::OK,
        Json(json!({ "received": true, "duplicate": false })),
    )
        .into_response()
}

/// Пары заголовков, которые ищет [`WebhookHeaders::from_pairs`].
///
/// Имена приводятся к нижнему регистру: `HeaderMap` регистронезависим, но
/// провайдер может присылать `Svix-Id` в любом регистре.
fn header_pairs(headers: &HeaderMap) -> impl Iterator<Item = (&str, &str)> {
    headers.iter().filter_map(|(name, value)| {
        let name = name.as_str();
        // `from_pairs` ждёт именно эти три имени; остальные не нужны.
        if matches!(
            name,
            "svix-id"
                | "svix-timestamp"
                | "svix-signature"
                | "Svix-Id"
                | "Svix-Timestamp"
                | "Svix-Signature"
        ) {
            value.to_str().ok().map(|v| (name, v))
        } else {
            None
        }
    })
}

// ============================================================
// Админ: журнал
// ============================================================

#[derive(Deserialize)]
pub struct MailListQuery {
    pub status: Option<String>,
    pub q: Option<String>,
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}

#[derive(Deserialize)]
pub struct MailStatsQuery {
    pub window_hours: Option<i64>,
}

#[derive(Deserialize)]
pub struct MailLimitQuery {
    pub limit: Option<i64>,
}

/// Допустимые значения `status` — ровно стейт-машина письма.
const MAIL_STATUSES: &[&str] = &[
    "queued",
    "sending",
    "sent",
    "delivered",
    "delivery_delayed",
    "bounced",
    "complained",
    "failed",
    "suppressed",
];

/// `GET /v1/admin/mail` — страница журнала писем.
async fn admin_mail_list(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Query(query): Query<MailListQuery>,
) -> Response {
    if ctx.role() != dropit_common::Role::Admin {
        return api_error(ApiError::Forbidden, "admin only");
    }
    let Some(repo) = state.mail.as_ref() else {
        return api_error(
            ApiError::ExternalServiceUnavailable,
            "database not configured",
        );
    };

    if let Some(status) = query.status.as_deref() {
        if !MAIL_STATUSES.contains(&status) {
            return error_response(
                StatusCode::BAD_REQUEST,
                "invalid_status",
                "Unknown mail status",
                json!({ "allowed": MAIL_STATUSES }),
            );
        }
    }

    let limit = query
        .limit
        .unwrap_or(MAIL_PAGE_DEFAULT)
        .clamp(1, MAIL_PAGE_MAX);
    let filter = MailFilter {
        status: query.status,
        query: query.q.filter(|s| !s.trim().is_empty()),
        limit,
        cursor: query.cursor.filter(|s| !s.is_empty()),
    };

    match repo.list(filter).await {
        Ok((items, next_cursor)) => {
            let items: Vec<Value> = items.iter().map(mail_list_item_json).collect();
            (
                StatusCode::OK,
                Json(json!({ "items": items, "next_cursor": next_cursor })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::warn!(error = %e, "admin mail: list failed");
            api_error(ApiError::Internal, "db error")
        }
    }
}

/// `GET /v1/admin/mail/stats` — счётчики по статусам за окно.
async fn admin_mail_stats(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Query(query): Query<MailStatsQuery>,
) -> Response {
    if ctx.role() != dropit_common::Role::Admin {
        return api_error(ApiError::Forbidden, "admin only");
    }
    let Some(repo) = state.mail.as_ref() else {
        return api_error(
            ApiError::ExternalServiceUnavailable,
            "database not configured",
        );
    };
    let window = query.window_hours.unwrap_or(24).clamp(1, 720);
    match repo.stats(window).await {
        Ok(s) => (StatusCode::OK, Json(json!(s))).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "admin mail: stats failed");
            api_error(ApiError::Internal, "db error")
        }
    }
}

/// `GET /v1/admin/mail/{id}` — письмо целиком + события доставки.
async fn admin_mail_detail(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(id): Path<String>,
) -> Response {
    if ctx.role() != dropit_common::Role::Admin {
        return api_error(ApiError::Forbidden, "admin only");
    }
    let Some(repo) = state.mail.as_ref() else {
        return api_error(
            ApiError::ExternalServiceUnavailable,
            "database not configured",
        );
    };
    let Ok(id) = Uuid::parse_str(&id) else {
        return mail_not_found();
    };
    match repo.get(id).await {
        Ok(Some(detail)) => {
            let mut body = mail_item_json(&detail.mail);
            body["html"] = Value::String(detail.mail.html.clone());
            body["text"] = Value::String(detail.mail.text.clone());
            body["vars"] = detail.mail.vars.clone();
            body["events"] = Value::Array(
                detail
                    .events
                    .iter()
                    .map(|e| {
                        json!({
                            "id": e.id,
                            "type": e.event_type,
                            "received_at": e.received_at,
                        })
                    })
                    .collect(),
            );
            (StatusCode::OK, Json(body)).into_response()
        }
        Ok(None) => mail_not_found(),
        Err(e) => {
            tracing::warn!(error = %e, "admin mail: detail failed");
            api_error(ApiError::Internal, "db error")
        }
    }
}

/// `POST /v1/admin/mail/{id}/resend` — поставить письмо в очередь заново.
///
/// Создаётся НОВАЯ строка с ключом `<original>:r<n>`: исходная остаётся
/// историей доставки. Иначе «переотправка» переписала бы прошлое.
async fn admin_mail_resend(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(id): Path<String>,
) -> Response {
    if ctx.role() != dropit_common::Role::Admin {
        return api_error(ApiError::Forbidden, "admin only");
    }
    let Some(repo) = state.mail.as_ref() else {
        return api_error(
            ApiError::ExternalServiceUnavailable,
            "database not configured",
        );
    };
    let Ok(id) = Uuid::parse_str(&id) else {
        return mail_not_found();
    };
    match repo.requeue(id).await {
        Ok(new_id) => (
            StatusCode::OK,
            Json(json!({ "id": new_id, "status": "queued" })),
        )
            .into_response(),
        Err(dropit_infra_db::mail::MailDbError::NotFound) => mail_not_found(),
        Err(e) => {
            tracing::warn!(error = %e, "admin mail: requeue failed");
            api_error(ApiError::Internal, "db error")
        }
    }
}

/// `GET /v1/admin/mail/suppressions` — адреса, на которые не отправляем.
async fn admin_mail_suppressions(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Query(query): Query<MailLimitQuery>,
) -> Response {
    if ctx.role() != dropit_common::Role::Admin {
        return api_error(ApiError::Forbidden, "admin only");
    }
    let Some(repo) = state.mail.as_ref() else {
        return api_error(
            ApiError::ExternalServiceUnavailable,
            "database not configured",
        );
    };
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    match repo.list_suppressions(limit).await {
        Ok(items) => {
            let items: Vec<Value> = items
                .iter()
                .map(|s| {
                    json!({
                        "email": s.email,
                        "reason": s.reason,
                        "detail": s.detail,
                        "created_at": s.created_at,
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "items": items }))).into_response()
        }
        Err(e) => {
            tracing::warn!(error = %e, "admin mail: suppressions failed");
            api_error(ApiError::Internal, "db error")
        }
    }
}

/// `DELETE /v1/admin/mail/suppressions/{email}` — снять подавление вручную.
async fn admin_mail_suppression_delete(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(email): Path<String>,
) -> Response {
    if ctx.role() != dropit_common::Role::Admin {
        return api_error(ApiError::Forbidden, "admin only");
    }
    let Some(repo) = state.mail.as_ref() else {
        return api_error(
            ApiError::ExternalServiceUnavailable,
            "database not configured",
        );
    };
    match repo.remove_suppression(&email).await {
        Ok(removed) => (StatusCode::OK, Json(json!({ "removed": removed }))).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "admin mail: remove suppression failed");
            api_error(ApiError::Internal, "db error")
        }
    }
}

#[derive(Deserialize)]
pub struct MailTestRequest {
    pub to: String,
}

/// `POST /v1/admin/mail/test` — тестовое письмо (проверка ключа и домена).
async fn admin_mail_test(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Json(req): Json<MailTestRequest>,
) -> Response {
    if ctx.role() != dropit_common::Role::Admin {
        return api_error(ApiError::Forbidden, "admin only");
    }
    let Some(repo) = state.mail.as_ref() else {
        return api_error(
            ApiError::ExternalServiceUnavailable,
            "database not configured",
        );
    };

    let to = match MailAddress::parse(req.to.trim()) {
        Ok(addr) if addr.email.contains('@') => addr,
        _ => {
            return error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "mail_validation_failed",
                "Invalid recipient address",
                json!({}),
            )
        }
    };

    let vars = json!({
        "sent_by": ctx.subject_id().to_string(),
    });
    let rendered = match render(TemplateKey::MailTest, &vars) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "admin mail: шаблон не отрисован");
            return api_error(ApiError::Internal, "template render failed");
        }
    };

    let idempotency_key = format!("mail_test:{}", Uuid::now_v7());
    let id = match repo
        .enqueue(dropit_infra_db::mail::NewMail {
            idempotency_key,
            template: TemplateKey::MailTest.as_str().to_string(),
            to_email: to.email.clone(),
            to_name: to.name.clone(),
            from_email: crate::infra::mail_from(),
            reply_to: crate::infra::mail_reply_to(),
            subject: rendered.subject,
            html: rendered.html,
            text: rendered.text,
            vars,
            aggregate_type: Some("admin_user".to_string()),
            aggregate_id: Some(ctx.subject_id()),
        })
        .await
    {
        Ok((id, _duplicate)) => id,
        Err(e) => {
            tracing::warn!(error = %e, "admin mail: enqueue failed");
            return api_error(ApiError::Internal, "db error");
        }
    };

    (
        StatusCode::OK,
        Json(json!({ "id": id, "status": "queued" })),
    )
        .into_response()
}

// ============================================================
// Хелперы
// ============================================================

/// JSON-представление строки списка (без тела письма).
fn mail_list_item_json(row: &dropit_infra_db::mail::MailListItem) -> Value {
    json!({
        "id": row.id,
        "to": row.to_email,
        "to_name": row.to_name,
        "subject": row.subject,
        "template": row.template,
        "status": row.status,
        "attempts": row.attempts,
        "max_attempts": row.max_attempts,
        "provider_id": row.provider_id,
        "last_error": row.last_error,
        "aggregate_type": row.aggregate_type,
        "aggregate_id": row.aggregate_id,
        "created_at": row.created_at,
        "sent_at": row.sent_at,
        "delivered_at": row.delivered_at,
    })
}

/// JSON-представление письма целиком (без тела — тело добавляет деталь).
fn mail_item_json(row: &dropit_infra_db::mail::MailRow) -> Value {
    json!({
        "id": row.id,
        "to": row.to_email,
        "to_name": row.to_name,
        "subject": row.subject,
        "template": row.template,
        "status": row.status,
        "attempts": row.attempts,
        "max_attempts": row.max_attempts,
        "provider_id": row.provider_id,
        "last_error": row.last_error,
        "aggregate_type": row.aggregate_type,
        "aggregate_id": row.aggregate_id,
        "created_at": row.created_at,
        "sent_at": row.sent_at,
        "delivered_at": row.delivered_at,
    })
}

/// 404 для отсутствующего письма: код `mail_not_found` (его ждут панель и
/// тесты; общий `not_found` не говорит клиенту, чего именно нет).
fn mail_not_found() -> Response {
    error_response(
        StatusCode::NOT_FOUND,
        "mail_not_found",
        "Mail message not found",
        json!({}),
    )
}

/// Конверт ошибки с бизнес-кодом, которого нет в [`ApiError`].
#[allow(clippy::result_large_err)]
fn error_response(status: StatusCode, code: &str, message: &str, details: Value) -> Response {
    (
        status,
        Json(ErrorEnvelope {
            code: code.to_string(),
            message: message.to_string(),
            details: Some(details),
        }),
    )
        .into_response()
}

/// Письмо, которое бизнес-событие просит поставить в очередь.
///
/// Отдельная структура, а не восемь аргументов: у вызова появляются имена
/// полей, и перепутать `aggregate_type` с `aggregate_id` больше нельзя.
pub struct MailIntent<'a> {
    /// Шаблон письма.
    pub template: TemplateKey,
    /// Получатель.
    pub to: &'a MailAddress,
    /// Переменные шаблона.
    pub vars: Value,
    /// Ключ идемпотентности (`dj_invite:<id>`, …).
    pub idempotency_key: String,
    /// Тип бизнес-сущности-источника (`dj`, `guest_account`, `payout`).
    pub aggregate_type: Option<&'a str>,
    /// Идентификатор бизнес-сущности-источника.
    pub aggregate_id: Option<Uuid>,
}

/// Поставить письмо в очередь из бизнес-обработчика.
///
/// Ошибка почты НИКОГДА не должна валить бизнес-операцию: инвайт DJ или
/// апгрейд гостя важнее письма. Поэтому результат только логируется, а
/// вызывающий продолжает работу. Возвращает `Some(id)` при успехе — это
/// удобно для тестов, которые проверяют, что письмо действительно встало.
pub async fn enqueue_best_effort(repo: &MailRepo, intent: MailIntent<'_>) -> Option<Uuid> {
    let MailIntent {
        template,
        to,
        vars,
        idempotency_key,
        aggregate_type,
        aggregate_id,
    } = intent;

    let rendered = match render(template, &vars) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, template = template.as_str(), "mail: шаблон не отрисован");
            return None;
        }
    };
    let mail = dropit_infra_db::mail::NewMail {
        idempotency_key,
        template: template.as_str().to_string(),
        to_email: to.email.clone(),
        to_name: to.name.clone(),
        from_email: crate::infra::mail_from(),
        reply_to: crate::infra::mail_reply_to(),
        subject: rendered.subject,
        html: rendered.html,
        text: rendered.text,
        vars,
        aggregate_type: aggregate_type.map(str::to_string),
        aggregate_id,
    };
    match repo.enqueue(mail).await {
        Ok((id, duplicate)) => {
            if duplicate {
                tracing::debug!(mail_id = %id, "mail: письмо уже в очереди (идемпотентно)");
            }
            Some(id)
        }
        Err(e) => {
            tracing::warn!(error = %e, template = template.as_str(), "mail: не удалось поставить в очередь");
            None
        }
    }
}
