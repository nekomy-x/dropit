//! Роуты F6: выплаты (админ-триггер), transfer-вебхуки, сверка.
//!
//! Контракт (техкарта Часть IV, админ + вебхуки):
//!   * `POST /v1/admin/payouts/run` — ручной триггер цикла (в проде:
//!     cron пн 10:00 UTC; воркер-бинарник — F6-скелет). Перед созданием
//!     payout проверяет наличие `stripe_account_id` у каждого payee,
//!     которому будет проведена выплата (иначе 409 `stripe_account_missing`
//!     без вызова провайдера); после создания вызывает
//!     `StripeApi::create_transfer` с `idempotency_key = "payout:{id}"`
//!     (повторный запуск не создаёт второй transfer);
//!   * `POST /v1/stripe/transfer-webhook` — transfer.paid/failed,
//!     идемпотентно по stripe_transfer_id;
//!   * `POST /v1/admin/reconciliation/run` — ежедневная сверка
//!     (cron 05:00);
//!   * `GET /v1/admin/incidents` + `POST /v1/:id/resolve`;
//!   * `GET /v1/admin/payouts` — список.
//!
//! Провайдер — единственный: `ApiEnv.stripe` (`Arc<dyn StripeApi>`).

use crate::rbac::{auth_error_response, AuthContext};
use crate::routes::{api_error, ApiEnv};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use dropit_common::{ApiError, ErrorEnvelope, Role};
use dropit_domain::{decide_payout, PayoutDecision, PayoutStatus, DEFAULT_PAYOUT_THRESHOLD_MINOR};
use dropit_infra_db::{Payee, PayoutError, PayoutRepo};
use dropit_infra_stripe::{StripeError, TransferReq, WebhookEvent, WEBHOOK_TOLERANCE_SECS};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::BTreeMap;
use tracing::warn;
use uuid::Uuid;

/// Тело transfer-вебхука в legacy-формате `{event_id, type, transfer_id}`.
///
/// Поддерживается наряду со стандартным Stripe-событием
/// `{id, type, data:{object:{id}}}`; **подпись обязательна в обоих случаях**.
/// Поля опциональны — используется только для fallback-извлечения
/// `transfer_id`/`reason` после того, как событие уже проверено подписью
/// (см. [`transfer_id_from_event`]).
#[derive(Deserialize, Debug)]
pub struct TransferWebhookBody {
    /// Логируется, но НЕ является ключом идемпотентности.
    #[serde(default)]
    #[allow(dead_code)]
    pub event_id: Option<String>,
    #[serde(rename = "type", default)]
    #[allow(dead_code)]
    pub event_type: Option<String>,
    /// Идемпотентность transfer-вебхуков — по transfer_id.
    #[serde(default)]
    pub transfer_id: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Serialize)]
pub struct PayoutRunResponse {
    pub payouts_created: i64,
    #[serde(rename = "total_minor")]
    pub total_minor: i64,
}

#[derive(Serialize)]
pub struct PayoutDto {
    pub id: Uuid,
    pub payee_type: String,
    pub payee_id: Uuid,
    pub amount_minor: i64,
    pub status: String,
    pub attempts: i32,
    pub is_final: bool,
}

#[derive(Serialize)]
pub struct ReconciliationResponse {
    pub run_id: Uuid,
    pub status: String,
}

#[derive(Serialize)]
pub struct IncidentDto {
    pub id: Uuid,
    pub kind: String,
    pub message: String,
}

pub fn payouts_router() -> Router<ApiEnv> {
    Router::new()
        .route("/v1/admin/payouts/run", post(run_payouts))
        .route("/v1/admin/payouts", get(list_payouts))
        .route("/v1/admin/reconciliation/run", post(run_reconciliation))
        .route("/v1/admin/incidents", get(list_incidents))
        .route("/v1/admin/incidents/:id/resolve", post(resolve_incident))
        .route("/v1/stripe/transfer-webhook", post(transfer_webhook))
}

/// Конверт ошибки с произвольным кодом (для бизнес-кодов, которых нет в
/// `ApiError`: `stripe_account_missing`, `stripe_unavailable`).
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

/// Разрешает `stripe_account_id` payee (клуба или диджея) из БД.
/// `None` — у payee нет Connect-аккаунта (онбординг не пройден).
async fn resolve_stripe_account(
    pool: &PgPool,
    payee: &Payee,
) -> Result<Option<String>, sqlx::Error> {
    let q = match payee.payee_type {
        "club" => "SELECT stripe_account_id FROM club WHERE id = $1",
        "dj" => "SELECT stripe_account_id FROM dj WHERE id = $1",
        _ => return Ok(None),
    };
    let row: Option<(Option<String>,)> = sqlx::query_as(q)
        .bind(payee.payee_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.and_then(|(a,)| a))
}

/// Триггер цикла выплат. Админ (RBAC). В проде — cron-воркер
/// пн 10:00 UTC; финальные payees передаются при блокировке/уходе.
///
/// Шаги:
///   1. Pre-check: для каждого payee, которому БУДЕТ создана выплата
///      (`decide_payout → Send`), требуется `stripe_account_id`. Иначе
///      409 `stripe_account_missing` БЕЗ вызова провайдера.
///   2. `run_payout_cycle` создаёт in_flight payouts с
///      `stripe_transfer_id = NULL` (идентификатор провайдера ещё неизвестен).
///   3. Для каждого созданного payout — `StripeApi::create_transfer` с
///      `idempotency_key = "payout:{id}"`. Успех → `attach_transfer`
///      записывает реальный `tr_…` (статус `in_flight`; `completed` —
///      вебхук `transfer.paid` матчится по этому id); ошибка провайдера →
///      `fail_payout` (статус failed, `stripe_transfer_id` остаётся NULL).
async fn run_payouts(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let payouts = match state.payouts.clone() {
        Some(p) => p,
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
    let Some(repo) = state.repo.as_ref() else {
        return api_error(ApiError::Internal, "database not configured");
    };
    let pool = repo.pool();

    // 1. Pre-check: кто получит выплату — у тех нужен stripe_account_id.
    let net = match payouts.net_by_payee().await {
        Ok(n) => n,
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };
    // (payee_id → destination) для созданных выплат.
    let mut destinations: Vec<(Uuid, String)> = Vec::new();
    for (payee, net_minor) in &net {
        if matches!(
            decide_payout(*net_minor, DEFAULT_PAYOUT_THRESHOLD_MINOR, false),
            PayoutDecision::Send(_)
        ) {
            match resolve_stripe_account(&pool, payee).await {
                Ok(Some(acct)) => destinations.push((payee.payee_id, acct)),
                Ok(None) => {
                    return custom_error(
                        StatusCode::CONFLICT,
                        "stripe_account_missing",
                        &format!(
                            "payee {} ({}) has no stripe_account_id",
                            payee.payee_id, payee.payee_type
                        ),
                    );
                }
                Err(_) => return api_error(ApiError::Internal, "db error"),
            }
        }
    }

    // 2. Цикл: создаёт in_flight payouts.
    let rows = match payouts.run_payout_cycle(&[]).await {
        Ok(r) => r,
        Err(_) => return api_error(ApiError::Internal, "db error"),
    };

    // 3. Transfer через провайдера. Idempotency-key "payout:{payout_id}" —
    // повторный запуск того же payout не создаёт второй transfer.
    // Успех — записываем РЕАЛЬНЫЙ transfer id провайдера (attach_transfer):
    // только по нему вебхук transfer.paid сматчится со строкой выплаты.
    // Провайдер не возвращает надёжный status у Transfer (см. док
    // TransferResult.status), поэтому фиксируем in_flight — финальное
    // состояние ставит вебхук (или часовой poller).
    for row in &rows {
        let destination = destinations
            .iter()
            .find(|(id, _)| *id == row.payee_id)
            .map(|(_, a)| a.clone());
        let Some(destination) = destination else {
            // payee без аккаунта не должен был дойти до цикла — фиксируем failed.
            // transfer id ещё NULL → fail_payout по payout_id (не по transfer_id).
            if let Err(e) = payouts.fail_payout(row.id, "stripe_account_missing").await {
                warn!(error = %e, payout = %row.id, "fail_payout (missing account) failed");
            }
            continue;
        };
        let mut metadata = BTreeMap::new();
        metadata.insert("payout_id".to_string(), row.id.to_string());
        let req = TransferReq {
            amount_minor: row.amount_minor,
            currency: "EUR".to_string(),
            destination,
            metadata,
            idempotency_key: format!("payout:{}", row.id),
        };
        match stripe.create_transfer(req).await {
            Ok(res) => {
                if let Err(e) = payouts
                    .attach_transfer(row.id, &res.id, PayoutStatus::InFlight)
                    .await
                {
                    warn!(error = %e, payout = %row.id, transfer = %res.id, "attach_transfer failed");
                }
            }
            Err(e) => {
                warn!(error = %e, payout = %row.id, "stripe create_transfer failed");
                // Провайдер отказал до возврата transfer id — статус failed,
                // stripe_transfer_id остаётся NULL (фиксируем по payout_id).
                if let Err(e2) = payouts.fail_payout(row.id, "stripe transfer failed").await {
                    warn!(error = %e2, payout = %row.id, "fail_payout (provider error) failed");
                }
            }
        }
        // Успех: payout in_flight (processing) с реальным transfer id;
        // completed — вебхук transfer.paid (точка вызова провайдера — здесь).
    }

    (
        StatusCode::OK,
        Json(PayoutRunResponse {
            payouts_created: rows.len() as i64,
            total_minor: rows.iter().map(|r| r.amount_minor).sum(),
        }),
    )
        .into_response()
}

async fn list_payouts(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let payouts = match state.payouts.clone() {
        Some(p) => p,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    // Все выплаты за последнюю неделю.
    let rows = sqlx_payouts(&payouts).await;
    match rows {
        Ok(rows) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "items": rows.into_iter().map(|r| PayoutDto {
                    id: r.id,
                    payee_type: r.payee_type,
                    payee_id: r.payee_id,
                    amount_minor: r.amount_minor,
                    status: r.status,
                    attempts: r.attempts,
                    is_final: r.is_final,
                }).collect::<Vec<_>>()
            })),
        )
            .into_response(),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

async fn sqlx_payouts(p: &PayoutRepo) -> Result<Vec<dropit_infra_db::PayoutRow>, ()> {
    p.recent_payouts().await.map_err(|_| ())
}

/// transfer.paid / transfer.failed. Идемпотентно по transfer_id
/// (event_id у провайдера меняется между доставками).
///
/// **Подпись обязательна всегда**: сырое тело (`Bytes`) + заголовок
/// `Stripe-Signature` → `StripeApi::verify_webhook(payload, header,
/// tolerance=300)` — тот же путь, что у `/v1/stripe/webhook`
/// (capture-вебхук). Невалидная/просроченная/отсутствующая подпись →
/// 400 `webhook_signature_invalid`, состояние выплаты не меняется.
///
/// Формат тела — событие Stripe (`{id, type, data:{object:{id}}}`);
/// legacy-формат `{type, transfer_id}` также поддерживается (fallback
/// после проверки подписи).
async fn transfer_webhook(
    State(state): State<ApiEnv>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let payouts = match state.payouts.clone() {
        Some(p) => p,
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
    // Отказ подписи → 400, событие НЕ применяется, состояние не меняется.
    let event = match stripe.verify_webhook(body.as_ref(), &sig, WEBHOOK_TOLERANCE_SECS) {
        Ok(ev) => ev,
        Err(StripeError::SignatureInvalid(_) | StripeError::SignatureExpired) => {
            warn!("transfer webhook rejected: invalid or expired signature");
            return custom_error(
                StatusCode::BAD_REQUEST,
                "webhook_signature_invalid",
                "invalid webhook signature",
            );
        }
        Err(StripeError::Decode(reason)) => {
            warn!(reason, "transfer webhook rejected: payload not parseable");
            return custom_error(
                StatusCode::BAD_REQUEST,
                "webhook_signature_invalid",
                "invalid webhook payload",
            );
        }
        Err(e) => {
            warn!(error = %e, "transfer webhook verify failed");
            return custom_error(
                StatusCode::BAD_REQUEST,
                "webhook_signature_invalid",
                "invalid webhook signature",
            );
        }
    };

    // transfer_id — из проверенного события. Стандартный Stripe-формат:
    // data.object.id. Legacy-формат: top-level "transfer_id" (подпись
    // уже проверена — можно доверять).
    let Some(transfer_id) = transfer_id_from_event(&event, body.as_ref()) else {
        return custom_error(
            StatusCode::BAD_REQUEST,
            "webhook_signature_invalid",
            "webhook missing transfer id",
        );
    };
    let reason = reason_from_event(&event, body.as_ref());

    match event.event_type.as_str() {
        "transfer.paid" => match payouts.handle_transfer_paid(&transfer_id).await {
            // true — закрыли; false — дубль вебхука или чужой transfer_id
            // (no-op 200, чтобы Stripe не повторял доставку).
            Ok(_) => (
                StatusCode::OK,
                Json(serde_json::json!({ "received": true })),
            )
                .into_response(),
            Err(PayoutError::NotFound) => api_error(ApiError::NotFound, "transfer not found"),
            Err(_) => api_error(ApiError::Internal, "db error"),
        },
        "transfer.failed" => {
            let reason = reason.unwrap_or_else(|| "unspecified".into());
            match payouts.handle_transfer_failed(&transfer_id, &reason).await {
                Ok(row) => (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "received": true,
                        "status": row.status,
                        "attempts": row.attempts,
                    })),
                )
                    .into_response(),
                Err(PayoutError::NotFound) => api_error(ApiError::NotFound, "transfer not found"),
                Err(_) => api_error(ApiError::Internal, "db error"),
            }
        }
        other => {
            warn!("transfer webhook {other} acknowledged");
            (
                StatusCode::OK,
                Json(serde_json::json!({ "received": true })),
            )
                .into_response()
        }
    }
}

/// Извлекает `transfer_id` из проверенного подписью события.
///
/// 1. Стандартный Stripe-формат: `data.object.id` (`tr_…`).
/// 2. Legacy-формат: top-level `transfer_id` (подпись уже проверена,
///    поэтому сырой payload можно доверять). Используется для обратной
///    совместимости со старым контрактом `{event_id, type, transfer_id}`.
///
/// Возвращает `None`, если `transfer_id` отсутствует или пуст — тогда
/// маршрут отвечает 400 (подпись была валидной, но событие неполное).
fn transfer_id_from_event(event: &WebhookEvent, raw: &[u8]) -> Option<String> {
    // 1. Стандартный Stripe-формат: data.object.id
    if let Some(id) = event
        .data
        .get("object")
        .and_then(|o| o.get("id"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        return Some(id.to_string());
    }
    // 2. Legacy-формат: top-level "transfer_id"
    serde_json::from_slice::<TransferWebhookBody>(raw)
        .ok()
        .and_then(|b| b.transfer_id)
        .filter(|s| !s.is_empty())
}

/// Извлекает причину провала из проверенного события (для transfer.failed).
///
/// 1. Stripe: `data.object.failure_message` (или `failure_code`).
/// 2. Legacy: top-level `reason`.
fn reason_from_event(event: &WebhookEvent, raw: &[u8]) -> Option<String> {
    // 1. Stripe-формат: data.object.failure_message
    if let Some(msg) = event
        .data
        .get("object")
        .and_then(|o| {
            o.get("failure_message")
                .and_then(|v| v.as_str())
                .or_else(|| o.get("failure_code").and_then(|v| v.as_str()))
        })
        .filter(|s| !s.is_empty())
    {
        return Some(msg.to_string());
    }
    // 2. Legacy-формат: top-level "reason"
    serde_json::from_slice::<TransferWebhookBody>(raw)
        .ok()
        .and_then(|b| b.reason)
        .filter(|s| !s.is_empty())
}

/// Ручной триггер сверки (прод: cron 05:00).
async fn run_reconciliation(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let payouts = match state.payouts.clone() {
        Some(p) => p,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    match payouts.run_reconciliation().await {
        Ok(run_id) => {
            let status = payouts
                .run_status(run_id)
                .await
                .unwrap_or_else(|_| "unknown".into());
            (
                StatusCode::OK,
                Json(ReconciliationResponse { run_id, status }),
            )
                .into_response()
        }
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

async fn list_incidents(ctx: AuthContext, State(state): State<ApiEnv>) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let payouts = match state.payouts.clone() {
        Some(p) => p,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    match payouts.open_incidents().await {
        Ok(rows) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "items": rows.into_iter().map(|(id, kind, message)| IncidentDto {
                    id, kind, message,
                }).collect::<Vec<_>>()
            })),
        )
            .into_response(),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

async fn resolve_incident(
    ctx: AuthContext,
    State(state): State<ApiEnv>,
    Path(incident_id): Path<Uuid>,
) -> Response {
    if ctx.role() != Role::Admin {
        return auth_error_response(ApiError::Forbidden);
    }
    let payouts = match state.payouts.clone() {
        Some(p) => p,
        None => {
            return api_error(
                ApiError::ExternalServiceUnavailable,
                "database not configured",
            )
        }
    };
    match payouts
        .resolve_incident(incident_id, ctx.subject_id())
        .await
    {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({ "resolved": true })),
        )
            .into_response(),
        Ok(false) => api_error(ApiError::NotFound, "incident not found or already resolved"),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
}

/// Инвариант-чекер payee (ночная джоба; здесь — админ-эндпоинт).
#[allow(dead_code)]
async fn check_invariant(state: &ApiEnv) -> Vec<Payee> {
    match state.payouts.clone() {
        Some(p) => p.payee_invariant_violations().await.unwrap_or_default(),
        None => vec![],
    }
}
