//! Разбор ответов Stripe из `serde_json::Value` в типы контракта.
//!
//! Используется ручное чтение полей (а не `#[derive(Deserialize)]`), чтобы:
//!  * давать осмысленные `Decode`-ошибки с именем отсутствующего поля;
//!  * толерантно относиться к null/отсутствию опциональных полей;
//!  * не падать на лишних полях Stripe (forward-compat).

use serde_json::Value;

use crate::error::StripeError;
use crate::types::{
    ConnectedAccount, IntentResult, IntentStatus, RefundResult, TransferResult, WebhookEvent,
};

/// Максимальная длина текста ошибки в `StripeError::Api.message`.
const MAX_ERROR_TEXT: usize = 500;

/// Читает обязательное строковое поле.
fn str_field(value: &Value, name: &str) -> Result<String, StripeError> {
    match value.get(name).and_then(|v| v.as_str()) {
        Some(s) if !s.is_empty() => Ok(s.to_string()),
        _ => Err(StripeError::Decode(format!(
            "поле `{name}` отсутствует, пусто или не строка"
        ))),
    }
}

/// Читает опциональное строковое поле (`null`/отсутствие → `None`).
fn opt_str_field(value: &Value, name: &str) -> Result<Option<String>, StripeError> {
    match value.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if !s.is_empty() => Ok(Some(s.clone())),
        Some(Value::String(_)) => Ok(None),
        Some(other) => Err(StripeError::Decode(format!(
            "поле `{name}` ожидается строка, получено {other}"
        ))),
    }
}

/// Читает обязательное целочисленное поле.
fn i64_field(value: &Value, name: &str) -> Result<i64, StripeError> {
    match value.get(name).and_then(|v| v.as_i64()) {
        Some(n) => Ok(n),
        None => Err(StripeError::Decode(format!(
            "поле `{name}` отсутствует или не целое"
        ))),
    }
}

/// Разбирает ответ `POST /v1/payment_intents` (и capture).
pub(crate) fn parse_intent(json: &Value) -> Result<IntentResult, StripeError> {
    let id = str_field(json, "id")?;
    let amount_minor = i64_field(json, "amount")?;
    let status_raw = str_field(json, "status")?;
    let status = IntentStatus::from_stripe_str(&status_raw).ok_or_else(|| {
        StripeError::Decode(format!("неизвестный статус payment_intent: {status_raw}"))
    })?;
    let client_secret = opt_str_field(json, "client_secret")?;
    Ok(IntentResult {
        id,
        status,
        amount_minor,
        client_secret,
    })
}

/// Разбирает ответ `POST /v1/refunds`.
pub(crate) fn parse_refund(json: &Value) -> Result<RefundResult, StripeError> {
    let id = str_field(json, "id")?;
    let amount_minor = i64_field(json, "amount")?;
    let status = str_field(json, "status")?;
    Ok(RefundResult {
        id,
        amount_minor,
        status,
    })
}

/// Разбирает ответ аккаунта (`POST /v1/accounts`, `GET /v1/accounts/{id}`).
///
/// Булевые флаги при отсутствии/`null` принимаются равными `false`
/// (аккаунт без пройденного онбординга).
pub(crate) fn parse_connected_account(json: &Value) -> Result<ConnectedAccount, StripeError> {
    let id = str_field(json, "id")?;
    Ok(ConnectedAccount {
        id,
        charges_enabled: json
            .get("charges_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        payouts_enabled: json
            .get("payouts_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        details_submitted: json
            .get("details_submitted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    })
}

/// Разбирает ответ `POST /v1/account_links` → извлекает `url`.
pub(crate) fn parse_account_link(json: &Value) -> Result<String, StripeError> {
    let url = str_field(json, "url")?;
    Ok(url)
}

/// Разбирает ответ `POST /v1/transfers`.
///
/// Stripe не возвращает поле `status` у Transfer — при отсутствии
/// подставляется `"created"` (финальное состояние приходит вебхуком).
pub(crate) fn parse_transfer(json: &Value) -> Result<TransferResult, StripeError> {
    let id = str_field(json, "id")?;
    let amount_minor = i64_field(json, "amount")?;
    let status = opt_str_field(json, "status")?.unwrap_or_else(|| "created".to_string());
    Ok(TransferResult {
        id,
        amount_minor,
        status,
    })
}

/// Обрезает текст ошибки для безопасного логирования/возврата.
fn truncate(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        text.to_string()
    } else {
        let mut s: String = text.chars().take(max).collect();
        s.push('…');
        s
    }
}

/// Строит [`StripeError::Api`] из тела не-2xx ответа.
///
/// Пытается разобрать JSON `{ "error": { code, message, ... } }`;
/// при отсутствии поля `code` берёт `decline_code`, затем `type`,
/// затем `"unknown_error"`. Если тело — не JSON, сообщение = сырой текст.
pub(crate) fn parse_api_error(status: u16, body: &[u8]) -> StripeError {
    let text = String::from_utf8_lossy(body);
    if let Some(err) = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| v.get("error").cloned())
    {
        let code = err
            .get("code")
            .and_then(|v| v.as_str())
            .or_else(|| err.get("decline_code").and_then(|v| v.as_str()))
            .or_else(|| err.get("type").and_then(|v| v.as_str()))
            .unwrap_or("unknown_error")
            .to_string();
        let message = err
            .get("message")
            .and_then(|v| v.as_str())
            .map(|s| truncate(s, MAX_ERROR_TEXT))
            .unwrap_or_else(|| truncate(text.trim(), MAX_ERROR_TEXT));
        return StripeError::Api {
            status,
            code,
            message,
        };
    }
    StripeError::Api {
        status,
        code: "unknown_error".to_string(),
        message: truncate(text.trim(), MAX_ERROR_TEXT),
    }
}

/// Разбирает проверенный payload вебхука в [`WebhookEvent`].
///
/// Читает поля `id`, `type` (→ `event_type`), `data` (сырой объект).
/// `data` при отсутствии/null становится `Value::Null`.
pub(crate) fn parse_event(payload: &[u8]) -> Result<WebhookEvent, StripeError> {
    let value: Value = serde_json::from_slice(payload)
        .map_err(|e| StripeError::Decode(format!("payload вебхука не JSON: {e}")))?;
    let id = str_field(&value, "id")?;
    let event_type = str_field(&value, "type")?;
    let data = match value.get("data") {
        None | Some(Value::Null) => Value::Null,
        Some(other) => other.clone(),
    };
    Ok(WebhookEvent {
        id,
        event_type,
        data,
    })
}

/// Разбирает тело ответа (успешный JSON) в `Value` с понятной ошибкой.
pub(crate) fn decode_json(body: &[u8]) -> Result<Value, StripeError> {
    serde_json::from_slice(body)
        .map_err(|e| StripeError::Decode(format!("ответ не является JSON: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::IntentStatus;

    #[test]
    fn parse_intent_requires_capture_with_secret() {
        let v: Value = serde_json::from_str(
            r#"{
                "id": "pi_3O0000",
                "object": "payment_intent",
                "amount": 1500,
                "status": "requires_capture",
                "client_secret": "pi_3O0000_secret_abc"
            }"#,
        )
        .unwrap();
        let r = parse_intent(&v).unwrap();
        assert_eq!(r.id, "pi_3O0000");
        assert_eq!(r.amount_minor, 1500);
        assert_eq!(r.status, IntentStatus::RequiresCapture);
        assert_eq!(r.client_secret.as_deref(), Some("pi_3O0000_secret_abc"));
    }

    #[test]
    fn parse_intent_succeeded_without_secret() {
        let v: Value = serde_json::from_str(
            r#"{"id":"pi_1","object":"payment_intent","amount":500,"status":"succeeded","client_secret":null}"#,
        )
        .unwrap();
        let r = parse_intent(&v).unwrap();
        assert_eq!(r.status, IntentStatus::Succeeded);
        assert_eq!(r.client_secret, None);
    }

    #[test]
    fn parse_intent_requires_action_mapped() {
        let v: Value =
            serde_json::from_str(r#"{"id":"pi_9","amount":100,"status":"requires_action"}"#)
                .unwrap();
        let r = parse_intent(&v).unwrap();
        assert_eq!(r.status, IntentStatus::RequiresPaymentMethod);
    }

    #[test]
    fn parse_intent_unknown_status_errors() {
        let v: Value =
            serde_json::from_str(r#"{"id":"pi_x","amount":100,"status":"time_travel"}"#).unwrap();
        let err = parse_intent(&v).unwrap_err();
        assert!(matches!(err, StripeError::Decode(ref m) if m.contains("time_travel")));
    }

    #[test]
    fn parse_intent_missing_amount_errors() {
        let v: Value = serde_json::from_str(r#"{"id":"pi_x","status":"succeeded"}"#).unwrap();
        let err = parse_intent(&v).unwrap_err();
        assert!(matches!(err, StripeError::Decode(ref m) if m.contains("amount")));
    }

    #[test]
    fn parse_refund_fields() {
        let v: Value = serde_json::from_str(
            r#"{"id":"re_42","object":"refund","amount":750,"status":"succeeded"}"#,
        )
        .unwrap();
        let r = parse_refund(&v).unwrap();
        assert_eq!(r.id, "re_42");
        assert_eq!(r.amount_minor, 750);
        assert_eq!(r.status, "succeeded");
    }

    #[test]
    fn parse_account_missing_bools_defaults_false() {
        let v: Value = serde_json::from_str(r#"{"id":"acct_1","object":"account"}"#).unwrap();
        let a = parse_connected_account(&v).unwrap();
        assert_eq!(a.id, "acct_1");
        assert!(!a.charges_enabled);
        assert!(!a.payouts_enabled);
        assert!(!a.details_submitted);
    }

    #[test]
    fn parse_account_full() {
        let v: Value = serde_json::from_str(
            r#"{"id":"acct_2","charges_enabled":true,"payouts_enabled":false,"details_submitted":true}"#,
        )
        .unwrap();
        let a = parse_connected_account(&v).unwrap();
        assert!(a.charges_enabled);
        assert!(!a.payouts_enabled);
        assert!(a.details_submitted);
    }

    #[test]
    fn parse_account_link_url() {
        let v: Value = serde_json::from_str(
            r#"{"object":"account_link","url":"https://connect.stripe.com/setup/c/abc"}"#,
        )
        .unwrap();
        assert_eq!(
            parse_account_link(&v).unwrap(),
            "https://connect.stripe.com/setup/c/abc"
        );
    }

    #[test]
    fn parse_transfer_with_status() {
        let v: Value = serde_json::from_str(
            r#"{"id":"tr_1","object":"transfer","amount":1200,"status":"paid"}"#,
        )
        .unwrap();
        let t = parse_transfer(&v).unwrap();
        assert_eq!(t.id, "tr_1");
        assert_eq!(t.amount_minor, 1200);
        assert_eq!(t.status, "paid");
    }

    #[test]
    fn parse_transfer_without_status_defaults_created() {
        let v: Value =
            serde_json::from_str(r#"{"id":"tr_2","object":"transfer","amount":900}"#).unwrap();
        let t = parse_transfer(&v).unwrap();
        assert_eq!(t.status, "created");
    }

    #[test]
    fn parse_api_error_402_card_declined() {
        let body = br#"{
            "error": {
                "type": "card_error",
                "code": "card_declined",
                "decline_code": "insufficient_funds",
                "message": "Your card was declined."
            }
        }"#;
        let err = parse_api_error(402, body);
        match err {
            StripeError::Api {
                status,
                code,
                message,
            } => {
                assert_eq!(status, 402);
                assert_eq!(code, "card_declined");
                assert_eq!(message, "Your card was declined.");
            }
            other => panic!("expected Api, got {other:?}"),
        }
    }

    #[test]
    fn parse_api_error_falls_back_to_type_when_no_code() {
        let body = br#"{"error":{"type":"invalid_request_error","message":"bad param"}}"#;
        let err = parse_api_error(400, body);
        match err {
            StripeError::Api {
                status,
                code,
                message,
            } => {
                assert_eq!(status, 400);
                assert_eq!(code, "invalid_request_error");
                assert_eq!(message, "bad param");
            }
            other => panic!("expected Api, got {other:?}"),
        }
    }

    #[test]
    fn parse_api_error_non_json_body() {
        let err = parse_api_error(502, b"<html>Gateway down</html>");
        match err {
            StripeError::Api {
                status,
                code,
                message,
            } => {
                assert_eq!(status, 502);
                assert_eq!(code, "unknown_error");
                assert!(message.contains("Gateway down"));
            }
            other => panic!("expected Api, got {other:?}"),
        }
    }

    #[test]
    fn decode_json_rejects_garbage() {
        let err = decode_json(b"not json at all").unwrap_err();
        assert!(matches!(err, StripeError::Decode(_)));
    }

    #[test]
    fn parse_event_full() {
        let payload = br#"{
            "id": "evt_abc",
            "type": "payment_intent.succeeded",
            "data": {"object": {"id": "pi_1", "amount": 500}}
        }"#;
        let ev = parse_event(payload).unwrap();
        assert_eq!(ev.id, "evt_abc");
        assert_eq!(ev.event_type, "payment_intent.succeeded");
        assert_eq!(ev.data["object"]["amount"], 500);
    }

    #[test]
    fn parse_event_missing_data_is_null() {
        let payload = br#"{"id":"evt_2","type":"transfer.paid"}"#;
        let ev = parse_event(payload).unwrap();
        assert_eq!(ev.event_type, "transfer.paid");
        assert!(ev.data.is_null());
    }

    #[test]
    fn parse_event_missing_id_errors() {
        let payload = br#"{"type":"x"}"#;
        let err = parse_event(payload).unwrap_err();
        assert!(matches!(err, StripeError::Decode(ref m) if m.contains("id")));
    }

    #[test]
    fn parse_event_non_json_errors() {
        let err = parse_event(b"totally not json").unwrap_err();
        assert!(matches!(err, StripeError::Decode(_)));
    }
}
