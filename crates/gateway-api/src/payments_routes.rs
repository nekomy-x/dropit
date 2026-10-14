//! Роуты F5: подтверждение оплаты и webhook-intake.
//!
//! Контракт (техкарта Часть IV/V):
//!   * `POST /v1/payments/:id/confirm` — GUARD: session == active
//!     AND request == pending, иначе 409 + авто-cancel интента
//!     (иначе «заплачено за expired»);
//!   * `POST /v1/stripe/webhook` — приём вебхука: верификация
//!     подписи (tolerance 5 мин, clock-drift — отдельный лог),
//!     идемпотентность по (aggregate_id, event_type); дубль
//!     доставки — no-op 200.
//!
//! Провайдер — мок (правило 5): прод-Stripe после ADR-004/Q15.

use crate::payments::{
    verify_webhook_signature, MockStripe, PaymentProvider, WebhookSignatureError,
};
use crate::routes::{api_error, ApiEnv};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use dropit_common::ApiError;
use dropit_infra_db::BillingError;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use uuid::Uuid;

// ============================================================
// DTO
// ============================================================

#[derive(Deserialize)]
pub struct ConfirmRequest {
    /// Мок-режим: способ оплаты не важен; реальный — токен
    /// Stripe SDK (payment_method). Поле оставлено контрактом.
    #[allow(dead_code)]
    pub payment_method: Option<String>,
}

#[derive(Serialize)]
pub struct ConfirmResponse {
    pub payment_id: Uuid,
    pub status: String,
}

#[derive(Deserialize, Debug)]
pub struct WebhookBody {
    pub event_id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    /// Платёж (наш payment.id).
    pub payment_id: Uuid,
    pub amount_minor: i64,
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

// ============================================================
// Create intent (гость): POST /v1/requests/:id/payment
// ============================================================

/// Создание платёж-интента для запроса. Цена — из ДЕЙСТВУЮЩЕГО
/// тарифа клуба (Q6; комиссии платформы по умолчанию 30%/50-50
/// ДО platform-settings). Idempotency-Key обязателен.
/// Провайдер — мок (Test Mode): intent_id детерминирован.
#[derive(serde::Serialize)]
struct PaymentIntentResponse {
    payment_id: Uuid,
    request_id: Uuid,
    amount_minor: i64,
    currency: String,
    /// Мок-провайдер: «client_secret» для «Stripe SDK» клиента.
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

    // Создание payment (мок intent_id стабилен по ключу).
    let intent_id = format!("pi_mock_{}", idem_key);
    match billing
        .create_payment(
            request_id,
            price_minor,
            &idem_key,
            &intent_id,
            &dropit_infra_db::CommissionParams {
                commission_bp: 3000,
                dj_share_bp: 5000,
                currency: "EUR".into(),
            },
        )
        .await
    {
        Ok(payment) => (
            StatusCode::CREATED,
            Json(PaymentIntentResponse {
                payment_id: payment.id,
                request_id,
                amount_minor: payment.amount_minor,
                currency: "EUR".into(),
                client_secret: format!(
                    "{}__mock_secret",
                    payment.stripe_payment_intent_id.clone().unwrap_or_default()
                ),
                status: payment.status,
            }),
        )
            .into_response(),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

// ============================================================
// Confirm (гость)
// ============================================================

/// GUARD по техкарте: session active + request pending, иначе 409
/// и интент ОТМЕНЯЕТСЯ (авто-cancel — деньги не зависают).
async fn confirm_payment(
    State(state): State<ApiEnv>,
    Path(payment_id): Path<Uuid>,
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
    let payment = match billing.find_payment(payment_id).await {
        Ok(Some(p)) => p,
        Ok(None) => return api_error(ApiError::NotFound, "payment not found"),
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };

    // МОК-ПРОВАЙДЕР (правило 5): реальный вызов Stripe — после ADR-004.
    let provider = MockStripe;
    let intent = payment.stripe_payment_intent_id.clone().unwrap_or_default();

    match billing.confirm_payment(payment_id).await {
        Ok(row) => {
            // Провайдер подтверждает интент → processing.
            let _ = provider.confirm_intent(&intent);
            (
                StatusCode::OK,
                Json(ConfirmResponse {
                    payment_id: row.id,
                    status: row.status,
                }),
            )
                .into_response()
        }
        // GUARD сработал: пауза/закрытие/expired → 409 + cancel.
        Err(
            BillingError::SessionNotActive
            | BillingError::RequestNotPending
            | BillingError::InvalidTransition,
        ) => {
            // Авто-отмена интента У ПРОВАЙДЕРА (не локальная метка).
            let _ = provider.cancel_intent(&intent);
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
                Json(dropit_common::ErrorEnvelope {
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

/// Приём вебхука. Валидация подписи → дедуп доставки (event_id) →
/// semantic-идемпотентность (aggregate_id, event_type) →
/// payment→captured + ledger (одна TX в репо).
async fn stripe_webhook(State(state): State<ApiEnv>, headers: HeaderMap, body: String) -> Response {
    let billing = match state.billing.clone() {
        Some(b) => b,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };

    // Подпись: Stripe-Signature header.
    let sig = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let now = chrono::Utc::now().timestamp();
    let secret = std::env::var("DROPIT_STRIPE_WEBHOOK_SECRET")
        .unwrap_or_else(|_| "whsec_mock_dev_only".into());
    match verify_webhook_signature(&body, &sig, secret.as_bytes(), now) {
        Ok(()) => {}
        // Clock-drift — ОТДЕЛЬНЫЙ лог (не смешивается с подделкой,
        // техкарта VII: отказ из-за timestamp ≠ отказ подписи).
        Err(WebhookSignatureError::StaleTimestamp) => {
            warn!("webhook rejected: timestamp outside tolerance (clock drift?)");
            return api_error(ApiError::BadRequest, "webhook timestamp stale");
        }
        Err(WebhookSignatureError::Invalid) => {
            warn!("webhook rejected: invalid signature");
            return api_error(ApiError::BadRequest, "invalid webhook signature");
        }
    }

    // Парсинг.
    let Ok(event) = serde_json::from_str::<WebhookBody>(&body) else {
        return api_error(ApiError::BadRequest, "malformed webhook payload");
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

    // Платёж + участники (сплит) — из request/session.
    let payment = match billing.find_payment(event.payment_id).await {
        Ok(Some(p)) => p,
        _ => return api_error(ApiError::NotFound, "payment not found"),
    };
    let (club_id, dj_id): (Uuid, Option<Uuid>) =
        match billing.request_participants(payment.request_id).await {
            Ok(p) => p,
            Err(_) => return api_error(ApiError::Internal, "db error"),
        };

    match billing
        .handle_capture_webhook(
            &event.event_id,
            &event.event_type,
            event.payment_id,
            event.amount_minor,
            club_id,
            dj_id,
        )
        .await
    {
        // Проводка создана ИЛИ no-op-дубль — оба штатные 200
        // (дубль вебхука с другим event_id — НЕ ошибка!).
        Ok(_) => (StatusCode::OK, Json(WebhookAck { received: true })).into_response(),
        Err(BillingError::AmountMismatch) => {
            warn!("webhook amount mismatch for payment {}", event.payment_id);
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
