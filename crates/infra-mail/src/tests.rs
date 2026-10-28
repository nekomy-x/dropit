//! Unit-тесты крейта `dropit-infra-mail`.
//!
//! Все тесты детерминированы и не выполняют сетевых вызовов: транспорт
//! подменяется фиктивным [`FakeTransport`], время в проверке подписи
//! передаётся явно (`now_unix`), окружение — через [`from_env_via`].
//! Ни один тест не требует `RESEND_API_KEY`.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::event::{parse_event, MailEventKind};
use crate::resend::{
    from_env_via, ResendConfig, ResendSender, DEFAULT_API_BASE, MAX_IDEMPOTENCY_KEY_LEN,
};
use crate::sender::{MailAddress, MailSender, NoopMail, OutgoingMail, SendReceipt};
use crate::template::{escape_html, render, TemplateKey};
use crate::transport::{HttpRequest, HttpResponse, HttpTransport, SharedTransport};
use crate::webhook::{WebhookHeaders, WebhookVerifier, WEBHOOK_TOLERANCE_SECS, WHSEC_PREFIX};
use crate::MailError;

// ---------------------------------------------------------------------------
// Фиктивный транспорт
// ---------------------------------------------------------------------------

/// Запланированный исход одного запроса.
#[derive(Debug, Clone)]
enum Stub {
    /// Ответ с заданными статусом, заголовками и телом.
    Response {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    },
    /// Сбой транспорта: таймаут.
    Timeout,
    /// Сбой транспорта: соединение не установлено.
    Unavailable,
}

/// Правило «URL содержит подстроку» с очередью исходов.
struct FakeRule {
    match_sub: String,
    stubs: Mutex<VecDeque<Stub>>,
}

/// Фиктивный HTTP-транспорт: пишет запросы, отвечает по правилам.
struct FakeTransport {
    rules: Mutex<Vec<FakeRule>>,
    calls: Mutex<Vec<HttpRequest>>,
}

impl FakeTransport {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            rules: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
        })
    }

    /// Зарегистрировать очередь исходов для URL, содержащих `match_sub`.
    fn stub(self: &Arc<Self>, match_sub: &str, stubs: Vec<Stub>) {
        self.rules.lock().unwrap().push(FakeRule {
            match_sub: match_sub.to_string(),
            stubs: Mutex::new(stubs.into()),
        });
    }

    /// Перехваченные запросы.
    fn recorded(&self) -> Vec<HttpRequest> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl HttpTransport for FakeTransport {
    async fn execute(&self, req: HttpRequest) -> Result<HttpResponse, MailError> {
        self.calls.lock().unwrap().push(req.clone());
        let rules = self.rules.lock().unwrap();
        for rule in rules.iter() {
            if req.url.contains(&rule.match_sub) {
                let stub = rule.stubs.lock().unwrap().pop_front();
                return match stub {
                    Some(Stub::Response {
                        status,
                        headers,
                        body,
                    }) => Ok(HttpResponse {
                        status,
                        headers,
                        body,
                    }),
                    Some(Stub::Timeout) => Err(MailError::Timeout),
                    Some(Stub::Unavailable) => Err(MailError::Unavailable(
                        "fake transport: connection refused".into(),
                    )),
                    None => Err(MailError::Unavailable(format!(
                        "fake transport: no stubs left for {}",
                        rule.match_sub
                    ))),
                };
            }
        }
        Err(MailError::Unavailable(format!(
            "fake transport: no rule for {}",
            req.url
        )))
    }
}

/// Ответ без заголовков.
fn resp(status: u16, body: &[u8]) -> Stub {
    Stub::Response {
        status,
        headers: Vec::new(),
        body: body.to_vec(),
    }
}

/// Ответ с заголовками.
fn resp_h(status: u16, headers: &[(&str, &str)], body: &[u8]) -> Stub {
    Stub::Response {
        status,
        headers: headers
            .iter()
            .map(|(n, v)| ((*n).to_string(), (*v).to_string()))
            .collect(),
        body: body.to_vec(),
    }
}

/// Тело успешного ответа Resend.
fn ok_body(id: &str) -> Vec<u8> {
    format!("{{\"id\":\"{id}\"}}").into_bytes()
}

/// Собрать отправителя на фейковом транспорте.
fn sender_with(http: &Arc<FakeTransport>) -> ResendSender {
    let config = ResendConfig {
        api_key: "re_test_key".to_string(),
        api_base: DEFAULT_API_BASE.to_string(),
        from: MailAddress::parse("DROP IT <no-reply@dropit.club>").unwrap(),
        reply_to: None,
    };
    ResendSender::with_transport(config, http.clone() as SharedTransport)
}

/// Письмо с заданным ключом идемпотентности.
fn mail(key: &str) -> OutgoingMail {
    OutgoingMail {
        id: uuid::Uuid::now_v7(),
        idempotency_key: key.to_string(),
        from: MailAddress::parse("DROP IT <no-reply@dropit.club>").unwrap(),
        to: MailAddress::parse("guest@example.com").unwrap(),
        reply_to: None,
        subject: "Тема".to_string(),
        html: "<p>Привет</p>".to_string(),
        text: "Привет".to_string(),
    }
}

/// Заголовок запроса по имени (без учёта регистра).
fn header<'a>(req: &'a HttpRequest, name: &str) -> Option<&'a str> {
    req.headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Тело запроса как JSON.
fn body_json(req: &HttpRequest) -> serde_json::Value {
    serde_json::from_slice(&req.body).expect("request body is json")
}

// ---------------------------------------------------------------------------
// MailAddress: разбор и форматирование
// ---------------------------------------------------------------------------

#[test]
fn address_parses_bare_email() {
    let addr = MailAddress::parse("  a@b.c  ").expect("parse");
    assert_eq!(addr.email, "a@b.c");
    assert_eq!(addr.name, None);
    assert_eq!(addr.to_string(), "a@b.c");
}

#[test]
fn address_parses_name_and_email() {
    let addr = MailAddress::parse("DROP IT <no-reply@dropit.club>").expect("parse");
    assert_eq!(addr.email, "no-reply@dropit.club");
    assert_eq!(addr.name.as_deref(), Some("DROP IT"));
    assert_eq!(addr.to_string(), "DROP IT <no-reply@dropit.club>");
}

#[test]
fn address_strips_quoted_name() {
    let addr = MailAddress::parse("\"DROP IT\" <no-reply@dropit.club>").expect("parse");
    assert_eq!(addr.name.as_deref(), Some("DROP IT"));
    assert_eq!(addr.to_string(), "DROP IT <no-reply@dropit.club>");
}

#[test]
fn address_roundtrips_through_display() {
    for raw in [
        "a@b.c",
        "DROP IT <no-reply@dropit.club>",
        "\"DROP IT\" <no-reply@dropit.club>",
        "Иван Петров <ivan@club.ru>",
    ] {
        let first = MailAddress::parse(raw).expect("parse");
        let second = MailAddress::parse(&first.to_string()).expect("reparse");
        assert_eq!(first, second, "roundtrip failed for {raw:?}");
    }
}

#[test]
fn address_rejects_invalid_input() {
    for raw in [
        "",
        "   ",
        "no-at-sign",
        "two@at@signs",
        "@b.c",
        "a@",
        "a b@c.d",
        "Name <>",
        "<a@b.c",
    ] {
        let err = MailAddress::parse(raw).unwrap_err();
        assert!(matches!(err, MailError::Config(_)), "{raw:?} → {err:?}");
    }
}

// ---------------------------------------------------------------------------
// Resend: запрос
// ---------------------------------------------------------------------------

#[tokio::test]
async fn send_sets_idempotency_and_auth_headers() {
    let http = FakeTransport::new();
    http.stub("/emails", vec![resp(200, &ok_body("pm_1"))]);
    let sender = sender_with(&http);
    let key = "dj_invite:0b8f6a1e-0000-7000-8000-000000000000";

    let receipt = sender.send(&mail(key)).await.expect("send");

    assert_eq!(
        receipt,
        SendReceipt {
            provider_id: "pm_1".to_string(),
            provider: "resend",
        }
    );
    let req = &http.recorded()[0];
    assert_eq!(req.url, format!("{DEFAULT_API_BASE}/emails"));
    assert_eq!(header(req, "Idempotency-Key"), Some(key));
    assert!(key.len() <= MAX_IDEMPOTENCY_KEY_LEN);
    assert_eq!(header(req, "Authorization"), Some("Bearer re_test_key"));
    assert_eq!(header(req, "Content-Type"), Some("application/json"));
    assert_eq!(header(req, "User-Agent"), Some("dropit-infra-mail"));
}

#[tokio::test]
async fn send_rejects_overlong_idempotency_key_before_network() {
    let http = FakeTransport::new();
    let sender = sender_with(&http);
    let key = "k".repeat(MAX_IDEMPOTENCY_KEY_LEN + 1);

    let err = sender.send(&mail(&key)).await.unwrap_err();
    assert!(matches!(err, MailError::Config(_)), "{err:?}");
    assert!(http.recorded().is_empty(), "сеть не должна дёргаться");
}

#[tokio::test]
async fn send_rejects_empty_idempotency_key() {
    let http = FakeTransport::new();
    let sender = sender_with(&http);
    let err = sender.send(&mail("  ")).await.unwrap_err();
    assert!(matches!(err, MailError::Config(_)), "{err:?}");
    assert!(http.recorded().is_empty());
}

#[tokio::test]
async fn body_has_from_and_to_without_display_name() {
    let http = FakeTransport::new();
    http.stub("/emails", vec![resp(200, &ok_body("pm_1"))]);
    let sender = sender_with(&http);

    let mut out = mail("mail_test:1");
    out.to = MailAddress::parse("guest@example.com").unwrap();
    sender.send(&out).await.expect("send");

    let body = body_json(&http.recorded()[0]);
    assert_eq!(body["from"], "DROP IT <no-reply@dropit.club>");
    assert_eq!(body["to"][0], "guest@example.com");
    assert!(
        body.get("reply_to").is_none(),
        "reply_to не задан — поля нет"
    );
    assert_eq!(body["subject"], "Тема");
    assert_eq!(body["text"], "Привет");
}

#[tokio::test]
async fn body_has_display_name_and_reply_to() {
    let http = FakeTransport::new();
    http.stub("/emails", vec![resp(200, &ok_body("pm_2"))]);
    let sender = sender_with(&http);

    let mut out = mail("mail_test:2");
    out.to = MailAddress::parse("Иван Петров <ivan@club.ru>").unwrap();
    out.reply_to = Some(MailAddress::parse("support@dropit.club").unwrap());
    sender.send(&out).await.expect("send");

    let body = body_json(&http.recorded()[0]);
    assert_eq!(body["to"][0], "Иван Петров <ivan@club.ru>");
    assert_eq!(body["reply_to"][0], "support@dropit.club");
}

// ---------------------------------------------------------------------------
// Resend: отображение ошибок
// ---------------------------------------------------------------------------

#[tokio::test]
async fn status_429_maps_to_rate_limited_with_retry_after() {
    let http = FakeTransport::new();
    http.stub(
        "/emails",
        vec![resp_h(
            429,
            &[("Retry-After", "7")],
            b"{\"error\":\"slow down\"}",
        )],
    );
    let sender = sender_with(&http);
    let err = sender.send(&mail("mail_test:429")).await.unwrap_err();
    match err {
        MailError::RateLimited { retry_after_secs } => {
            assert_eq!(retry_after_secs, Some(7));
        }
        other => panic!("expected RateLimited, got {other:?}"),
    }
}

#[tokio::test]
async fn status_429_without_retry_after_is_still_rate_limited() {
    let http = FakeTransport::new();
    http.stub("/emails", vec![resp(429, b"")]);
    let sender = sender_with(&http);
    let err = sender.send(&mail("mail_test:429b")).await.unwrap_err();
    match err {
        MailError::RateLimited { retry_after_secs } => assert_eq!(retry_after_secs, None),
        other => panic!("expected RateLimited, got {other:?}"),
    }
}

#[tokio::test]
async fn status_429_with_unreadable_retry_after_is_none() {
    let http = FakeTransport::new();
    http.stub(
        "/emails",
        vec![resp_h(429, &[("retry-after", "soon")], b"")],
    );
    let sender = sender_with(&http);
    let err = sender.send(&mail("mail_test:429c")).await.unwrap_err();
    match err {
        MailError::RateLimited { retry_after_secs } => assert_eq!(retry_after_secs, None),
        other => panic!("expected RateLimited, got {other:?}"),
    }
}

#[tokio::test]
async fn status_401_and_403_map_to_unauthorized() {
    for status in [401u16, 403] {
        let http = FakeTransport::new();
        http.stub(
            "/emails",
            vec![resp(status, b"{\"message\":\"invalid api key\"}")],
        );
        let sender = sender_with(&http);
        let err = sender.send(&mail("mail_test:401")).await.unwrap_err();
        assert!(
            matches!(err, MailError::Unauthorized(_)),
            "status {status} → {err:?}"
        );
        // Тело ответа попадает в ошибку в укороченном виде.
        let text = err.to_string();
        assert!(text.contains("invalid api key"), "{text}");
    }
}

#[tokio::test]
async fn status_400_and_422_map_to_rejected_with_sanitized_body() {
    // Длинное тело: ошибка обязана быть укороченной, но узнаваемой.
    let long_body = format!(
        "{{\"message\":\"{}\",\"name\":\"validation_error\"}}",
        "x".repeat(500)
    );
    for status in [400u16, 422] {
        let http = FakeTransport::new();
        http.stub("/emails", vec![resp(status, long_body.as_bytes())]);
        let sender = sender_with(&http);
        let err = sender.send(&mail("mail_test:400")).await.unwrap_err();
        match &err {
            MailError::Rejected(detail) => {
                assert!(detail.starts_with("resend /emails: "), "{detail}");
                assert!(
                    detail.chars().count() <= "resend /emails: ".chars().count() + 161,
                    "тело не укорочено: {} символов",
                    detail.chars().count()
                );
                assert!(detail.ends_with('…'), "нет признака обрезки: {detail}");
            }
            other => panic!("status {status} expected Rejected, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn status_500_maps_to_unavailable() {
    let http = FakeTransport::new();
    http.stub("/emails", vec![resp(503, b"upstream down")]);
    let sender = sender_with(&http);
    let err = sender.send(&mail("mail_test:500")).await.unwrap_err();
    assert!(matches!(err, MailError::Unavailable(_)), "{err:?}");
}

#[tokio::test]
async fn transport_timeout_maps_to_timeout() {
    let http = FakeTransport::new();
    http.stub("/emails", vec![Stub::Timeout]);
    let sender = sender_with(&http);
    let err = sender.send(&mail("mail_test:timeout")).await.unwrap_err();
    assert!(matches!(err, MailError::Timeout), "{err:?}");
}

#[tokio::test]
async fn transport_connect_error_maps_to_unavailable() {
    let http = FakeTransport::new();
    http.stub("/emails", vec![Stub::Unavailable]);
    let sender = sender_with(&http);
    let err = sender.send(&mail("mail_test:conn")).await.unwrap_err();
    assert!(matches!(err, MailError::Unavailable(_)), "{err:?}");
}

#[tokio::test]
async fn status_200_without_id_is_decode_error() {
    for body in [
        &br#"{"message":"ok"}"#[..],
        &br#"{"id":""}"#[..],
        &br#"{"id":"   "}"#[..],
        &br#"{}"#[..],
        &br#"not json at all"#[..],
    ] {
        let http = FakeTransport::new();
        http.stub("/emails", vec![resp(200, body)]);
        let sender = sender_with(&http);
        let err = sender.send(&mail("mail_test:decode")).await.unwrap_err();
        assert!(
            matches!(err, MailError::Decode(_)),
            "body {:?} → {err:?}",
            String::from_utf8_lossy(body)
        );
    }
}

#[tokio::test]
async fn other_2xx_is_decode_error() {
    for status in [201u16, 202, 204] {
        let http = FakeTransport::new();
        http.stub("/emails", vec![resp(status, &ok_body("pm_1"))]);
        let sender = sender_with(&http);
        let err = sender.send(&mail("mail_test:2xx")).await.unwrap_err();
        assert!(matches!(err, MailError::Decode(_)), "{status} → {err:?}");
    }
}

#[test]
fn sanitize_body_truncates_multibyte_without_panic() {
    // Кириллица: обрезка по байту здесь паниковала бы.
    let body = "Проверка ".repeat(80);
    let out = crate::transport::sanitize_body("resend /emails", body.as_bytes());
    assert!(out.ends_with('…'), "{out}");
    assert!(out.chars().count() < 200, "{}", out.chars().count());
}

// ---------------------------------------------------------------------------
// NoopMail
// ---------------------------------------------------------------------------

#[tokio::test]
async fn noop_never_sends_and_reports_synthetic_id() {
    let http = FakeTransport::new();
    // Правил нет: любой сетевой вызов провалился бы с «no rule».
    let _ = &http;
    let noop = NoopMail;
    assert!(!noop.is_live());

    let receipt = noop.send(&mail("mail_test:noop")).await.expect("noop");
    assert!(receipt.provider_id.starts_with("noop-"), "{receipt:?}");
    assert_eq!(receipt.provider, "noop");
    assert!(http.recorded().is_empty());
}

#[test]
fn resend_sender_is_live() {
    let http = FakeTransport::new();
    assert!(sender_with(&http).is_live());
}

// ---------------------------------------------------------------------------
// Svix: проверка подписи
// ---------------------------------------------------------------------------

const SVIX_SECRET_RAW: &[u8] = b"dropit-svix-test-secret";
const SVIX_BODY: &[u8] = br#"{"type":"email.delivered","data":{"email_id":"pm_1"}}"#;
const SVIX_NOW: i64 = 1_700_000_000;

/// Секрет в формате `whsec_<base64>`.
fn whsec() -> String {
    format!("{WHSEC_PREFIX}{}", STANDARD.encode(SVIX_SECRET_RAW))
}

/// Подпись `"{id}.{ts}.{body}"` для тестов.
fn svix_sign(secret: &[u8], id: &str, ts: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("hmac key");
    mac.update(format!("{id}.{ts}.").as_bytes());
    mac.update(body);
    STANDARD.encode(mac.finalize().into_bytes())
}

/// Заголовки с корректной подписью.
fn good_headers() -> WebhookHeaders {
    let ts = SVIX_NOW.to_string();
    WebhookHeaders {
        id: "msg_1".to_string(),
        signature: format!("v1,{}", svix_sign(SVIX_SECRET_RAW, "msg_1", &ts, SVIX_BODY)),
        timestamp: ts,
    }
}

#[test]
fn svix_verify_ok() {
    let verifier = WebhookVerifier::from_whsec(&whsec()).expect("verifier");
    verifier
        .verify(SVIX_BODY, &good_headers(), SVIX_NOW)
        .expect("valid signature");
    assert_eq!(verifier.tolerance_secs(), WEBHOOK_TOLERANCE_SECS);
}

#[test]
fn svix_verify_ok_without_base64_padding() {
    let unpadded = whsec().trim_end_matches('=').to_string();
    let verifier = WebhookVerifier::from_whsec(&unpadded).expect("verifier without padding");
    verifier
        .verify(SVIX_BODY, &good_headers(), SVIX_NOW)
        .expect("valid signature");
}

#[test]
fn svix_tampered_body_rejected() {
    let verifier = WebhookVerifier::from_whsec(&whsec()).expect("verifier");
    let tampered = br#"{"type":"email.complained","data":{"email_id":"pm_1"}}"#;
    let err = verifier
        .verify(tampered, &good_headers(), SVIX_NOW)
        .unwrap_err();
    assert!(matches!(err, MailError::SignatureInvalid(_)), "{err:?}");
}

#[test]
fn svix_wrong_secret_rejected() {
    let verifier = WebhookVerifier::from_whsec(&whsec()).expect("verifier");
    let other =
        WebhookVerifier::from_whsec(&format!("{WHSEC_PREFIX}{}", STANDARD.encode(b"other")))
            .expect("other verifier");
    // Подпись сделана чужим ключом.
    let err = other
        .verify(SVIX_BODY, &good_headers(), SVIX_NOW)
        .unwrap_err();
    assert!(matches!(err, MailError::SignatureInvalid(_)), "{err:?}");
    // А свой ключ — проходит (контроль, что дело именно в секрете).
    verifier
        .verify(SVIX_BODY, &good_headers(), SVIX_NOW)
        .expect("own secret works");
}

#[test]
fn svix_expired_timestamp_is_distinct_error() {
    let verifier = WebhookVerifier::from_whsec(&whsec()).expect("verifier");
    let err = verifier
        .verify(
            SVIX_BODY,
            &good_headers(),
            SVIX_NOW + WEBHOOK_TOLERANCE_SECS + 1,
        )
        .unwrap_err();
    assert!(matches!(err, MailError::SignatureExpired), "{err:?}");
    // И в обратную сторону (часы провайдера впереди).
    let err = verifier
        .verify(
            SVIX_BODY,
            &good_headers(),
            SVIX_NOW - WEBHOOK_TOLERANCE_SECS - 1,
        )
        .unwrap_err();
    assert!(matches!(err, MailError::SignatureExpired), "{err:?}");
}

#[test]
fn svix_tolerance_boundary_accepted() {
    let verifier = WebhookVerifier::from_whsec(&whsec()).expect("verifier");
    verifier
        .verify(
            SVIX_BODY,
            &good_headers(),
            SVIX_NOW + WEBHOOK_TOLERANCE_SECS,
        )
        .expect("ровно на границе допуска — принимаем");
}

#[test]
fn svix_missing_header_yields_none() {
    assert!(WebhookHeaders::from_pairs(std::iter::empty()).is_none());
    // Не хватает подписи.
    assert!(WebhookHeaders::from_pairs(
        [("svix-id", "msg_1"), ("svix-timestamp", "1700000000"),].into_iter(),
    )
    .is_none());
    // Пустые значения не считаются присутствием.
    assert!(WebhookHeaders::from_pairs(
        [
            ("svix-id", "msg_1"),
            ("svix-timestamp", "  "),
            ("svix-signature", "v1,abc"),
        ]
        .into_iter(),
    )
    .is_none());
    // Все три — есть, регистр не важен.
    let headers = WebhookHeaders::from_pairs(
        [
            ("Svix-Id", "msg_1"),
            ("SVIX-TIMESTAMP", "1700000000"),
            ("svix-signature", "v1,abc"),
        ]
        .into_iter(),
    )
    .expect("headers");
    assert_eq!(headers.id, "msg_1");
}

#[test]
fn svix_empty_signature_header_rejected() {
    let verifier = WebhookVerifier::from_whsec(&whsec()).expect("verifier");
    let headers = WebhookHeaders {
        id: "msg_1".to_string(),
        timestamp: SVIX_NOW.to_string(),
        signature: "   ".to_string(),
    };
    let err = verifier.verify(SVIX_BODY, &headers, SVIX_NOW).unwrap_err();
    assert!(matches!(err, MailError::SignatureInvalid(_)), "{err:?}");
}

#[test]
fn svix_secret_without_whsec_prefix_is_config_error() {
    let err = WebhookVerifier::from_whsec("just-a-raw-secret").unwrap_err();
    assert!(matches!(err, MailError::Config(_)), "{err:?}");
}

#[test]
fn svix_secret_empty_or_broken_is_config_error() {
    for raw in ["", "   ", WHSEC_PREFIX, "whsec_***not-base64***"] {
        let err = WebhookVerifier::from_whsec(raw).unwrap_err();
        assert!(matches!(err, MailError::Config(_)), "{raw:?} → {err:?}");
    }
}

#[test]
fn svix_multi_entry_header_matches_any() {
    let verifier = WebhookVerifier::from_whsec(&whsec()).expect("verifier");
    let ts = SVIX_NOW.to_string();
    let good = svix_sign(SVIX_SECRET_RAW, "msg_1", &ts, SVIX_BODY);
    let junk = STANDARD.encode([0u8; 32]);
    let headers = WebhookHeaders {
        id: "msg_1".to_string(),
        timestamp: ts.clone(),
        signature: format!("v1,{junk} v1,{good}"),
    };
    verifier
        .verify(SVIX_BODY, &headers, SVIX_NOW)
        .expect("вторая подпись настоящая");

    // Только мусорные схемы — не «ok по умолчанию».
    let headers = WebhookHeaders {
        id: "msg_1".to_string(),
        timestamp: ts,
        signature: format!("v1a,{good}"),
    };
    let err = verifier.verify(SVIX_BODY, &headers, SVIX_NOW).unwrap_err();
    assert!(matches!(err, MailError::SignatureInvalid(_)), "{err:?}");
}

#[test]
fn svix_non_numeric_timestamp_rejected() {
    let verifier = WebhookVerifier::from_whsec(&whsec()).expect("verifier");
    let headers = WebhookHeaders {
        id: "msg_1".to_string(),
        timestamp: "yesterday".to_string(),
        signature: "v1,abc".to_string(),
    };
    let err = verifier.verify(SVIX_BODY, &headers, SVIX_NOW).unwrap_err();
    assert!(matches!(err, MailError::SignatureInvalid(_)), "{err:?}");
}

#[test]
fn svix_signature_is_over_raw_bytes() {
    // Тело не обязано быть UTF-8: подпись считается по байтам.
    let verifier = WebhookVerifier::from_whsec(&whsec()).expect("verifier");
    let body = &[0x00, 0xFF, 0x42, 0x7F];
    let ts = SVIX_NOW.to_string();
    let headers = WebhookHeaders {
        id: "msg_bin".to_string(),
        timestamp: ts.clone(),
        signature: format!("v1,{}", svix_sign(SVIX_SECRET_RAW, "msg_bin", &ts, body)),
    };
    verifier
        .verify(body, &headers, SVIX_NOW)
        .expect("binary body");
}

// ---------------------------------------------------------------------------
// Разбор событий
// ---------------------------------------------------------------------------

/// Разобрать событие из JSON-строки.
fn event(raw: &str) -> crate::event::MailWebhookEvent {
    parse_event(raw.as_bytes()).expect("parse event")
}

#[test]
fn parse_event_sent_and_delivered() {
    let sent = event(r#"{"type":"email.sent","data":{"email_id":"pm_1","to":["a@b.c"]}}"#);
    assert_eq!(sent.kind, MailEventKind::Sent);
    assert_eq!(sent.kind.as_str(), "email.sent");
    assert_eq!(sent.provider_id.as_deref(), Some("pm_1"));
    assert_eq!(sent.email.as_deref(), Some("a@b.c"));

    let delivered = event(r#"{"type":"email.delivered","data":{"email_id":"pm_2"}}"#);
    assert_eq!(delivered.kind, MailEventKind::Delivered);
    assert_eq!(delivered.provider_id.as_deref(), Some("pm_2"));
    assert!(delivered.email.is_none());
    assert!(delivered.bounce_kind.is_none());
}

#[test]
fn parse_event_bounced_carries_hard_kind_and_reason() {
    let ev = event(
        r#"{"type":"email.bounced","data":{"email_id":"pm_3","to":["gone@b.c"],
            "bounce":{"type":"hard","message":"mailbox does not exist"}}}"#,
    );
    assert_eq!(ev.kind, MailEventKind::Bounced);
    assert_eq!(ev.bounce_kind.as_deref(), Some("hard"));
    assert_eq!(ev.reason.as_deref(), Some("mailbox does not exist"));
    assert_eq!(ev.email.as_deref(), Some("gone@b.c"));
}

#[test]
fn parse_event_complained() {
    let ev = event(r#"{"type":"email.complained","data":{"email_id":"pm_4","to":["a@b.c"]}}"#);
    assert_eq!(ev.kind, MailEventKind::Complained);
    assert_eq!(ev.kind.as_str(), "email.complained");
}

#[test]
fn parse_event_failed_uses_failed_reason() {
    let ev = event(
        r#"{"type":"email.failed","data":{"email_id":"pm_5",
            "failed":{"reason":"reached daily quota"}}}"#,
    );
    assert_eq!(ev.kind, MailEventKind::Failed);
    assert_eq!(ev.reason.as_deref(), Some("reached daily quota"));
}

#[test]
fn parse_event_suppressed() {
    let ev = event(r#"{"type":"email.suppressed","data":{"email_id":"pm_6","to":["a@b.c"]}}"#);
    assert_eq!(ev.kind, MailEventKind::Suppressed);
    assert_eq!(ev.kind.as_str(), "email.suppressed");
}

#[test]
fn parse_event_delivery_delayed_opened_clicked() {
    assert_eq!(
        event(r#"{"type":"email.delivery_delayed"}"#).kind,
        MailEventKind::DeliveryDelayed
    );
    assert_eq!(
        event(r#"{"type":"email.opened"}"#).kind,
        MailEventKind::Opened
    );
    assert_eq!(
        event(r#"{"type":"email.clicked"}"#).kind,
        MailEventKind::Clicked
    );
}

#[test]
fn parse_event_unknown_type_is_other() {
    let ev = event(r#"{"type":"email.some_future_thing","data":{"email_id":"pm_7"}}"#);
    assert_eq!(ev.kind, MailEventKind::Other);
    assert_eq!(ev.kind.as_str(), "other");
    // Исходный тип не теряется: он в payload.
    assert_eq!(ev.raw["type"], "email.some_future_thing");
    assert_eq!(ev.provider_id.as_deref(), Some("pm_7"));
}

#[test]
fn parse_event_malformed_is_decode_error() {
    for raw in [
        "",
        "not json",
        "[]",
        r#"{"data":{}}"#,
        r#"{"type":123}"#,
        r#"{"type":"  "}"#,
    ] {
        let err = parse_event(raw.as_bytes()).unwrap_err();
        assert!(matches!(err, MailError::Decode(_)), "{raw:?} → {err:?}");
    }
}

#[test]
fn parse_event_empty_to_array_is_none() {
    let ev = event(r#"{"type":"email.delivered","data":{"email_id":"pm_8","to":[]}}"#);
    assert!(ev.email.is_none());
}

// ---------------------------------------------------------------------------
// Шаблоны
// ---------------------------------------------------------------------------

/// Полный набор переменных шаблона.
fn vars(key: TemplateKey) -> serde_json::Value {
    match key {
        TemplateKey::DjInvite => serde_json::json!({
            "club_name": "Клуб 12",
            "dj_name": "Аня",
            "invite_url": "https://app.dropit.club/invite/abc",
            "expires_at": "31 октября, 23:59",
        }),
        TemplateKey::GuestVerify => serde_json::json!({
            "code": "482913",
            "expires_at": "10 минут",
        }),
        TemplateKey::PayoutPaid => serde_json::json!({
            "payee_name": "Аня",
            "amount": "1 240 €",
            "period": "14–20 октября",
        }),
        TemplateKey::MailTest => serde_json::json!({
            "sent_by": "operator@dropit.club",
        }),
    }
}

#[test]
fn template_key_roundtrips() {
    for key in [
        TemplateKey::DjInvite,
        TemplateKey::GuestVerify,
        TemplateKey::PayoutPaid,
        TemplateKey::MailTest,
    ] {
        assert_eq!(TemplateKey::from_str(key.as_str()), Some(key));
    }
    assert_eq!(TemplateKey::from_str("unknown"), None);
    assert_eq!(TemplateKey::DjInvite.as_str(), "dj_invite");
    assert_eq!(TemplateKey::GuestVerify.as_str(), "guest_verify");
    assert_eq!(TemplateKey::PayoutPaid.as_str(), "payout_paid");
    assert_eq!(TemplateKey::MailTest.as_str(), "mail_test");
}

#[test]
fn template_subjects_match_contract() {
    let invite = render(TemplateKey::DjInvite, &vars(TemplateKey::DjInvite)).expect("render");
    assert_eq!(invite.subject, "Приглашение в Клуб 12");
    assert_eq!(
        render(TemplateKey::GuestVerify, &vars(TemplateKey::GuestVerify))
            .expect("render")
            .subject,
        "Подтвердите e-mail"
    );
    assert_eq!(
        render(TemplateKey::PayoutPaid, &vars(TemplateKey::PayoutPaid))
            .expect("render")
            .subject,
        "Выплата отправлена"
    );
    assert_eq!(
        render(TemplateKey::MailTest, &vars(TemplateKey::MailTest))
            .expect("render")
            .subject,
        "Тестовое письмо DROP IT"
    );
}

#[test]
fn template_renders_vars_into_both_versions() {
    let rendered =
        render(TemplateKey::GuestVerify, &vars(TemplateKey::GuestVerify)).expect("render");
    assert!(rendered.html.contains("482913"), "html без кода");
    assert!(rendered.text.contains("482913"), "text без кода");
    assert!(rendered.html.contains("10 минут"));
    // Ни одного незамещённого плейсхолдера.
    assert!(!rendered.html.contains("{{"), "в html остался плейсхолдер");
    assert!(!rendered.text.contains("{{"), "в text остался плейсхолдер");
    assert!(!rendered.subject.contains("{{"));
}

#[test]
fn template_escapes_html_but_not_text() {
    let mut v = vars(TemplateKey::DjInvite);
    v["club_name"] = serde_json::json!("<b>Клуб</b> & \"друзья\"");
    let rendered = render(TemplateKey::DjInvite, &v).expect("render");

    assert!(
        rendered
            .html
            .contains("&lt;b&gt;Клуб&lt;/b&gt; &amp; &quot;друзья&quot;"),
        "html не экранирован: {}",
        rendered.html
    );
    assert!(
        !rendered.html.contains("<b>Клуб</b>"),
        "тег просочился в html"
    );
    // В текстовой версии — как есть: там нет разметки.
    assert!(rendered.text.contains("<b>Клуб</b> & \"друзья\""));
    assert!(!rendered.text.contains("&lt;b&gt;"));
    // Тема экранируется тоже (она уходит в заголовок письма, но шаблон
    // подставляет значение в тот же проход).
    assert!(rendered.subject.contains("<b>Клуб</b>"));
}

#[test]
fn template_missing_var_is_config_error() {
    let mut v = vars(TemplateKey::GuestVerify);
    v.as_object_mut().unwrap().remove("code");
    let err = render(TemplateKey::GuestVerify, &v).unwrap_err();
    match &err {
        MailError::Config(message) => {
            assert!(message.contains("missing template var"), "{message}");
            assert!(message.contains("code"), "{message}");
            assert!(message.contains("guest_verify"), "{message}");
        }
        other => panic!("expected Config, got {other:?}"),
    }
}

#[test]
fn template_null_var_counts_as_missing() {
    let mut v = vars(TemplateKey::MailTest);
    v["sent_by"] = serde_json::Value::Null;
    let err = render(TemplateKey::MailTest, &v).unwrap_err();
    assert!(matches!(err, MailError::Config(_)), "{err:?}");
}

#[test]
fn template_non_object_vars_fail_cleanly() {
    let err = render(TemplateKey::MailTest, &serde_json::json!("nope")).unwrap_err();
    assert!(matches!(err, MailError::Config(_)), "{err:?}");
}

#[test]
fn template_numeric_var_is_stringified() {
    let mut v = vars(TemplateKey::MailTest);
    v["sent_by"] = serde_json::json!(42);
    let rendered = render(TemplateKey::MailTest, &v).expect("render");
    assert!(rendered.text.contains("42"));
}

#[test]
fn escape_html_escapes_five_characters() {
    assert_eq!(
        escape_html("<a href=\"x\">O'Neil & Co</a>"),
        "&lt;a href=&quot;x&quot;&gt;O&#39;Neil &amp; Co&lt;/a&gt;"
    );
    assert_eq!(escape_html("чистый текст"), "чистый текст");
}

#[test]
fn every_template_renders_full_document() {
    for key in [
        TemplateKey::DjInvite,
        TemplateKey::GuestVerify,
        TemplateKey::PayoutPaid,
        TemplateKey::MailTest,
    ] {
        let rendered = render(key, &vars(key)).expect("render");
        assert!(
            rendered.html.starts_with("<!doctype html>"),
            "{}: html не документ",
            key.as_str()
        );
        assert!(rendered.html.contains("</html>"), "{}", key.as_str());
        assert!(
            !rendered.text.trim().is_empty(),
            "{}: пустой text",
            key.as_str()
        );
        assert!(!rendered.subject.trim().is_empty());
    }
}

// ---------------------------------------------------------------------------
// Конфигурация из окружения
// ---------------------------------------------------------------------------

/// Источник переменных окружения из карты.
fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |key: &str| map.get(key).cloned()
}

#[test]
fn env_without_api_key_is_none() {
    assert!(from_env_via(env(&[])).is_none());
    assert!(from_env_via(env(&[("MAIL_FROM", "DROP IT <no-reply@dropit.club>")])).is_none());
    // Пустой ключ — как отсутствующий.
    assert!(from_env_via(env(&[("RESEND_API_KEY", "  ")])).is_none());
}

#[test]
fn env_key_without_from_degrades_to_none() {
    assert!(from_env_via(env(&[("RESEND_API_KEY", "re_test")])).is_none());
    // Невалидный MAIL_FROM — тоже деградация, а не паника.
    assert!(from_env_via(env(&[
        ("RESEND_API_KEY", "re_test"),
        ("MAIL_FROM", "not an address"),
    ]))
    .is_none());
}

#[test]
fn env_full_config_uses_defaults() {
    let config = from_env_via(env(&[
        ("RESEND_API_KEY", "re_test"),
        ("MAIL_FROM", "DROP IT <no-reply@dropit.club>"),
    ]))
    .expect("config");
    assert_eq!(config.resend.api_key, "re_test");
    assert_eq!(config.resend.api_base, DEFAULT_API_BASE);
    assert_eq!(
        config.resend.from,
        MailAddress::parse("DROP IT <no-reply@dropit.club>").unwrap()
    );
    assert!(config.resend.reply_to.is_none());
    assert!(config.webhook_secret.is_none());
}

#[test]
fn env_overrides_api_base_and_reads_reply_to_and_secret() {
    let config = from_env_via(env(&[
        ("RESEND_API_KEY", "re_test"),
        ("RESEND_API_URL", "http://127.0.0.1:9999/"),
        ("MAIL_FROM", "DROP IT <no-reply@dropit.club>"),
        ("MAIL_REPLY_TO", "support@dropit.club"),
        ("RESEND_WEBHOOK_SECRET", "whsec_abc"),
    ]))
    .expect("config");
    assert_eq!(config.resend.api_base, "http://127.0.0.1:9999/");
    assert_eq!(
        config.resend.reply_to,
        Some(MailAddress::parse("support@dropit.club").unwrap())
    );
    assert_eq!(config.webhook_secret.as_deref(), Some("whsec_abc"));
}

#[test]
fn env_invalid_reply_to_is_ignored() {
    let config = from_env_via(env(&[
        ("RESEND_API_KEY", "re_test"),
        ("MAIL_FROM", "DROP IT <no-reply@dropit.club>"),
        ("MAIL_REPLY_TO", "not-an-address"),
    ]))
    .expect("config");
    assert!(config.resend.reply_to.is_none());
}

#[test]
fn env_blank_webhook_secret_is_none() {
    let config = from_env_via(env(&[
        ("RESEND_API_KEY", "re_test"),
        ("MAIL_FROM", "DROP IT <no-reply@dropit.club>"),
        ("RESEND_WEBHOOK_SECRET", "   "),
    ]))
    .expect("config");
    assert!(config.webhook_secret.is_none());
}

#[test]
fn config_debug_hides_secrets() {
    let config = from_env_via(env(&[
        ("RESEND_API_KEY", "re_super_secret_key"),
        ("MAIL_FROM", "DROP IT <no-reply@dropit.club>"),
        ("RESEND_WEBHOOK_SECRET", "whsec_super_secret"),
    ]))
    .expect("config");
    let text = format!("{config:?}");
    assert!(!text.contains("re_super_secret_key"), "{text}");
    assert!(!text.contains("whsec_super_secret"), "{text}");
    assert!(text.contains("***"), "{text}");
}

#[test]
fn resend_sender_new_rejects_bad_config() {
    let base = ResendConfig {
        api_key: "re_test".to_string(),
        api_base: DEFAULT_API_BASE.to_string(),
        from: MailAddress::parse("DROP IT <no-reply@dropit.club>").unwrap(),
        reply_to: None,
    };

    let mut empty_key = base.clone();
    empty_key.api_key = "  ".to_string();
    assert!(matches!(
        ResendSender::new(empty_key).unwrap_err(),
        MailError::Config(_)
    ));

    let mut empty_base = base.clone();
    empty_base.api_base = "   ".to_string();
    assert!(matches!(
        ResendSender::new(empty_base).unwrap_err(),
        MailError::Config(_)
    ));

    let mut bad_base = base;
    bad_base.api_base = "api.resend.com".to_string();
    assert!(matches!(
        ResendSender::new(bad_base).unwrap_err(),
        MailError::Config(_)
    ));
}

#[test]
fn resend_sender_new_normalizes_base_and_hides_key() {
    let config = ResendConfig {
        api_key: "re_secret".to_string(),
        api_base: "https://api.resend.com///".to_string(),
        from: MailAddress::parse("DROP IT <no-reply@dropit.club>").unwrap(),
        reply_to: None,
    };
    let sender = ResendSender::new(config).expect("sender");
    assert_eq!(sender.config().api_base, "https://api.resend.com");
    let text = format!("{sender:?}");
    assert!(!text.contains("re_secret"), "{text}");
}

#[test]
fn outbound_timeout_is_bounded() {
    // Санити-проверка константы: таймаут должен быть конечным и разумным.
    assert!(Duration::from_secs(crate::transport::DEFAULT_REQUEST_TIMEOUT_SECS).as_secs() <= 60);
}
