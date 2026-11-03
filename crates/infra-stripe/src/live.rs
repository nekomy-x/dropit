//! Настоящий REST-клиент Stripe + проверка подписи вебхуков.
//!
//! `LiveStripe` не ходит в сеть при создании (`new`) — только при вызовах
//! методов трейта. Конструктор валидирует конфигурацию (ключ, секрет, base_url)
//! и строит `reqwest::Client` с таймаутом 20 с. Все запросы:
//!  * Bearer-авторизация (`Authorization: Bearer <api_key>`);
//!  * заголовок `Stripe-Version`;
//!  * `Idempotency-Key` для денежных POST;
//!  * тело `application/x-www-form-urlencoded`;
//!  * таймаут 20 с.
//!
//! Не-2xx → [`crate::StripeError::Api`] из JSON `error`.

use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;

use crate::api::StripeApi;
use crate::error::StripeError;
use crate::form::{
    account_link_body, account_path, capture_body, capture_path, connect_account_body,
    payment_intent_body, payment_intent_path, refund_body, transfer_body,
};
use crate::json::{
    decode_json, parse_account_link, parse_api_error, parse_connected_account, parse_intent,
    parse_refund, parse_transfer,
};
use crate::types::{
    ConnectAccountReq, ConnectedAccount, CreateIntent, IntentResult, RefundReq, RefundResult,
    TransferReq, TransferResult, WebhookEvent,
};
use crate::webhook::{now_unix, verify_signature};

/// Базовый URL Stripe API (по умолчанию).
pub const DEFAULT_BASE_URL: &str = "https://api.stripe.com";

/// Версия Stripe API, фиксированная в заголовке `Stripe-Version`.
///
/// Пин версии обязателен: иначе Stripe использует версию момента создания
/// аккаунта и поведение дрейфует (техкарта требует детерминизма ответов).
pub const STRIPE_API_VERSION: &str = "2024-06-20";

/// Таймаут запроса к Stripe (техкарта, Часть VI: 20 с).
pub const REQUEST_TIMEOUT_SECS: u64 = 20;

const IDEMPOTENCY_HEADER_NAME: &str = "idempotency-key";
const STRIPE_VERSION_HEADER_NAME: &str = "stripe-version";

/// Настоящий клиент Stripe (REST + HMAC вебхуков).
///
/// Поля закрыты: `api_key` нигде не логируется и не попадает в `Debug`.
/// Сетевой вызов происходит только в методах трейта, не в `new`.
pub struct LiveStripe {
    /// Секретный ключ Stripe (`sk_live_…` / `sk_test_…`).
    api_key: String,
    /// Секрет webhook-подписки (`whsec_…`).
    webhook_secret: String,
    /// Базовый URL (без trailing slash).
    base_url: String,
    /// HTTP-клиент с таймаутом.
    client: reqwest::Client,
}

impl LiveStripe {
    /// Создаёт клиент. Без сетевых вызовов: только валидация конфигурации
    /// и построение `reqwest::Client`.
    ///
    /// `base_url = None` → [`DEFAULT_BASE_URL`]. Полезно для тестов и для
    /// указания mock-сервера Stripe (не путать с [`crate::MockStripe`]).
    pub fn new(
        api_key: String,
        webhook_secret: String,
        base_url: Option<String>,
    ) -> Result<Self, StripeError> {
        let api_key = api_key.trim().to_string();
        if api_key.is_empty() {
            return Err(StripeError::Config("STRIPE_API_KEY пуст".into()));
        }
        if api_key.chars().any(|c| c.is_whitespace()) {
            return Err(StripeError::Config(
                "STRIPE_API_KEY содержит пробельные символы".into(),
            ));
        }
        let webhook_secret = webhook_secret.trim().to_string();
        if webhook_secret.is_empty() {
            return Err(StripeError::Config(
                "STRIPE_WEBHOOK_SECRET пуст — проверка подписи вебхуков невозможна".into(),
            ));
        }
        let base_url = normalize_base_url(base_url.as_deref())?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| {
                StripeError::Transport(format!("не удалось построить HTTP-клиент: {e}"))
            })?;
        Ok(Self {
            api_key,
            webhook_secret,
            base_url,
            client,
        })
    }

    /// Создаёт клиент из переменных окружения
    /// `STRIPE_API_KEY` / `STRIPE_WEBHOOK_SECRET` (через [`crate::from_env`]).
    ///
    /// Возвращает `Ok(None)`, если переменные не заданы — это нормальный
    /// «мок-режим» (gateway-api остаётся на [`crate::MockStripe`]).
    pub fn from_env() -> Result<Option<Self>, StripeError> {
        match crate::from_env() {
            Some((api_key, webhook_secret)) => Self::new(api_key, webhook_secret, None).map(Some),
            None => Ok(None),
        }
    }

    /// Базовый URL (без trailing slash). Для инспекции/логов.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Заголовки авторизации + Stripe-Version для каждого запроса.
    fn base_headers(&self) -> Result<HeaderMap, StripeError> {
        let mut headers = HeaderMap::new();
        let bearer = format!("Bearer {}", self.api_key);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&bearer)
                .map_err(|_| StripeError::Config("api_key содержит не-ASCII символы".into()))?,
        );
        headers.insert(
            HeaderName::from_static(STRIPE_VERSION_HEADER_NAME),
            HeaderValue::from_static(STRIPE_API_VERSION),
        );
        Ok(headers)
    }

    /// POST form-urlencoded, опциональный Idempotency-Key. Возвращает разобранный JSON.
    async fn post_form(
        &self,
        path: &str,
        body: String,
        idempotency_key: Option<&str>,
    ) -> Result<Value, StripeError> {
        let url = format!("{}{}", self.base_url, path);
        let mut headers = self.base_headers()?;
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        if let Some(key) = idempotency_key {
            headers.insert(
                HeaderName::from_static(IDEMPOTENCY_HEADER_NAME),
                HeaderValue::from_str(key).map_err(|_| {
                    StripeError::Config("idempotency_key содержит не-ASCII символы".into())
                })?,
            );
        }
        tracing::debug!(target: "dropit.stripe", path = path, "POST → stripe");
        let resp = self
            .client
            .post(&url)
            .headers(headers)
            .body(body)
            .send()
            .await
            .map_err(|e| StripeError::Transport(format!("POST {path}: {e}")))?;
        let status = resp.status().as_u16();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| StripeError::Transport(format!("POST {path}: чтение тела: {e}")))?;
        if !(200..300).contains(&status) {
            let err = parse_api_error(status, &bytes);
            tracing::warn!(
                target: "dropit.stripe",
                path = path,
                status = status,
                code = err_code(&err),
                "Stripe API error"
            );
            return Err(err);
        }
        decode_json(&bytes)
    }

    /// GET с авторизацией (без тела).
    async fn get(&self, path: &str) -> Result<Value, StripeError> {
        let url = format!("{}{}", self.base_url, path);
        let headers = self.base_headers()?;
        tracing::debug!(target: "dropit.stripe", path = path, "GET → stripe");
        let resp = self
            .client
            .get(&url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| StripeError::Transport(format!("GET {path}: {e}")))?;
        let status = resp.status().as_u16();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| StripeError::Transport(format!("GET {path}: чтение тела: {e}")))?;
        if !(200..300).contains(&status) {
            let err = parse_api_error(status, &bytes);
            tracing::warn!(
                target: "dropit.stripe",
                path = path,
                status = status,
                code = err_code(&err),
                "Stripe API error"
            );
            return Err(err);
        }
        decode_json(&bytes)
    }
}

#[async_trait::async_trait]
impl StripeApi for LiveStripe {
    async fn retrieve_payment_intent(&self, intent_id: &str) -> Result<IntentResult, StripeError> {
        let json = self.get(&payment_intent_path(intent_id)?).await?;
        parse_intent(&json)
    }

    async fn create_payment_intent(&self, req: CreateIntent) -> Result<IntentResult, StripeError> {
        let form = payment_intent_body(&req)?;
        let key = req.idempotency_key.trim().to_string();
        let json = self
            .post_form("/v1/payment_intents", form.encode(), Some(&key))
            .await?;
        parse_intent(&json)
    }

    async fn capture(
        &self,
        intent_id: &str,
        amount_minor: Option<i64>,
        idempotency_key: &str,
    ) -> Result<IntentResult, StripeError> {
        let path = capture_path(intent_id)?;
        let form = capture_body(amount_minor)?;
        let key = idempotency_key.trim().to_string();
        let json = self.post_form(&path, form.encode(), Some(&key)).await?;
        parse_intent(&json)
    }

    async fn refund(&self, req: RefundReq) -> Result<RefundResult, StripeError> {
        let form = refund_body(&req)?;
        let key = req.idempotency_key.trim().to_string();
        let json = self
            .post_form("/v1/refunds", form.encode(), Some(&key))
            .await?;
        parse_refund(&json)
    }

    async fn create_connected_account(
        &self,
        req: ConnectAccountReq,
    ) -> Result<ConnectedAccount, StripeError> {
        let form = connect_account_body(&req)?;
        let json = self.post_form("/v1/accounts", form.encode(), None).await?;
        parse_connected_account(&json)
    }

    async fn retrieve_account(&self, account_id: &str) -> Result<ConnectedAccount, StripeError> {
        let path = account_path(account_id)?;
        let json = self.get(&path).await?;
        parse_connected_account(&json)
    }

    async fn account_link(
        &self,
        account_id: &str,
        refresh_url: &str,
        return_url: &str,
    ) -> Result<String, StripeError> {
        let form = account_link_body(account_id, refresh_url, return_url)?;
        let json = self
            .post_form("/v1/account_links", form.encode(), None)
            .await?;
        parse_account_link(&json)
    }

    async fn create_transfer(&self, req: TransferReq) -> Result<TransferResult, StripeError> {
        let form = transfer_body(&req)?;
        let key = req.idempotency_key.trim().to_string();
        let json = self
            .post_form("/v1/transfers", form.encode(), Some(&key))
            .await?;
        parse_transfer(&json)
    }

    fn verify_webhook(
        &self,
        payload: &[u8],
        signature_header: &str,
        tolerance_secs: i64,
    ) -> Result<WebhookEvent, StripeError> {
        verify_signature(
            self.webhook_secret.as_bytes(),
            payload,
            signature_header,
            tolerance_secs,
            now_unix(),
        )?;
        crate::json::parse_event(payload)
    }
}

/// Красная `Debug`-реализация: ключ и секрет НЕ выводятся.
impl std::fmt::Debug for LiveStripe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveStripe")
            .field("base_url", &self.base_url)
            .field("api_key", &"<redacted>")
            .field("webhook_secret", &"<redacted>")
            .finish()
    }
}

/// Нормализует base_url: trim, scheme http/https, без trailing slash, без пробелов.
fn normalize_base_url(input: Option<&str>) -> Result<String, StripeError> {
    let raw = input.unwrap_or(DEFAULT_BASE_URL).trim();
    if raw.is_empty() {
        return Err(StripeError::Config("base_url пуст".into()));
    }
    let parsed = reqwest::Url::parse(raw)
        .map_err(|e| StripeError::Config(format!("неверный base_url {raw:?}: {e}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(StripeError::Config(format!(
            "base_url должен быть http/https, получена схема {}",
            parsed.scheme()
        )));
    }
    if parsed.host_str().is_none() {
        return Err(StripeError::Config("base_url без host".into()));
    }
    // Убираем trailing slash (пути присоединяются как /v1/...).
    let s = parsed.to_string();
    Ok(s.trim_end_matches('/').to_string())
}

/// Извлекает `code` из `StripeError::Api` для лога (без клонирования).
fn err_code(err: &StripeError) -> &str {
    match err {
        StripeError::Api { code, .. } => code.as_str(),
        _ => "<non-api>",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_validates_empty_key() {
        let err = LiveStripe::new("".into(), "whsec_x".into(), None).unwrap_err();
        assert!(matches!(err, StripeError::Config(_)));
        let err = LiveStripe::new("  ".into(), "whsec_x".into(), None).unwrap_err();
        assert!(matches!(err, StripeError::Config(_)));
    }

    #[test]
    fn new_rejects_whitespace_in_key() {
        let err = LiveStripe::new("sk_test_abc def".into(), "whsec_x".into(), None).unwrap_err();
        assert!(matches!(err, StripeError::Config(_)));
    }

    #[test]
    fn new_validates_empty_webhook_secret() {
        let err = LiveStripe::new("sk_test_x".into(), "".into(), None).unwrap_err();
        assert!(matches!(err, StripeError::Config(_)));
    }

    #[test]
    fn new_validates_base_url_scheme() {
        let err = LiveStripe::new("sk_test_x".into(), "whsec_x".into(), Some("ftp://x".into()))
            .unwrap_err();
        assert!(matches!(err, StripeError::Config(_)));
        let err = LiveStripe::new(
            "sk_test_x".into(),
            "whsec_x".into(),
            Some("not-a-url".into()),
        )
        .unwrap_err();
        assert!(matches!(err, StripeError::Config(_)));
    }

    #[test]
    fn new_strips_trailing_slash_and_uses_default() {
        let s = LiveStripe::new("sk_test_x".into(), "whsec_x".into(), None).unwrap();
        assert_eq!(s.base_url(), DEFAULT_BASE_URL);
        let s = LiveStripe::new(
            "sk_test_x".into(),
            "whsec_x".into(),
            Some("https://api.stripe.com/".into()),
        )
        .unwrap();
        assert_eq!(s.base_url(), "https://api.stripe.com");
    }

    #[test]
    fn new_does_not_touch_network() {
        // Конструкция не делает HTTP-запросов — даже с «неправильным» хостом.
        let s = LiveStripe::new(
            "sk_test_x".into(),
            "whsec_x".into(),
            Some("https://invalid.invalid".into()),
        )
        .unwrap();
        assert_eq!(s.base_url(), "https://invalid.invalid");
    }

    #[test]
    fn debug_redacts_secrets() {
        let s = LiveStripe::new(
            "sk_test_super_secret".into(),
            "whsec_top_secret".into(),
            None,
        )
        .unwrap();
        let dbg = format!("{s:?}");
        assert!(!dbg.contains("super_secret"));
        assert!(!dbg.contains("top_secret"));
        assert!(dbg.contains("<redacted>"));
    }

    #[test]
    fn from_env_returns_ok_without_panicking() {
        // В CI переменные могут быть не заданы — это Ok(None), не ошибка.
        let _ = LiveStripe::from_env();
    }
}
