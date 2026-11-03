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
//!
//! Возврат средств (F6, админ): `POST /v1/admin/payments/:id/refund` —
//! деньги возвращаются у провайдера, затем ОДНОЙ транзакцией в БД
//! фиксируются `payment.refunded_minor`, пропорциональный реверс сплитов,
//! ledger-проводка `reversal` (Σ == 0), outbox `payment.refunded` и аудит.
//! Инварианты и порядок шагов — в `BillingRepo::apply_refund`.

use crate::payments::{intent_from_event, WEBHOOK_TOLERANCE_SECS};
use crate::rbac::AuthContext;
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
use dropit_infra_db::billing::{ApplyRefund, CreatePaymentOutcome, RefundRecord};
use dropit_infra_db::BillingError;
use dropit_infra_stripe::{CaptureMethod, CreateIntent, RefundReq, StripeError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, error, info, warn};
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
        .route("/v1/payments/:id/test-confirm", post(confirm_test_payment))
        .route("/v1/stripe/webhook", post(stripe_webhook))
        .route("/v1/admin/payments/:id/refund", post(admin_refund_payment))
}

/// Конверт ошибки с произвольным кодом/статусом (для бизнес-кодов, которых
/// нет в `ApiError`: `webhook_signature_invalid`, `stripe_unavailable`).
fn custom_error(status: StatusCode, code: &str, message: &str) -> Response {
    error_with_details(status, code, message, None)
}

/// Тот же конверт, но с `details` (клиенту нужен `max_refundable_minor`
/// и имя поля, чтобы показать ошибку рядом с вводом, а не общим текстом).
fn error_with_details(
    status: StatusCode,
    code: &str,
    message: &str,
    details: Option<serde_json::Value>,
) -> Response {
    (
        status,
        Json(ErrorEnvelope {
            code: code.to_string(),
            message: message.to_string(),
            details,
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
    test_mode: bool,
    payment_id: Uuid,
    request_id: Uuid,
    amount_minor: i64,
    currency: String,
    /// `client_secret` от провайдера — для Stripe SDK клиента.
    client_secret: String,
    status: String,
}

/// Called only after request ownership has been checked. A provider GET
/// recovers the original secret without creating another PaymentIntent.
async fn replay_payment_intent(
    payment: dropit_infra_db::PaymentRow,
    stripe: &dyn dropit_infra_stripe::StripeApi,
) -> Response {
    if stripe.is_mock() && !payment.stripe_payment_intent_id.as_deref().is_some_and(|id| id.starts_with("pi_mock_")) {
        return api_error(ApiError::Conflict, "payment belongs to a different provider");
    }
    let client_secret = if payment.status == "intent" {
        let Some(intent_id) = payment.stripe_payment_intent_id.as_deref() else {
            return api_error(ApiError::Internal, "payment intent missing");
        };
        match stripe.retrieve_payment_intent(intent_id).await {
            Ok(intent) => intent.client_secret.unwrap_or_default(),
            Err(_) => return custom_error(StatusCode::BAD_GATEWAY, "stripe_unavailable", "could not resume checkout"),
        }
    } else {
        String::new()
    };
    (StatusCode::OK, Json(PaymentIntentResponse {
        test_mode: stripe.is_mock(),
        payment_id: payment.id,
        request_id: payment.request_id,
        amount_minor: payment.amount_minor,
        currency: payment.currency,
        client_secret,
        status: payment.status,
    })).into_response()
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
    // failed) — это replay/повтор. Восстанавливаем исходный intent
    // БЕЗ создания нового. Два разных Idempotency-Key на один
    // request не должны плодить два интента → двойной capture.
    if let Ok(Some(existing)) = billing.find_active_payment_by_request(request_id).await {
        return replay_payment_intent(existing, stripe.as_ref()).await;
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
        .create_payment_for_request(request_id, price_minor, &idem_key, &intent.id, &commission)
        .await
    {
        Ok(CreatePaymentOutcome::Created(payment)) => (
            StatusCode::CREATED,
            Json(PaymentIntentResponse {
                test_mode: stripe.is_mock(),
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
        Ok(CreatePaymentOutcome::Existing(payment)) => replay_payment_intent(payment, stripe.as_ref()).await,
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
        Err(BillingError::InvalidTransition) => match billing.find_payment(payment_id).await {
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
        },
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

/// Same request/intent/confirmation/ledger flow, without an external charge.
/// A live provider (including Stripe sandbox keys) cannot use this endpoint.
async fn confirm_test_payment(
    State(state): State<ApiEnv>,
    Path(payment_id): Path<Uuid>,
    headers: HeaderMap,
) -> Response {
    if !state.stripe.as_ref().is_some_and(|provider| provider.is_mock()) {
        return api_error(ApiError::NotFound, "test payments are unavailable");
    }
    let guest = match resolve_guest_device_id(&headers, &state.keys) {
        Ok(guest) => guest,
        Err(error) => return error,
    };
    if headers.get("idempotency-key").and_then(|key| key.to_str().ok()).is_none_or(|key| key.trim().is_empty()) {
        return api_error(ApiError::ValidationError, "Idempotency-Key header required");
    }
    let (Some(billing), Some(requests)) = (&state.billing, &state.requests) else {
        return api_error(ApiError::ExternalServiceUnavailable, "database not configured");
    };
    let payment = match billing.find_payment(payment_id).await {
        Ok(Some(payment)) => payment,
        _ => return api_error(ApiError::NotFound, "payment not found"),
    };
    match requests.find_request(payment.request_id).await {
        Ok(Some(request)) if request.guest_device_id == guest => {},
        _ => return api_error(ApiError::NotFound, "payment not found"),
    }
    if !payment.stripe_payment_intent_id.as_deref().is_some_and(|id| id.starts_with("pi_mock_")) {
        return api_error(ApiError::Conflict, "payment is not a test intent");
    }
    if payment.status != "captured" {
        let response = confirm_payment(State(state.clone()), Path(payment_id), headers, Json(ConfirmRequest { payment_method: None })).await;
        if !response.status().is_success()
            && !matches!(billing.find_payment(payment_id).await, Ok(Some(ref current)) if current.status == "captured") {
            return response;
        }
        let (club, dj) = match billing.request_participants(payment.request_id).await {
            Ok(participants) => participants,
            Err(_) => return api_error(ApiError::Internal, "db error"),
        };
        if let Err(error) = billing.handle_capture_webhook(
            &format!("test-payment-{payment_id}"), "test.payment.succeeded",
            payment_id, payment.amount_minor, club, dj,
        ).await {
            error!(%error, %payment_id, "test payment confirmation failed");
            return api_error(ApiError::Conflict, "test payment could not be completed");
        }
    }
    (StatusCode::OK, Json(json!({ "payment_id": payment_id, "status": "captured", "test_mode": true }))).into_response()
}

/// Приём вебхука. Сырое тело (bytes) + заголовок `Stripe-Signature` →
/// `StripeApi::verify_webhook(payload, header, tolerance=300)`.
/// Невалидная/просроченная подпись → 400 `webhook_signature_invalid`,
/// событие не применяется и не пишется в `webhook_event`. Валидная →
/// разбор `payment_intent.succeeded`/`charge.succeeded`, lookup платёжа
/// по `stripe_payment_intent_id`, затем `handle_capture_webhook`
/// (идемпотентность по `(aggregate_id, event_type)` не менялась).
async fn stripe_webhook(State(state): State<ApiEnv>, headers: HeaderMap, body: Bytes) -> Response {
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

// ============================================================
// Возврат средств админом: POST /v1/admin/payments/:id/refund
// ============================================================

/// Тело возврата. Оба поля необязательны: без `amount_minor` возвращается
/// весь остаток платежа.
#[derive(Deserialize, Default)]
pub struct RefundBody {
    pub amount_minor: Option<i64>,
    /// Причина для провайдера. Допустимы три значения Stripe — см.
    /// [`REFUND_REASONS`]; прочие отклоняются 422, а не уходят в провайдер.
    pub reason: Option<String>,
}

/// Причины, которые принимает `POST /v1/refunds` у провайдера.
const REFUND_REASONS: [&str; 3] = ["duplicate", "fraudulent", "requested_by_customer"];

/// Ответ возврата.
///
/// Денежные поля намеренно названы по-разному, чтобы их нельзя было
/// перепутать:
///   * `amount_minor` — сумма ПЛАТЕЖА (как в `GET /v1/admin/payments`);
///   * `refund_amount_minor` — сумма ЭТОГО возврата;
///   * `refunded_minor` — сколько возвращено по платежу всего.
#[derive(Serialize)]
pub struct RefundResponse {
    pub payment_id: Uuid,
    pub status: String,
    pub refunded_minor: i64,
    pub refund_id: String,
    pub amount_minor: i64,
    pub refund_amount_minor: i64,
    pub currency: String,
}

fn refund_response(
    payment_amount_minor: i64,
    currency: &str,
    record: &RefundRecord,
) -> RefundResponse {
    RefundResponse {
        payment_id: record.payment_id,
        status: record.status.clone(),
        refunded_minor: record.refunded_minor,
        refund_id: record.refund_id.clone(),
        amount_minor: payment_amount_minor,
        refund_amount_minor: record.amount_minor,
        currency: currency.to_string(),
    }
}

/// Разбор тела: пустое тело = «вернуть весь остаток», невалидный JSON и
/// неизвестная причина — 422 в конверте (не text/plain от экстрактора).
#[allow(clippy::result_large_err)]
fn parse_refund_body(raw: &[u8]) -> Result<RefundBody, Response> {
    if raw.iter().all(u8::is_ascii_whitespace) {
        return Ok(RefundBody::default());
    }
    let body: RefundBody = serde_json::from_slice(raw).map_err(|_| {
        error_with_details(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_error",
            "refund body is not a valid JSON object",
            Some(json!({ "field": "body" })),
        )
    })?;
    if let Some(reason) = body.reason.as_deref() {
        if !REFUND_REASONS.contains(&reason) {
            return Err(error_with_details(
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                "reason must be one of duplicate, fraudulent, requested_by_customer",
                Some(json!({ "field": "reason", "allowed": REFUND_REASONS })),
            ));
        }
    }
    Ok(body)
}

/// `POST /v1/admin/payments/:id/refund` — возврат средств админом.
///
/// Порядок (деньги важнее удобства):
///   1. RBAC: роль Admin + `require_admin_write` (readonly → 403);
///   2. replay по `Idempotency-Key` — ДО провайдера и ДО гейта состояния:
///      повтор не возвращает деньги второй раз и не превращается в 409;
///   3. состояние: возвращать можно только `captured` /
///      `partially_refunded`, иначе 409 (`already_refunded` — отдельным
///      кодом);
///   4. сумма: по умолчанию весь остаток; `1..=остаток`, иначе 422 с
///      `details.max_refundable_minor`;
///   5. возврат у провайдера. Ошибка провайдера → 502, БД не меняется;
///   6. одна транзакция БД (`BillingRepo::apply_refund`): реверс сплитов,
///      ledger, outbox, аудит.
///
/// Окно между успешным возвратом у провайдера и записью в БД узкое, но оно
/// есть: держать блокировку строки через сетевой вызов хуже, чем возможный
/// рассинхрон по одному платежу. Если БД откажет после возврата, ответ 5xx
/// и запись в лог с `refund_id` — это сигнал на ручную сверку, а не молчание.
async fn admin_refund_payment(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(payment_id): Path<Uuid>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(response) = crate::rbac::require_admin_write(&ctx, &state).await {
        return response;
    }
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

    let body = match parse_refund_body(&body) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let idem_key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let payment = match billing.find_payment(payment_id).await {
        Ok(Some(payment)) => payment,
        Ok(None) => return api_error(ApiError::NotFound, "payment not found"),
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };

    // Replay ДО гейта состояния и ДО провайдера: повтор с тем же ключом —
    // тот же ответ. Проверять статус первым нельзя: сетевой ретрай уже
    // успешного возврата приходил бы на `refunded` и получал 409 вместо
    // того же 200.
    if let Some(key) = idem_key.as_deref() {
        match billing.find_refund_by_key(key).await {
            Ok(Some(record)) if record.payment_id == payment_id => {
                return (
                    StatusCode::OK,
                    Json(refund_response(
                        payment.amount_minor,
                        &payment.currency,
                        &record,
                    )),
                )
                    .into_response();
            }
            Ok(Some(_)) => {
                return custom_error(
                    StatusCode::CONFLICT,
                    "idempotency_conflict",
                    "Idempotency-Key already used for another payment",
                )
            }
            Ok(None) => {}
            Err(_) => return api_error(ApiError::Internal, "db error"),
        }
    }

    // Состояние: возврат — только из захваченного платежа.
    match payment.status.as_str() {
        "captured" | "partially_refunded" => {}
        "refunded" => {
            return custom_error(
                StatusCode::CONFLICT,
                "already_refunded",
                "payment is already fully refunded",
            )
        }
        _ => {
            return custom_error(
                StatusCode::CONFLICT,
                "invalid_payment_status",
                "only a captured payment can be refunded",
            )
        }
    }

    // Сумма: по умолчанию весь остаток.
    let max_refundable_minor = (payment.amount_minor - payment.refunded_minor).max(0);
    let amount_minor = body.amount_minor.unwrap_or(max_refundable_minor);
    if amount_minor <= 0 || amount_minor > max_refundable_minor {
        return error_with_details(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_refund_amount",
            "refund amount must be between 1 and the remaining amount",
            Some(json!({
                "field": "amount_minor",
                "max_refundable_minor": max_refundable_minor,
            })),
        );
    }

    // Возвращать нечего, если платёж не связан с интентом провайдера.
    let intent_id = payment
        .stripe_payment_intent_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let Some(intent_id) = intent_id else {
        return custom_error(
            StatusCode::CONFLICT,
            "no_payment_intent",
            "payment has no provider intent to refund",
        );
    };

    // Провайдер. Ключ идемпотентности прокидывается как есть: повтор с тем
    // же ключом провайдер сам схлопнет в один возврат. Без ключа клиента
    // генерируется уникальный — два независимых возврата остаются двумя.
    let stripe_key = idem_key
        .clone()
        .unwrap_or_else(|| format!("refund:{payment_id}:{}", Uuid::now_v7()));
    let refund = match stripe
        .refund(RefundReq {
            payment_intent: intent_id,
            amount_minor: Some(amount_minor),
            reason: body.reason.clone(),
            idempotency_key: stripe_key,
        })
        .await
    {
        Ok(result) => result,
        Err(failure) => {
            warn!(
                error = %failure,
                payment_id = %payment_id,
                "stripe refund failed; database untouched"
            );
            return custom_error(
                StatusCode::BAD_GATEWAY,
                "stripe_unavailable",
                "stripe refund failed",
            );
        }
    };
    // Провайдер — источник истины по деньгам, но сумму мы запросили явно;
    // расхождение означает, что вернули не то, что просили: пишем в лог
    // (сигнал на сверку) и фиксируем запрошенную сумму.
    if refund.amount_minor != amount_minor {
        error!(
            payment_id = %payment_id,
            refund_id = %refund.id,
            requested_minor = amount_minor,
            provider_minor = refund.amount_minor,
            "stripe refunded an amount different from the request"
        );
    }

    match billing
        .apply_refund(ApplyRefund {
            payment_id,
            amount_minor,
            refund_id: &refund.id,
            reason: body.reason.as_deref(),
            idempotency_key: idem_key.as_deref(),
            admin_id: ctx.subject_id(),
        })
        .await
    {
        Ok(outcome) => {
            info!(
                payment_id = %payment_id,
                refund_id = %outcome.record.refund_id,
                amount_minor = outcome.record.amount_minor,
                replayed = outcome.replayed,
                "payment refunded"
            );
            (
                StatusCode::OK,
                Json(refund_response(
                    payment.amount_minor,
                    &payment.currency,
                    &outcome.record,
                )),
            )
                .into_response()
        }
        Err(BillingError::AlreadyRefunded) => custom_error(
            StatusCode::CONFLICT,
            "already_refunded",
            "payment is already fully refunded",
        ),
        Err(BillingError::InvalidPaymentStatus) => custom_error(
            StatusCode::CONFLICT,
            "invalid_payment_status",
            "only a captured payment can be refunded",
        ),
        // Гонка: пока шёл возврат у провайдера, остаток изменился. Деньги у
        // провайдера уже вернулись — это не «ничего не произошло».
        Err(BillingError::RefundAmountOutOfRange {
            max_refundable_minor,
        }) => {
            error!(
                payment_id = %payment_id,
                refund_id = %refund.id,
                amount_minor,
                max_refundable_minor,
                "refund applied by provider but the database rejected the amount; reconcile"
            );
            error_with_details(
                StatusCode::CONFLICT,
                "refund_conflict",
                "another refund changed the payment while this one was in flight; reconcile with the provider",
                Some(json!({ "max_refundable_minor": max_refundable_minor })),
            )
        }
        Err(BillingError::IdempotencyConflict) => custom_error(
            StatusCode::CONFLICT,
            "idempotency_conflict",
            "Idempotency-Key already used for another payment",
        ),
        Err(BillingError::SplitInvariantViolated) => {
            error!(
                payment_id = %payment_id,
                refund_id = %refund.id,
                "refund applied by provider but split rows do not sum to the payment amount; reconcile"
            );
            custom_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "split_invariant_violated",
                "payment split does not sum to the payment amount",
            )
        }
        Err(BillingError::NotFound) => api_error(ApiError::NotFound, "payment not found"),
        Err(failure) => {
            error!(
                error = %failure,
                payment_id = %payment_id,
                refund_id = %refund.id,
                "refund applied by provider but the database write failed; reconcile"
            );
            api_error(ApiError::Internal, "db error")
        }
    }
}
