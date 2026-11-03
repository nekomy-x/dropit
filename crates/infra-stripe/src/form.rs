//! Построение тел запросов Stripe в формате `application/x-www-form-urlencoded`.
//!
//! Кодирует параметры «руками» (без внешних crate-зависимостей), чтобы логика
//! построения запросов покрывалась детерминированными unit-тестами без сети.
//! Метаданные идут в виде `metadata[ключ]=значение`, ключи сортируются
//! (`BTreeMap`) — стабильный, воспроизводимый вывод.

use std::collections::BTreeMap;

use crate::error::StripeError;
use crate::types::{validate_amount, ConnectAccountReq, CreateIntent, RefundReq, TransferReq};

/// Максимальная длина Idempotency-Key (Stripe ограничивает 255 символами).
pub(crate) const MAX_IDEMPOTENCY_KEY_LEN: usize = 255;

/// Допустимый тип Connect-аккаунта при онбординге.
///
/// provisional (ADR-004 открыт): `express` — стандартный путь онбординга
/// через Stripe-hosted формы (`account_links`). Финальный выбор зависит от
/// Q14/Q15 (юрлицо, страна); смена значения — точечно, контракт не меняется.
pub const CONNECT_ACCOUNT_TYPE: &str = "express";

/// Внутренний билдер form-urlencoded тела: список пар `(ключ, значение)`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct FormBody {
    /// Пары в порядке добавления.
    pairs: Vec<(String, String)>,
}

impl FormBody {
    /// Пустое тело.
    pub(crate) fn new() -> Self {
        Self { pairs: Vec::new() }
    }

    /// Добавляет пару `ключ=значение`.
    pub(crate) fn push(&mut self, key: &str, value: impl Into<String>) -> &mut Self {
        self.pairs.push((key.to_string(), value.into()));
        self
    }

    /// Добавляет пару, только если значение есть.
    pub(crate) fn push_opt(&mut self, key: &str, value: Option<&str>) -> &mut Self {
        if let Some(v) = value {
            self.push(key, v);
        }
        self
    }

    /// Добавляет пару с целочисленным значением.
    pub(crate) fn push_i64(&mut self, key: &str, value: i64) -> &mut Self {
        self.push(key, value.to_string());
        self
    }

    /// Добавляет все метаданные как `metadata[ключ]=значение` (ключи отсортированы).
    pub(crate) fn push_metadata(&mut self, map: &BTreeMap<String, String>) -> &mut Self {
        for (k, v) in map {
            self.push(&format!("metadata[{k}]"), v.as_str());
        }
        self
    }

    /// Список пар (для тестов и аудита).
    #[cfg(test)]
    pub(crate) fn pairs(&self) -> &[(String, String)] {
        &self.pairs
    }

    /// Кодирует в строку `k=v&k=v` (application/x-www-form-urlencoded).
    pub(crate) fn encode(&self) -> String {
        self.pairs
            .iter()
            .map(|(k, v)| format!("{}={}", encode_component(k), encode_component(v)))
            .collect::<Vec<_>>()
            .join("&")
    }
}

/// Percent-encoding компонента по RFC 3986 (unreserved set).
///
/// `space` → `%20` (вместо `+`); символы вне unreserved — `%XX` в верхнем
/// регистре. UTF-8 кодируется побайтово. Квадратные скобки в ключах
/// `metadata[x]` кодируются как `%5B` / `%5D` — Stripe принимает оба вида,
/// это совпадает с поведением официальных SDK.
pub(crate) fn encode_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for &b in input.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => {
                out.push('%');
                out.push(hex_digit(b >> 4));
                out.push(hex_digit(b & 0x0f));
            }
        }
    }
    out
}

fn hex_digit(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        _ => (b'A' + n - 10) as char,
    }
}

/// Нормализует валюту: trim + lowercase. Должна быть 3 ASCII-буквы.
pub(crate) fn normalize_currency(input: &str) -> Result<String, StripeError> {
    let lower = input.trim().to_lowercase();
    let ok = lower.len() == 3 && lower.bytes().all(|b| b.is_ascii_alphabetic());
    if ok {
        Ok(lower)
    } else {
        Err(StripeError::Config(format!(
            "валюта должна быть 3 ASCII-буквы, получено {input:?}"
        )))
    }
}

/// Валидирует Idempotency-Key: непустой, без управляющих символов, ≤ 255 байт.
pub(crate) fn validate_idempotency_key(key: &str) -> Result<(), StripeError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(StripeError::Config("idempotency_key пуст".into()));
    }
    if trimmed.len() > MAX_IDEMPOTENCY_KEY_LEN {
        return Err(StripeError::Config(format!(
            "idempotency_key длиннее {MAX_IDEMPOTENCY_KEY_LEN} байт"
        )));
    }
    if trimmed.bytes().any(|b| b.is_ascii_control()) {
        return Err(StripeError::Config(
            "idempotency_key содержит управляющие символы".into(),
        ));
    }
    Ok(())
}

/// Валидирует непустой path-сегмент (id ресурса) и процент-кодирует его.
pub(crate) fn escape_path_segment(segment: &str) -> Result<String, StripeError> {
    let trimmed = segment.trim();
    if trimmed.is_empty() {
        return Err(StripeError::Config(
            "идентификатор ресурса Stripe пуст".into(),
        ));
    }
    Ok(encode_component(trimmed))
}

/// Тело запроса `POST /v1/payment_intents`.
pub(crate) fn payment_intent_body(req: &CreateIntent) -> Result<FormBody, StripeError> {
    validate_amount(req.amount_minor)?;
    validate_idempotency_key(&req.idempotency_key)?;
    let mut form = FormBody::new();
    form.push_i64("amount", req.amount_minor);
    form.push("currency", normalize_currency(&req.currency)?);
    form.push("capture_method", req.capture_method.as_stripe_str());
    form.push_opt("customer", req.customer_id.as_deref());
    form.push_opt(
        "transfer_data[destination]",
        req.connected_account.as_deref(),
    );
    form.push_metadata(&req.metadata);
    Ok(form)
}

/// Тело запроса `POST /v1/payment_intents/{id}/capture`.
pub(crate) fn capture_body(amount_minor: Option<i64>) -> Result<FormBody, StripeError> {
    let mut form = FormBody::new();
    if let Some(amount) = amount_minor {
        validate_amount(amount)?;
        form.push_i64("amount_to_capture", amount);
    }
    Ok(form)
}

/// Тело запроса `POST /v1/refunds`.
pub(crate) fn refund_body(req: &RefundReq) -> Result<FormBody, StripeError> {
    let intent = req.payment_intent.trim();
    if intent.is_empty() {
        return Err(StripeError::Config("payment_intent пуст".into()));
    }
    validate_idempotency_key(&req.idempotency_key)?;
    if let Some(amount) = req.amount_minor {
        validate_amount(amount)?;
    }
    let mut form = FormBody::new();
    form.push("payment_intent", intent);
    form.push_opt("amount", req.amount_minor.map(|a| a.to_string()).as_deref());
    form.push_opt("reason", req.reason.as_deref());
    Ok(form)
}

/// Тело запроса `POST /v1/accounts`.
pub(crate) fn connect_account_body(req: &ConnectAccountReq) -> Result<FormBody, StripeError> {
    if req.email.trim().is_empty() {
        return Err(StripeError::Config("email пуст".into()));
    }
    if req.country.trim().is_empty() || req.business_type.trim().is_empty() {
        return Err(StripeError::Config("country/business_type пуст".into()));
    }
    let mut form = FormBody::new();
    form.push("type", CONNECT_ACCOUNT_TYPE);
    form.push("email", req.email.trim());
    form.push("country", req.country.trim());
    form.push("business_type", req.business_type.trim());
    form.push_metadata(&req.metadata);
    Ok(form)
}

/// Тело запроса `POST /v1/account_links`.
pub(crate) fn account_link_body(
    account_id: &str,
    refresh_url: &str,
    return_url: &str,
) -> Result<FormBody, StripeError> {
    let account = escape_path_segment(account_id)?;
    let refresh = refresh_url.trim();
    let return_ = return_url.trim();
    if refresh.is_empty() || return_.is_empty() {
        return Err(StripeError::Config("refresh_url/return_url пуст".into()));
    }
    let mut form = FormBody::new();
    form.push("account", account.as_str());
    form.push("refresh_url", refresh);
    form.push("return_url", return_);
    form.push("type", "account_onboarding");
    Ok(form)
}

/// Тело запроса `POST /v1/transfers`.
pub(crate) fn transfer_body(req: &TransferReq) -> Result<FormBody, StripeError> {
    validate_amount(req.amount_minor)?;
    validate_idempotency_key(&req.idempotency_key)?;
    let dest = req.destination.trim();
    if dest.is_empty() {
        return Err(StripeError::Config("destination пуст".into()));
    }
    let mut form = FormBody::new();
    form.push_i64("amount", req.amount_minor);
    form.push("currency", normalize_currency(&req.currency)?);
    form.push("destination", dest);
    form.push_metadata(&req.metadata);
    Ok(form)
}

/// Retrieve the existing PaymentIntent by its escaped provider id.
pub(crate) fn payment_intent_path(intent_id: &str) -> Result<String, StripeError> {
    let id = escape_path_segment(intent_id)?;
    Ok(format!("/v1/payment_intents/{id}"))
}

/// Путь запроса захвата: `/v1/payment_intents/{id}/capture`.
pub(crate) fn capture_path(intent_id: &str) -> Result<String, StripeError> {
    let id = escape_path_segment(intent_id)?;
    Ok(format!("/v1/payment_intents/{id}/capture"))
}

/// Путь запроса аккаунта: `/v1/accounts/{id}`.
pub(crate) fn account_path(account_id: &str) -> Result<String, StripeError> {
    let id = escape_path_segment(account_id)?;
    Ok(format!("/v1/accounts/{id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CaptureMethod::{Automatic, Manual};

    fn meta(items: &[(&str, &str)]) -> BTreeMap<String, String> {
        items
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn percent_encoding() {
        assert_eq!(encode_component("hello"), "hello");
        assert_eq!(encode_component("a b"), "a%20b");
        assert_eq!(encode_component("100&200=300"), "100%26200%3D300");
        // Квадратные скобки в ключах кодируются — как в официальных SDK Stripe.
        assert_eq!(
            encode_component("metadata[order_id]"),
            "metadata%5Border_id%5D"
        );
        // UTF-8 кодируется побайтово.
        assert_eq!(encode_component("ä"), "%C3%A4");
        assert_eq!(encode_component("-._~"), "-._~");
    }

    #[test]
    fn currency_normalized_and_validated() {
        assert_eq!(normalize_currency(" EUR ").unwrap(), "eur");
        assert!(normalize_currency("EU").is_err());
        assert!(normalize_currency("евро").is_err());
        assert!(normalize_currency("US1").is_err());
    }

    #[test]
    fn idempotency_key_validated() {
        assert!(validate_idempotency_key("abc-123").is_ok());
        assert!(validate_idempotency_key("  ").is_err());
        assert!(validate_idempotency_key(&"x".repeat(300)).is_err());
        assert!(validate_idempotency_key("bad\nkey").is_err());
    }

    #[test]
    fn payment_intent_body_full() {
        let req = CreateIntent {
            amount_minor: 1500,
            currency: "EUR".into(),
            customer_id: Some("cus_abc".into()),
            connected_account: Some("acct_club1".into()),
            capture_method: Manual,
            metadata: meta(&[("order_id", "42"), ("dj_id", "7")]),
            idempotency_key: "key-1".into(),
        };
        let body = payment_intent_body(&req).unwrap();
        // Порядок пар предсказуем: amount, currency, capture_method, customer,
        // transfer_data[destination], metadata (отсортировано: dj_id, order_id).
        assert_eq!(
            body.pairs(),
            &[
                ("amount".into(), "1500".into()),
                ("currency".into(), "eur".into()),
                ("capture_method".into(), "manual".into()),
                ("customer".into(), "cus_abc".into()),
                ("transfer_data[destination]".into(), "acct_club1".into()),
                ("metadata[dj_id]".into(), "7".into()),
                ("metadata[order_id]".into(), "42".into()),
            ]
        );
        let encoded = body.encode();
        assert!(encoded.contains("amount=1500"));
        assert!(encoded.contains("currency=eur"));
        assert!(encoded.contains("capture_method=manual"));
        assert!(encoded.contains("transfer_data%5Bdestination%5D=acct_club1"));
        assert!(encoded.contains("metadata%5Border_id%5D=42"));
        assert!(encoded.contains("metadata%5Bdj_id%5D=7"));
    }

    #[test]
    fn payment_intent_body_minimal() {
        let req = CreateIntent {
            amount_minor: 500,
            currency: "usd".into(),
            customer_id: None,
            connected_account: None,
            capture_method: Automatic,
            metadata: BTreeMap::new(),
            idempotency_key: "k".into(),
        };
        let body = payment_intent_body(&req).unwrap();
        let encoded = body.encode();
        assert_eq!(encoded, "amount=500&currency=usd&capture_method=automatic");
        assert!(!encoded.contains("customer"));
        assert!(!encoded.contains("transfer_data"));
        assert!(!encoded.contains("metadata"));
    }

    #[test]
    fn payment_intent_body_rejects_nonpositive_amount() {
        let req = CreateIntent {
            amount_minor: 0,
            currency: "eur".into(),
            customer_id: None,
            connected_account: None,
            capture_method: Automatic,
            metadata: BTreeMap::new(),
            idempotency_key: "k".into(),
        };
        let err = payment_intent_body(&req).unwrap_err();
        assert!(matches!(err, StripeError::Config(_)));
    }

    #[test]
    fn capture_body_optional_amount() {
        let full = capture_body(Some(1000)).unwrap();
        assert_eq!(full.pairs(), &[("amount_to_capture".into(), "1000".into())]);
        let none = capture_body(None).unwrap();
        assert!(none.pairs().is_empty());
        assert!(capture_body(Some(0)).is_err());
    }

    #[test]
    fn refund_body_fields() {
        let req = RefundReq {
            payment_intent: "pi_123".into(),
            amount_minor: Some(750),
            reason: Some("requested_by_customer".into()),
            idempotency_key: "rk-1".into(),
        };
        let body = refund_body(&req).unwrap();
        assert_eq!(
            body.pairs(),
            &[
                ("payment_intent".into(), "pi_123".into()),
                ("amount".into(), "750".into()),
                ("reason".into(), "requested_by_customer".into()),
            ]
        );
        // Полный возврат — без amount.
        let full = RefundReq {
            amount_minor: None,
            reason: None,
            ..req
        };
        let encoded = refund_body(&full).unwrap().encode();
        assert_eq!(encoded, "payment_intent=pi_123");
    }

    #[test]
    fn connect_account_body_has_type_express() {
        let req = ConnectAccountReq {
            email: "dj@club.io".into(),
            country: "ES".into(),
            business_type: "individual".into(),
            metadata: meta(&[("club_id", "c1")]),
        };
        let body = connect_account_body(&req).unwrap();
        assert!(body
            .pairs()
            .iter()
            .any(|(k, v)| k == "type" && v == CONNECT_ACCOUNT_TYPE));
        assert!(body
            .pairs()
            .iter()
            .any(|(k, v)| k == "metadata[club_id]" && v == "c1"));
    }

    #[test]
    fn account_link_body_fields() {
        let body =
            account_link_body("acct_1", "https://app/refresh", "https://app/return").unwrap();
        let encoded = body.encode();
        assert!(encoded.contains("account=acct_1"));
        assert!(encoded.contains("type=account_onboarding"));
        assert!(encoded.contains("refresh_url=https%3A%2F%2Fapp%2Frefresh"));
        assert!(encoded.contains("return_url=https%3A%2F%2Fapp%2Freturn"));
    }

    #[test]
    fn transfer_body_fields() {
        let req = TransferReq {
            amount_minor: 1200,
            currency: "EUR".into(),
            destination: "acct_dj1".into(),
            metadata: meta(&[("payout_id", "p1")]),
            idempotency_key: "tk-1".into(),
        };
        let body = transfer_body(&req).unwrap();
        let encoded = body.encode();
        assert!(encoded.contains("amount=1200"));
        assert!(encoded.contains("currency=eur"));
        assert!(encoded.contains("destination=acct_dj1"));
        assert!(encoded.contains("metadata%5Bpayout_id%5D=p1"));
    }

    #[test]
    fn paths_escape_ids() {
        assert_eq!(payment_intent_path("pi_abc").unwrap(), "/v1/payment_intents/pi_abc");
        assert_eq!(payment_intent_path("a/b").unwrap(), "/v1/payment_intents/a%2Fb");
        assert!(payment_intent_path("  ").is_err());
        assert_eq!(
            capture_path("pi_abc").unwrap(),
            "/v1/payment_intents/pi_abc/capture"
        );
        assert_eq!(account_path("acct_1").unwrap(), "/v1/accounts/acct_1");
        assert!(capture_path("  ").is_err());
        // Поддельный id с разделителем пути не пробивается наружу.
        assert_eq!(account_path("a/b").unwrap(), "/v1/accounts/a%2Fb");
    }
}
