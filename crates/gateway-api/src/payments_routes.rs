//! Роуты F5: создание PaymentIntent, подтверждение оплаты и webhook-intake.
//!
//! Контракт (техкарта Часть IV/V):
//!   * `POST /v1/requests/:id/payment` — создание PaymentIntent у провайдера
//!     (`ApiEnv.stripe`); `client_secret` отдаётся клиенту для Stripe SDK;
//!   * `POST /v1/payments/:id/confirm` — GUARD: session == active AND
//!     request == pending, иначе 409 + авто-cancel интента в БД
//!     (иначе «заплачено за expired»);
//!   * `POST /v1/stripe/webhook` — приём вебхука: сырое тело + заголовок
//!     `Stripe-Signature` → `StripeApi::verify_webhook(payload, header,
//!     tolerance=300)`. Невалидная/просроченная подпись → 400
//!     `webhook_signature_invalid`, событие не применяется и не пишется в
//!     `webhook_event`. Валидная → существующая обработка (идемпотентность
//!     по `(aggregate_id, event_type)` — правило не менялось).
//!
//! Провайдер — единственный: `ApiEnv.stripe` (`Arc<dyn StripeApi>`).
//! По умолчанию `MockStripe` (деградация без `STRIPE_API_KEY`); в проде
//! `LiveStripe` с настоящим HMAC.

use crate::payments::{intent_from_event, WEBHOOK_TOLERANCE_SECS};
use crate::routes::{api_error, ApiEnv};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use dropit_common::{ApiError, ErrorEnvelope, Role};
use dropit_infra_db::billing::CreatePaymentOutcome;
use dropit_infra_db::BillingError;
use dropit_infra_stripe::{CaptureMethod, CreateIntent, StripeError};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use uuid::Uuid;

// ============================================================
// DTO
// ============================================================

#[derive(Deserialize)]
pub struct ConfirmRequest {
    /// Способ оплаты (токен Stripe SDK `pm_…`). В мок-режиме не важен;
    /// поле оставлено контрактом.
    #[allow(dead_code)]
    pub payment_method: Option<String>,
}

#[derive(Serialize)]
pub struct ConfirmResponse {
    pub payment_id: Uuid,
    pub status: String,
}

#[derive(Serialize)]
pub struct WebhookAck {
    pub received: bool,
}

// ============================================================
// Router
// ============================================================

pub fn payments_router() -> Router<ApiEnv> {
    Router::new()
        .route("/v1/requests/:id/payment", post(create_payment_intent))
        .route("/v1/payments/:id/confirm", post(confirm_payment))
        .route("/v1/stripe/webhook", post(stripe_webhook))
}

/// Конверт ошибки с произвольным кодом/статусом (для бизнес-кодов, которых
/// нет в `ApiError`: `webhook_signature_invalid`, `stripe_unavailable`).
fn custom_error(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(ErrorEnvelope {
            code: code.to_string(),
            message: message.to_string(),
            details: None,
        }),
    )
        .into_response()
}

// ============================================================
// Create intent (гость): POST /v1/requests/:id/payment
// ============================================================

/// Создание PaymentIntent для запроса. Цена — из ДЕЙСТВУЮЩЕГО тарифа клуба
/// (Q6; дефолт 300 = 3 €). Idempotency-Key обязателен и проксируется в
/// `StripeApi::create_payment_intent` (становится `Idempotency-Key` Stripe).
/// Провайдер — `ApiEnv.stripe`: MockStripe (деградация) или LiveStripe.
#[derive(Serialize)]
struct PaymentIntentResponse {
    payment_id: Uuid,
    request_id: Uuid,
    amount_minor: i64,
    currency: String,
    /// `client_secret` от провайдера — для Stripe SDK клиента.
    client_secret: String,
    status: String,
}

async fn create_payment_intent(
    State(state): State<ApiEnv>,
    Path(request_id): Path<Uuid>,
    headers: axum::http::HeaderMap,
) -> Response {
    let billing = match state.billing.clone() {
        Some(b) => b,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    let sessions = match state.sessions.clone() {
        Some(s) => s,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    let Some(stripe) = state.stripe.clone() else {
        return custom_error(
            StatusCode::BAD_GATEWAY,
            "stripe_unavailable",
            "stripe provider not configured",
        );
    };
    // Гость: device-token.
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);
    let Some(token) = bearer else {
        return crate::rbac::auth_error_response(ApiError::Unauthorized);
    };
    let Ok(guest_device_id) = Uuid::parse_str(&token) else {
        return crate::rbac::auth_error_response(ApiError::Unauthorized);
    };
    let Some(idem_key) = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
    else {
        return api_error(ApiError::ValidationError, "Idempotency-Key header required");
    };

    // Запрос: принадлежит гостю, статус pending.
    let requests = match state.requests.clone() {
        Some(r) => r,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    let row = match requests.find_request(request_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return api_error(ApiError::NotFound, "request not found"),
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    if row.guest_device_id != guest_device_id {
        return api_error(ApiError::NotFound, "request not found");
    }
    if row.status != "pending" {
        return api_error(ApiError::Conflict, "request is not pending");
    }
    // Сессия активна.
    let session = match sessions.find_session(row.session_id).await {
        Ok(Some(s)) => s,
        _ => return api_error(ApiError::Internal, "db error"),
    };
    if session.status != "active" {
        return api_error(ApiError::SessionClosed, "session is not active");
    }

    // Цена: действующий тариф клуба (Q6) или дефолт 300 (3 €).
    let pool = state.repo.as_ref().map(|r| r.pool());
    let price_minor: i64 = match pool {
        Some(p) => sqlx::query_scalar::<_, i64>(
            "SELECT price_minor FROM pricing_policy \
             WHERE club_id = $1 AND valid_to IS NULL ORDER BY valid_from DESC LIMIT 1",
        )
        .bind(session.club_id)
        .fetch_optional(&p)
        .await
        .ok()
        .flatten()
        .unwrap_or(300),
        None => 300,
    };
    let currency = "EUR".to_string();

    // АНТИ-ДВОЙНОЕ СПИСАНИЕ (быстрая проверка без блокировки): если для
    // этого request уже есть «активный» платёж (не cancelled/refunded/
    // failed) — это replay/повтор. Возвращаем его БЕЗ вызова провайдера
    // и БЕЗ создания нового intent. Два разных Idempotency-Key на один
    // request не должны плодить два интента → двойной capture.
    if let Ok(Some(existing)) = billing.find_active_payment_by_request(request_id).await {
        return (StatusCode::OK, Json(PaymentIntentResponse {
            payment_id: existing.id,
            request_id,
            amount_minor: existing.amount_minor,
            currency: existing.currency.clone(),
            // replay: у клиента уже есть оригинальный client_secret
            // первого ответа; новый не выдаётся (и провайдер не дёргается).
            client_secret: String::new(),
            status: existing.status,
        }))
            .into_response();
    }

    // Создание PaymentIntent у провайдера (единственный Stripe-клиент).
    // Manual-capture: холд, захват — вебхуком/явным capture; ADR-004 (destination
    // charges) открыт — connected_account пока не подставляется.
    let mut metadata = std::collections::BTreeMap::new();
    metadata.insert("request_id".to_string(), request_id.to_string());
    metadata.insert("club_id".to_string(), session.club_id.to_string());
    let intent = match stripe
        .create_payment_intent(CreateIntent {
            amount_minor: price_minor,
            currency: currency.clone(),
            customer_id: None,
            connected_account: None,
            capture_method: CaptureMethod::Manual,
            metadata,
            idempotency_key: idem_key.clone(),
        })
        .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "stripe create_payment_intent failed");
            return custom_error(
                StatusCode::BAD_GATEWAY,
                "stripe_unavailable",
                "stripe create_payment_intent failed",
            );
        }
    };

    // Атомарное создание строки payment с защитой от гонки: блокировка
    // строки request (FOR UPDATE) + повторная проверка существующего
    // активного платежа ВНУТРИ TX. Если параллельный запрос успел создать
    // платёж — возвращаем его (orphaned intent у провайдера остаётся в
    // requires_capture, денег не двигает). Идемпотентность по ключу —
    // ON CONFLICT (Defect 3): ретрай тем же ключом не даёт 500.
    let commission = dropit_infra_db::CommissionParams {
        commission_bp: 3000,
        dj_share_bp: 5000,
        currency: currency.clone(),
    };
    match billing
        .create_payment_for_request(
            request_id,
            price_minor,
            &idem_key,
            &intent.id,
            &commission,
        )
        .await
    {
        Ok(CreatePaymentOutcome::Created(payment)) => (
            StatusCode::CREATED,
            Json(PaymentIntentResponse {
                payment_id: payment.id,
                request_id,
                amount_minor: payment.amount_minor,
                currency,
                // client_secret от провайдера; у MockStripe — `…_secret_mock`.
                client_secret: intent.client_secret.unwrap_or_default(),
                status: payment.status,
            }),
        )
            .into_response(),
        Ok(CreatePaymentOutcome::Existing(payment)) => (
            StatusCode::OK,
            Json(PaymentIntentResponse {
                payment_id: payment.id,
                request_id,
                amount_minor: payment.amount_minor,
                currency: payment.currency.clone(),
                // Гонку выиграл параллельный создатель — его client_secret
                // у того клиента; здесь новый не выдаётся.
                client_secret: String::new(),
                status: payment.status,
            }),
        )
            .into_response(),
        Err(BillingError::RequestNotPending) => {
            api_error(ApiError::Conflict, "request is not pending")
        }
        // Чужой Idempotency-Key: ключ уже привязан к другому request_id —
        // выдача чужого payment_id/amount_minor недопустима (W7-A, ДЕФЕКТ 2).
        Err(BillingError::IdempotencyConflict) => custom_error(
            StatusCode::CONFLICT,
            "idempotency_conflict",
            "Idempotency-Key already used by another request",
        ),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

// ============================================================
// Confirm (гость)
// ============================================================

/// Извлекает guest_device_id из Bearer — как в остальных гостевых
// эндпоинтах (`requests_routes::resolve_guest_device_id`). Поддерживает
/// два формата: (1) device-token — UUIDv7 (guest_device.id, как в
/// `/sessions/join`, `/requests`); (2) гостевой JWT (Role::Guest,
/// `sub` = guest_device_id). Чужая роль JWT → 403; мусор/отсутствие → 401.
#[allow(clippy::result_large_err)]
fn resolve_guest_device_id(
    headers: &HeaderMap,
    keys: &crate::auth::JwtKeys,
) -> Result<Uuid, Response> {
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);
    let Some(token) = bearer else {
        return Err(crate::rbac::auth_error_response(ApiError::Unauthorized));
    };
    // (1) device-token — UUIDv7 (guest_device.id).
    if let Ok(id) = Uuid::parse_str(&token) {
        return Ok(id);
    }
    // (2) JWT: подпись → роль → Guest.
    match keys.verify_access(&token) {
        Ok(claims) if claims.role == Role::Guest => Ok(claims.sub),
        Ok(_) => Err(crate::rbac::auth_error_response(ApiError::Forbidden)),
        Err(_) => Err(crate::rbac::auth_error_response(ApiError::Unauthorized)),
    }
}

/// Подтверждение оплаты гостем. GUARD по техкарте: session active +
/// request pending, иначе 409 и интент ОТМЕНЯЕТСЯ в БД (авто-cancel —
/// деньги не зависают). Захват у провайдера выполняется Stripe SDK
/// клиента (manual-capture); источник истины захвата — вебхук
/// `payment_intent.succeeded`.
///
/// Безопасность (закрытие IDOR):
/// * **Аутентификация гостя** — `device-token` и/или гостевой JWT
///   (Bearer), как в `/v1/requests`. Без токена → 401.
/// * **Владение платежом** — `payment → request → guest_device_id`
///   совпадает с субъектом. Чужой платёж → 404 `payment_not_found`
///   (существование не раскрывается).
/// * **Идемпотентность** — `Idempotency-Key` обязателен; повтор тем же
///   ключом успешного confirm возвращает 200 `processing` БЕЗ повторного
///   перехода состояния (платёж уже `processing`).
async fn confirm_payment(
    State(state): State<ApiEnv>,
    Path(payment_id): Path<Uuid>,
    headers: HeaderMap,
    Json(_req): Json<ConfirmRequest>,
) -> Response {
    let billing = match state.billing.clone() {
        Some(b) => b,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };

    // Аутентификация гостя (device-token и/или гостевой JWT).
    let guest_device_id = match resolve_guest_device_id(&headers, &state.keys) {
        Ok(id) => id,
        Err(r) => return r,
    };

    // Idempotency-Key обязателен (контракт).
    let Some(_idem_key) = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
    else {
        return api_error(ApiError::ValidationError, "Idempotency-Key header required");
    };

    // Платёж.
    let payment = match billing.find_payment(payment_id).await {
        Ok(Some(p)) => p,
        Ok(None) => return api_error(ApiError::NotFound, "payment not found"),
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };

    // Владение: payment → request → guest_device_id == субъект.
    // Чужой платёж → 404 (не раскрываем существование).
    let requests = match state.requests.clone() {
        Some(r) => r,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    let req = match requests.find_request(payment.request_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return api_error(ApiError::NotFound, "payment not found"),
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    if req.guest_device_id != guest_device_id {
        return api_error(ApiError::NotFound, "payment not found");
    }

    // Идемпотентный replay: уже processing → тот же 200, без повторного
    // перехода состояния (сетевой ретрай успешного confirm).
    if payment.status == "processing" {
        return (
            StatusCode::OK,
            Json(ConfirmResponse {
                payment_id: payment.id,
                status: payment.status,
            }),
        )
            .into_response();
    }
    // Не intent → подтвердить нельзя (captured/cancelled/failed/refunded/
    // cancelling). 409 — состояние не меняется.
    if payment.status != "intent" {
        return (
            StatusCode::CONFLICT,
            Json(ErrorEnvelope {
                code: "conflict".into(),
                message: "Payment cannot be confirmed in current state.".into(),
                details: None,
            }),
        )
            .into_response();
    }

    match billing.confirm_payment(payment_id).await {
        Ok(row) => (
            StatusCode::OK,
            Json(ConfirmResponse {
                payment_id: row.id,
                status: row.status,
            }),
        )
            .into_response(),
        // Replay: платёж уже НЕ intent (параллельный confirm успел
        // перевести intent→processing, либо терминальное состояние).
        // Это НЕ GUARD-провал — отменять подтверждённый платёж нельзя
        // (W7-A, ДЕФЕКТ 1). Перечитываем актуальный статус и отвечаем
        // им: processing → 200 replay, иное → 409 без смены состояния.
        Err(BillingError::InvalidTransition) => {
            match billing.find_payment(payment_id).await {
                Ok(Some(p)) if p.status == "processing" => (
                    StatusCode::OK,
                    Json(ConfirmResponse {
                        payment_id: p.id,
                        status: p.status,
                    }),
                )
                    .into_response(),
                Ok(Some(_p)) => (
                    StatusCode::CONFLICT,
                    Json(ErrorEnvelope {
                        code: "conflict".into(),
                        message: "Payment cannot be confirmed in current state.".into(),
                        details: None,
                    }),
                )
                    .into_response(),
                Ok(None) => api_error(ApiError::NotFound, "payment not found"),
                Err(_) => api_error(ApiError::Internal, "db error"),
            }
        }
        // GUARD сработал: session не active / request не pending. Здесь
        // платёж гарантированно был intent (иначе confirm_payment вернул
        // бы InvalidTransition выше) — отмена безопасна. begin_cancel
        // условный: применится только к intent (и processing у TTL-воркера),
        // подтверждённый/терминальный платёж не тронет.
        Err(BillingError::SessionNotActive | BillingError::RequestNotPending) => {
            let _ = billing.begin_cancel(payment_id).await;
            let (code, msg) = if matches!(
                billing.find_payment(payment_id).await,
                Ok(Some(ref p)) if p.status == "cancelling"
            ) {
                (
                    "session_paused",
                    "Session is not active; payment intent cancelled",
                )
            } else {
                (
                    "conflict",
                    "Payment cannot be confirmed in current state; cancelled",
                )
            };
            (
                StatusCode::CONFLICT,
                Json(ErrorEnvelope {
                    code: code.into(),
                    message: msg.into(),
                    details: None,
                }),
            )
                .into_response()
        }
        Err(BillingError::NotFound) => api_error(ApiError::NotFound, "payment not found"),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

// ============================================================
// Webhook-intake (Stripe)
// ============================================================

/// Приём вебхука. Сырое тело (bytes) + заголовок `Stripe-Signature` →
/// `StripeApi::verify_webhook(payload, header, tolerance=300)`.
/// Невалидная/просроченная подпись → 400 `webhook_signature_invalid`,
/// событие не применяется и не пишется в `webhook_event`. Валидная →
/// разбор `payment_intent.succeeded`/`charge.succeeded`, lookup платёжа
/// по `stripe_payment_intent_id`, затем `handle_capture_webhook`
/// (идемпотентность по `(aggregate_id, event_type)` не менялась).
async fn stripe_webhook(
    State(state): State<ApiEnv>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let billing = match state.billing.clone() {
        Some(b) => b,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    let Some(stripe) = state.stripe.clone() else {
        return custom_error(
            StatusCode::BAD_GATEWAY,
            "stripe_unavailable",
            "stripe provider not configured",
        );
    };

    // Заголовок подписи. axum/http `HeaderMap` хранит имена в нижнем
    // регистре, поэтому `stripe-signature` ловит и каноничный
    // `Stripe-Signature`.
    let sig = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    // Реальная проверка подписи: LiveStripe — HMAC-SHA256 по ключу;
    // MockStripe — принимает любой непустой заголовок (документировано).
    let event = match stripe.verify_webhook(body.as_ref(), &sig, WEBHOOK_TOLERANCE_SECS) {
        Ok(ev) => ev,
        Err(StripeError::SignatureInvalid(_) | StripeError::SignatureExpired) => {
            warn!("webhook rejected: invalid or expired signature");
            return custom_error(
                StatusCode::BAD_REQUEST,
                "webhook_signature_invalid",
                "invalid webhook signature",
            );
        }
        Err(StripeError::Decode(reason)) => {
            warn!(reason, "webhook rejected: payload not parseable");
            return custom_error(
                StatusCode::BAD_REQUEST,
                "webhook_signature_invalid",
                "invalid webhook payload",
            );
        }
        Err(e) => {
            warn!(error = %e, "webhook verify failed");
            return custom_error(
                StatusCode::BAD_REQUEST,
                "webhook_signature_invalid",
                "invalid webhook signature",
            );
        }
    };

    // Обрабатываем только события захвата (F5); failed/refund —
    // отдельные обработчики (F6) с той же идемпотентностью.
    match event.event_type.as_str() {
        "payment_intent.succeeded" | "charge.succeeded" => {}
        other => {
            debug!("webhook event {other} acknowledged (handler: F6)");
            return (StatusCode::OK, Json(WebhookAck { received: true })).into_response();
        }
    }

    // PaymentIntent id + сумма — из data.object.
    let Some((intent_id, amount_opt)) = intent_from_event(&event) else {
        return api_error(ApiError::BadRequest, "webhook missing object.id");
    };

    // Платёж по stripe_payment_intent_id (источник истины — наша запись).
    let pool = match state.repo.as_ref() {
        Some(r) => r.pool(),
        None => return api_error(ApiError::Internal, "db not configured"),
    };
    let row: Result<(Uuid, Uuid, i64), sqlx::Error> = sqlx::query_as(
        "SELECT id, request_id, amount_minor FROM payment WHERE stripe_payment_intent_id = $1",
    )
    .bind(&intent_id)
    .fetch_one(&pool)
    .await;
    let Ok((payment_id, request_id, payment_amount)) = row else {
        return api_error(ApiError::NotFound, "payment not found for intent");
    };
    // Участники (сплит) — из request/session.
    let (club_id, dj_id): (Uuid, Option<Uuid>) =
        match billing.request_participants(request_id).await {
            Ok(p) => p,
            Err(_) => return api_error(ApiError::Internal, "db error"),
        };
    let amount_minor = amount_opt.unwrap_or(payment_amount);

    match billing
        .handle_capture_webhook(
            &event.id,
            &event.event_type,
            payment_id,
            amount_minor,
            club_id,
            dj_id,
        )
        .await
    {
        // Проводка создана ИЛИ no-op-дубль — оба штатные 200
        // (дубль вебхука с другим event_id — НЕ ошибка!).
        Ok(_) => (StatusCode::OK, Json(WebhookAck { received: true })).into_response(),
        Err(BillingError::AmountMismatch) => {
            warn!("webhook amount mismatch for payment {payment_id}");
            api_error(ApiError::Conflict, "amount mismatch")
        }
        Err(BillingError::InvalidTransition) => {
            // Например failed → captured: гонка необычная, честно 409.
            api_error(ApiError::Conflict, "invalid payment transition")
        }
        Err(BillingError::NotFound) => api_error(ApiError::NotFound, "payment not found"),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}
