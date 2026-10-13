//! Роуты F6: выплаты (админ-триггер), transfer-вебхуки, сверка.
//!
//! Контракт (техкарта Часть IV, админ + вебхуки):
//!   * `POST /v1/admin/payouts/run` — ручной триггер цикла (в проде:
//!     cron пн 10:00 UTC; воркер-бинарник — F6-скелет);
//!   * `POST /v1/stripe/transfer-webhook` — transfer.paid/failed,
//!     идемпотентно по stripe_transfer_id;
//!   * `POST /v1/admin/reconciliation/run` — ежедневная сверка
//!     (cron 05:00);
//!   * `GET /v1/admin/incidents` + `POST /v1/:id/resolve`;
//!   * `GET /v1/admin/payouts` — список.
//!
//! Провайдер — МОК (правило 5): transfer_id присваивается в репо.

use crate::rbac::{auth_error_response, AuthContext};
use crate::routes::{api_error, ApiEnv};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use dropit_common::{ApiError, Role};
use dropit_infra_db::{Payee, PayoutRepo};
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

#[derive(Deserialize, Debug)]
pub struct TransferWebhookBody {
    /// Логируется, но НЕ является ключом идемпотентности:
    /// transfer-вебхуки дедуплицируются по transfer_id (техкарта).
    #[allow(dead_code)]
    pub event_id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    /// Идемпотентность transfer-вебхуков — по transfer_id.
    pub transfer_id: String,
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

/// Триггер цикла выплат. Админ (RBAC). В проде — cron-воркер
/// пн 10:00 UTC; финальные payees передаются при блокировке/уходе.
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
    // Финальные выплаты (блокировка/уход) задаются отдельным
    // действием админа; здесь регулярный цикл.
    match payouts.run_payout_cycle(&[]).await {
        Ok(rows) => (
            StatusCode::OK,
            Json(PayoutRunResponse {
                payouts_created: rows.len() as i64,
                total_minor: rows.iter().map(|r| r.amount_minor).sum(),
            }),
        )
            .into_response(),
        Err(_) => api_error(ApiError::Internal, "db error"),
    }
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
async fn transfer_webhook(
    State(state): State<ApiEnv>,
    axum::Json(body): axum::Json<TransferWebhookBody>,
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
    match body.event_type.as_str() {
        "transfer.paid" => match payouts.handle_transfer_paid(&body.transfer_id).await {
            // true — закрыли; false — дубль вебхука (no-op 200).
            Ok(_) => (
                StatusCode::OK,
                Json(serde_json::json!({ "received": true })),
            )
                .into_response(),
            Err(dropit_infra_db::PayoutError::NotFound) => {
                api_error(ApiError::NotFound, "transfer not found")
            }
            Err(_) => api_error(ApiError::Internal, "db error"),
        },
        "transfer.failed" => {
            let reason = body.reason.clone().unwrap_or_else(|| "unspecified".into());
            match payouts
                .handle_transfer_failed(&body.transfer_id, &reason)
                .await
            {
                Ok(row) => (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "received": true,
                        "status": row.status,
                        "attempts": row.attempts,
                    })),
                )
                    .into_response(),
                Err(dropit_infra_db::PayoutError::NotFound) => {
                    api_error(ApiError::NotFound, "transfer not found")
                }
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
