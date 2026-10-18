//! Unit-тесты крейта `dropit-infra-push`.
//!
//! Все тесты детерминированы и не выполняют сетевых вызовов: транспорт
//! подменяется фиктивным [`FakeTransport`], ключи загружаются из фикстур
//! `tests/fixtures/`.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde_json::Value;

use crate::apns::ApnsSender;
use crate::fcm::FcmSender;
use crate::transport::{HttpRequest, HttpResponse, HttpTransport};
use crate::{
    ApnsConfig, FcmConfig, NoopPush, Platform, PushConfig, PushError, PushMessage, PushSender,
    PushToken, RoutingPush,
};

/// Фикстура RSA-ключа для FCM (RS256).
fn rsa_pem() -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/test_rsa.pem");
    std::fs::read_to_string(path).expect("read test_rsa.pem")
}

/// Фикстура EC P-256 ключа для APNs (ES256).
fn ec_p8() -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/test_key.p8");
    std::fs::read_to_string(path).expect("read test_key.p8")
}

/// Сервисный аккаунт Google для тестов FCM.
fn service_account_json(project_id: &str, token_uri: &str) -> String {
    serde_json::json!({
        "type": "service_account",
        "project_id": project_id,
        "client_email": format!("firebase-adminsdk@{project_id}.iam.gserviceaccount.com"),
        "private_key": rsa_pem(),
        "token_uri": token_uri,
    })
    .to_string()
}

/// Запись запроса, перехваченного фейк-транспортом.
#[derive(Debug, Clone)]
struct RecordedRequest {
    url: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    http2: bool,
}

/// Один запланированный ответ для URL, содержащего `match_sub`.
#[derive(Debug, Clone)]
struct StubResponse {
    status: u16,
    body: Vec<u8>,
}

/// Фиктивный HTTP-транспорт: отвечает по правилам вида «URL содержит X».
struct FakeTransport {
    rules: Mutex<Vec<FakeRule>>,
    calls: Mutex<Vec<RecordedRequest>>,
}

struct FakeRule {
    match_sub: String,
    responses: Mutex<VecDeque<StubResponse>>,
}

impl FakeTransport {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            rules: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
        })
    }

    /// Зарегистрировать очередь ответов для URL, содержащих `match_sub`.
    fn stub(self: &Arc<Self>, match_sub: &str, responses: Vec<StubResponse>) {
        self.rules.lock().unwrap().push(FakeRule {
            match_sub: match_sub.to_string(),
            responses: Mutex::new(responses.into()),
        });
    }

    /// Все перехваченные запросы.
    fn recorded(&self) -> Vec<RecordedRequest> {
        self.calls.lock().unwrap().clone()
    }

    /// Запросы, чей URL содержит `sub`.
    fn recorded_for(&self, sub: &str) -> Vec<RecordedRequest> {
        self.recorded()
            .into_iter()
            .filter(|r| r.url.contains(sub))
            .collect()
    }
}

#[async_trait]
impl HttpTransport for FakeTransport {
    async fn execute(&self, req: HttpRequest) -> Result<HttpResponse, PushError> {
        self.calls.lock().unwrap().push(RecordedRequest {
            url: req.url.clone(),
            headers: req.headers.clone(),
            body: req.body.clone(),
            http2: req.http2,
        });
        let rules = self.rules.lock().unwrap();
        for rule in rules.iter() {
            if req.url.contains(&rule.match_sub) {
                let mut queue = rule.responses.lock().unwrap();
                if let Some(resp) = queue.pop_front() {
                    return Ok(HttpResponse {
                        status: resp.status,
                        body: resp.body,
                    });
                }
                return Err(PushError::Unavailable(format!(
                    "fake transport: no responses left for {}",
                    rule.match_sub
                )));
            }
        }
        Err(PushError::Unavailable(format!(
            "fake transport: no stub for url {}",
            req.url
        )))
    }
}

fn ok_body() -> Vec<u8> {
    b"{\"name\":\"projects/dropit-prod/messages/123\"}".to_vec()
}

fn apns_ok_body() -> Vec<u8> {
    Vec::new()
}

fn resp(status: u16, body: &[u8]) -> StubResponse {
    StubResponse {
        status,
        body: body.to_vec(),
    }
}

fn token_resp(token: &str, expires_in: u64) -> Vec<u8> {
    serde_json::json!({
        "access_token": token,
        "expires_in": expires_in,
        "token_type": "Bearer",
    })
    .to_string()
    .into_bytes()
}

/// Декодировать header/payload JWT без проверки подписи.
fn jwt_parts(token: &str) -> (Value, Value) {
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3, "jwt must have 3 parts");
    let header = URL_SAFE_NO_PAD.decode(parts[0]).expect("decode header");
    let payload = URL_SAFE_NO_PAD.decode(parts[1]).expect("decode payload");
    let header: Value = serde_json::from_slice(&header).expect("header json");
    let payload: Value = serde_json::from_slice(&payload).expect("payload json");
    (header, payload)
}

fn msg(title: &str, body: &str) -> PushMessage {
    PushMessage {
        title: title.to_string(),
        body: body.to_string(),
        data: Default::default(),
        collapse_key: None,
        route: None,
    }
}

fn android_token(t: &str) -> PushToken {
    PushToken {
        token: t.to_string(),
        platform: Platform::Android,
    }
}

fn ios_token(t: &str) -> PushToken {
    PushToken {
        token: t.to_string(),
        platform: Platform::Ios,
    }
}

// ---------------------------------------------------------------------------
// NoopPush
// ---------------------------------------------------------------------------

#[tokio::test]
async fn noop_empty_returns_zeroes() {
    let report = NoopPush.send(&[], msg("t", "b")).await.expect("noop");
    assert_eq!(report.sent, 0);
    assert_eq!(report.failed, 0);
    assert!(report.invalid_tokens.is_empty());
    assert!(report.outcomes.is_empty());
}

#[tokio::test]
async fn noop_nonempty_outcomes_per_token() {
    let tokens = [android_token("a1"), android_token("a2")];
    let report = NoopPush.send(&tokens, msg("t", "b")).await.expect("noop");
    assert_eq!(report.sent, 2);
    assert_eq!(report.outcomes.len(), 2);
    assert!(report.outcomes.iter().all(|o| o.ok));
}

// ---------------------------------------------------------------------------
// FCM: конфигурация
// ---------------------------------------------------------------------------

#[test]
fn fcm_new_rejects_invalid_json() {
    let err = FcmSender::new("proj".into(), "not json".into()).unwrap_err();
    assert!(matches!(err, PushError::Config(_)), "{err:?}");
}

#[test]
fn fcm_new_rejects_missing_client_email() {
    let json = serde_json::json!({"private_key": rsa_pem()}).to_string();
    let err = FcmSender::new("proj".into(), json).unwrap_err();
    assert!(matches!(err, PushError::Config(_)), "{err:?}");
}

#[test]
fn fcm_new_rejects_missing_private_key() {
    let json = serde_json::json!({"client_email": "x@y.gserviceaccount.com"}).to_string();
    let err = FcmSender::new("proj".into(), json).unwrap_err();
    assert!(matches!(err, PushError::Config(_)), "{err:?}");
}

#[test]
fn fcm_new_rejects_bad_private_key() {
    let json = serde_json::json!({
        "client_email": "x@y.gserviceaccount.com",
        "private_key": "not a pem",
    })
    .to_string();
    let err = FcmSender::new("proj".into(), json).unwrap_err();
    assert!(matches!(err, PushError::Config(_)), "{err:?}");
}

#[test]
fn fcm_new_rejects_invalid_project_id_path() {
    let json = service_account_json("dropit-prod", "https://oauth2.googleapis.com/token");
    let err = FcmSender::new("dropit/prod".into(), json).unwrap_err();
    assert!(matches!(err, PushError::Config(_)), "{err:?}");
}

#[tokio::test]
async fn fcm_new_project_id_falls_back_to_json() {
    let http = FakeTransport::new();
    let json = service_account_json("dropit-prod", "https://oauth2.googleapis.com/token");
    let sender = FcmSender::with_transport(String::new(), json, http.clone())
        .expect("project_id fallback from json");
    // Косвенная проверка: отправка строит URL с проектом из JSON.
    http.stub(
        "oauth2.googleapis.com/token",
        vec![resp(200, &token_resp("tok", 3600))],
    );
    http.stub("messages:send", vec![resp(200, &ok_body())]);
    let report = sender
        .send(&[android_token("reg-1")], msg("t", "b"))
        .await
        .unwrap();
    assert_eq!(report.sent, 1);
    let send_url = http.recorded_for("messages:send")[0].url.clone();
    assert!(
        send_url.contains("/projects/dropit-prod/messages:send"),
        "{send_url}"
    );
}

// ---------------------------------------------------------------------------
// FCM: OAuth2 JWT (RS256)
// ---------------------------------------------------------------------------

#[test]
fn fcm_assertion_claims_and_header() {
    let http = FakeTransport::new();
    let json = service_account_json("dropit-prod", "https://oauth2.googleapis.com/token");
    let sender = FcmSender::with_transport("dropit-prod".into(), json, http).unwrap();
    let assertion = sender.build_assertion().expect("assertion");
    let (header, claims) = jwt_parts(&assertion);
    assert_eq!(header["alg"], "RS256");
    assert_eq!(header["typ"], "JWT");
    assert_eq!(
        claims["iss"],
        "firebase-adminsdk@dropit-prod.iam.gserviceaccount.com"
    );
    assert_eq!(
        claims["scope"],
        "https://www.googleapis.com/auth/firebase.messaging"
    );
    assert_eq!(claims["aud"], "https://oauth2.googleapis.com/token");
    let iat = claims["iat"].as_i64().expect("iat");
    let exp = claims["exp"].as_i64().expect("exp");
    assert!(exp > iat);
    assert!(exp - iat <= 3600);
}

// ---------------------------------------------------------------------------
// FCM: обмен токена и кеш
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fcm_token_request_form_and_caching() {
    let http = FakeTransport::new();
    http.stub(
        "oauth2.googleapis.com/token",
        vec![
            resp(200, &token_resp("access-1", 3600)),
            resp(200, &token_resp("access-2", 3600)),
        ],
    );
    http.stub(
        "messages:send",
        vec![resp(200, &ok_body()), resp(200, &ok_body())],
    );
    let json = service_account_json("dropit-prod", "https://oauth2.googleapis.com/token");
    let sender = FcmSender::with_transport("dropit-prod".into(), json, http.clone()).unwrap();

    let r1 = sender
        .send(&[android_token("a")], msg("t", "b"))
        .await
        .unwrap();
    let r2 = sender
        .send(&[android_token("b")], msg("t", "b"))
        .await
        .unwrap();
    assert_eq!(r1.sent + r2.sent, 2);

    let token_calls = http.recorded_for("oauth2.googleapis.com/token");
    assert_eq!(token_calls.len(), 1, "access token must be cached");
    let body = String::from_utf8_lossy(&token_calls[0].body);
    assert!(body.contains("grant_type=urn:ietf:params:oauth:grant-type:jwt-bearer"));
    assert!(body.contains("assertion="));
    assert_eq!(
        token_calls[0]
            .headers
            .iter()
            .find(|(k, _)| k == "Content-Type")
            .map(|(_, v)| v.as_str()),
        Some("application/x-www-form-urlencoded")
    );

    let send_calls = http.recorded_for("messages:send");
    assert_eq!(send_calls.len(), 2);
    assert_eq!(
        send_calls[0]
            .headers
            .iter()
            .find(|(k, _)| k == "Authorization")
            .map(|(_, v)| v.clone()),
        Some("Bearer access-1".to_string())
    );
}

#[tokio::test]
async fn fcm_token_refreshed_when_expired() {
    let http = FakeTransport::new();
    http.stub(
        "oauth2.googleapis.com/token",
        vec![
            resp(200, &token_resp("access-1", 30)),
            resp(200, &token_resp("access-2", 30)),
        ],
    );
    http.stub(
        "messages:send",
        vec![resp(200, &ok_body()), resp(200, &ok_body())],
    );
    let json = service_account_json("dropit-prod", "https://oauth2.googleapis.com/token");
    let sender = FcmSender::with_transport("dropit-prod".into(), json, http.clone()).unwrap();

    sender
        .send(&[android_token("a")], msg("t", "b"))
        .await
        .unwrap();
    sender
        .send(&[android_token("b")], msg("t", "b"))
        .await
        .unwrap();

    let token_calls = http.recorded_for("oauth2.googleapis.com/token");
    assert_eq!(token_calls.len(), 2, "expired token must be refreshed");
    let send_calls = http.recorded_for("messages:send");
    let auths: Vec<String> = send_calls
        .iter()
        .map(|r| {
            r.headers
                .iter()
                .find(|(k, _)| k == "Authorization")
                .map(|(_, v)| v.clone())
                .unwrap()
        })
        .collect();
    assert_ne!(auths[0], auths[1]);
}

#[tokio::test]
async fn fcm_token_endpoint_unauthorized() {
    let http = FakeTransport::new();
    http.stub(
        "oauth2.googleapis.com/token",
        vec![resp(401, b"invalid_grant")],
    );
    let json = service_account_json("dropit-prod", "https://oauth2.googleapis.com/token");
    let sender = FcmSender::with_transport("dropit-prod".into(), json, http).unwrap();
    let err = sender
        .send(&[android_token("a")], msg("t", "b"))
        .await
        .unwrap_err();
    assert!(matches!(err, PushError::Unauthorized(_)), "{err:?}");
}

// ---------------------------------------------------------------------------
// FCM: тело сообщения
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fcm_message_body_structure() {
    let http = FakeTransport::new();
    http.stub(
        "oauth2.googleapis.com/token",
        vec![resp(200, &token_resp("tok", 3600))],
    );
    http.stub("messages:send", vec![resp(200, &ok_body())]);
    let json = service_account_json("dropit-prod", "https://oauth2.googleapis.com/token");
    let sender = FcmSender::with_transport("dropit-prod".into(), json, http.clone()).unwrap();

    let mut data = std::collections::BTreeMap::new();
    data.insert("order_id".to_string(), "42".to_string());
    let message = PushMessage {
        title: "Запрос принят".into(),
        body: "Трек в очереди".into(),
        data,
        collapse_key: Some("requests".into()),
        route: Some("request/42".into()),
    };
    let report = sender
        .send(&[android_token("reg-token")], message)
        .await
        .unwrap();
    assert_eq!(report.sent, 1);

    let req = &http.recorded_for("messages:send")[0];
    assert_eq!(
        req.headers
            .iter()
            .find(|(k, _)| k == "Authorization")
            .map(|(_, v)| v.as_str()),
        Some("Bearer tok")
    );
    let body: Value = serde_json::from_slice(&req.body).unwrap();
    let message = &body["message"];
    assert_eq!(message["token"], "reg-token");
    assert_eq!(message["notification"]["title"], "Запрос принят");
    assert_eq!(message["notification"]["body"], "Трек в очереди");
    assert_eq!(message["data"]["order_id"], "42");
    assert_eq!(message["data"]["route"], "request/42");
    assert_eq!(message["android"]["collapse_key"], "requests");
    assert_eq!(message["android"]["priority"], "high");
}

#[tokio::test]
async fn fcm_send_url_and_success() {
    let http = FakeTransport::new();
    http.stub(
        "oauth2.googleapis.com/token",
        vec![resp(200, &token_resp("tok", 3600))],
    );
    http.stub("messages:send", vec![resp(200, &ok_body())]);
    let json = service_account_json("dropit-prod", "https://oauth2.googleapis.com/token");
    let sender = FcmSender::with_transport("dropit-prod".into(), json, http.clone()).unwrap();
    let report = sender
        .send(&[android_token("reg-1")], msg("t", "b"))
        .await
        .unwrap();
    assert_eq!(report.sent, 1);
    assert_eq!(report.failed, 0);
    assert!(report.invalid_tokens.is_empty());
    let url = &http.recorded_for("messages:send")[0].url;
    assert_eq!(
        url,
        "https://fcm.googleapis.com/v1/projects/dropit-prod/messages:send"
    );
}

// ---------------------------------------------------------------------------
// FCM: разбор ошибок
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fcm_unregistered_marks_invalid() {
    let http = FakeTransport::new();
    http.stub(
        "oauth2.googleapis.com/token",
        vec![resp(200, &token_resp("tok", 3600))],
    );
    http.stub(
        "messages:send",
        vec![resp(
            404,
            br#"{"error":{"code":404,"status":"NOT_FOUND","message":"Requested entity was not found.","details":[{"@type":"type.googleapis.com/google.firebase.fcm.v1.FcmError","errorCode":"UNREGISTERED"}]}}"#,
        )],
    );
    let json = service_account_json("dropit-prod", "https://oauth2.googleapis.com/token");
    let sender = FcmSender::with_transport("dropit-prod".into(), json, http).unwrap();
    let report = sender
        .send(&[android_token("dead")], msg("t", "b"))
        .await
        .unwrap();
    assert_eq!(report.sent, 0);
    assert_eq!(report.failed, 1);
    assert_eq!(report.invalid_tokens, vec!["dead".to_string()]);
    assert!(report.outcomes[0].invalid_token);
}

#[tokio::test]
async fn fcm_invalid_argument_marks_invalid() {
    let http = FakeTransport::new();
    http.stub(
        "oauth2.googleapis.com/token",
        vec![resp(200, &token_resp("tok", 3600))],
    );
    http.stub(
        "messages:send",
        vec![resp(
            400,
            br#"{"error":{"status":"INVALID_ARGUMENT","message":"bad token"}}"#,
        )],
    );
    let json = service_account_json("dropit-prod", "https://oauth2.googleapis.com/token");
    let sender = FcmSender::with_transport("dropit-prod".into(), json, http).unwrap();
    let report = sender
        .send(&[android_token("dead")], msg("t", "b"))
        .await
        .unwrap();
    assert!(report.outcomes[0].invalid_token);
    assert_eq!(report.invalid_tokens, vec!["dead".to_string()]);
}

#[tokio::test]
async fn fcm_details_unregistered_marks_invalid() {
    let http = FakeTransport::new();
    http.stub(
        "oauth2.googleapis.com/token",
        vec![resp(200, &token_resp("tok", 3600))],
    );
    http.stub(
        "messages:send",
        vec![resp(
            400,
            br#"{"error":{"status":"INVALID_ARGUMENT","details":[{"errorCode":"UNREGISTERED"}]}}"#,
        )],
    );
    let json = service_account_json("dropit-prod", "https://oauth2.googleapis.com/token");
    let sender = FcmSender::with_transport("dropit-prod".into(), json, http).unwrap();
    let report = sender
        .send(&[android_token("dead")], msg("t", "b"))
        .await
        .unwrap();
    assert!(report.outcomes[0].invalid_token);
}

#[tokio::test]
async fn fcm_generic_error_is_failed_not_invalid() {
    let http = FakeTransport::new();
    http.stub(
        "oauth2.googleapis.com/token",
        vec![resp(200, &token_resp("tok", 3600))],
    );
    http.stub(
        "messages:send",
        vec![resp(
            400,
            br#"{"error":{"status":"PERMISSION_DENIED","message":"no access"}}"#,
        )],
    );
    let json = service_account_json("dropit-prod", "https://oauth2.googleapis.com/token");
    let sender = FcmSender::with_transport("dropit-prod".into(), json, http).unwrap();
    let report = sender
        .send(&[android_token("x")], msg("t", "b"))
        .await
        .unwrap();
    assert!(!report.outcomes[0].invalid_token);
    assert_eq!(report.failed, 1);
    assert!(report.invalid_tokens.is_empty());
}

#[tokio::test]
async fn fcm_send_unauthorized_propagates() {
    let http = FakeTransport::new();
    http.stub(
        "oauth2.googleapis.com/token",
        vec![resp(200, &token_resp("tok", 3600))],
    );
    http.stub("messages:send", vec![resp(403, b"sender mismatch")]);
    let json = service_account_json("dropit-prod", "https://oauth2.googleapis.com/token");
    let sender = FcmSender::with_transport("dropit-prod".into(), json, http).unwrap();
    let err = sender
        .send(&[android_token("x")], msg("t", "b"))
        .await
        .unwrap_err();
    assert!(matches!(err, PushError::Unauthorized(_)), "{err:?}");
}

#[tokio::test]
async fn fcm_send_503_retries_then_unavailable() {
    let http = FakeTransport::new();
    http.stub(
        "oauth2.googleapis.com/token",
        vec![resp(200, &token_resp("tok", 3600))],
    );
    http.stub(
        "messages:send",
        vec![resp(503, b"down"), resp(503, b"down")],
    );
    let json = service_account_json("dropit-prod", "https://oauth2.googleapis.com/token");
    let mut sender = FcmSender::with_transport("dropit-prod".into(), json, http.clone()).unwrap();
    sender.set_retry_policy(1, Duration::ZERO);
    let err = sender
        .send(&[android_token("x")], msg("t", "b"))
        .await
        .unwrap_err();
    assert!(matches!(err, PushError::Unavailable(_)), "{err:?}");
    assert_eq!(http.recorded_for("messages:send").len(), 2);
}

#[tokio::test]
async fn fcm_batch_partial_failure() {
    let http = FakeTransport::new();
    http.stub(
        "oauth2.googleapis.com/token",
        vec![resp(200, &token_resp("tok", 3600))],
    );
    http.stub(
        "messages:send",
        vec![
            resp(200, &ok_body()),
            resp(
                404,
                br#"{"error":{"status":"NOT_FOUND","details":[{"errorCode":"UNREGISTERED"}]}}"#,
            ),
        ],
    );
    let json = service_account_json("dropit-prod", "https://oauth2.googleapis.com/token");
    let sender = FcmSender::with_transport("dropit-prod".into(), json, http).unwrap();
    let report = sender
        .send(
            &[android_token("good"), android_token("bad")],
            msg("t", "b"),
        )
        .await
        .unwrap();
    assert_eq!(report.sent, 1);
    assert_eq!(report.failed, 1);
    assert_eq!(report.invalid_tokens, vec!["bad".to_string()]);
    assert_eq!(report.outcomes.len(), 2);
}

#[tokio::test]
async fn fcm_duplicate_tokens_deduped() {
    let http = FakeTransport::new();
    http.stub(
        "oauth2.googleapis.com/token",
        vec![resp(200, &token_resp("tok", 3600))],
    );
    http.stub("messages:send", vec![resp(200, &ok_body())]);
    let json = service_account_json("dropit-prod", "https://oauth2.googleapis.com/token");
    let sender = FcmSender::with_transport("dropit-prod".into(), json, http.clone()).unwrap();
    let report = sender
        .send(&[android_token("dup"), android_token("dup")], msg("t", "b"))
        .await
        .unwrap();
    assert_eq!(report.sent, 1);
    assert_eq!(report.outcomes.len(), 1);
    assert_eq!(http.recorded_for("messages:send").len(), 1);
}

// ---------------------------------------------------------------------------
// APNs: конфигурация
// ---------------------------------------------------------------------------

#[test]
fn apns_new_rejects_empty_fields() {
    let p8 = ec_p8();
    assert!(matches!(
        ApnsSender::new(
            String::new(),
            "KEY".into(),
            p8.clone(),
            "topic".into(),
            false
        )
        .unwrap_err(),
        PushError::Config(_)
    ));
    assert!(matches!(
        ApnsSender::new(
            "TEAM".into(),
            String::new(),
            p8.clone(),
            "topic".into(),
            false
        )
        .unwrap_err(),
        PushError::Config(_)
    ));
    assert!(matches!(
        ApnsSender::new(
            "TEAM".into(),
            "KEY".into(),
            p8.clone(),
            String::new(),
            false
        )
        .unwrap_err(),
        PushError::Config(_)
    ));
    assert!(matches!(
        ApnsSender::new(
            "TEAM".into(),
            "KEY".into(),
            String::new(),
            "topic".into(),
            false
        )
        .unwrap_err(),
        PushError::Config(_)
    ));
}

#[test]
fn apns_new_rejects_bad_p8() {
    let err = ApnsSender::new(
        "TEAM".into(),
        "KEY".into(),
        "not a pem".into(),
        "topic".into(),
        false,
    )
    .unwrap_err();
    assert!(matches!(err, PushError::Config(_)), "{err:?}");
}

// ---------------------------------------------------------------------------
// APNs: JWT (ES256)
// ---------------------------------------------------------------------------

#[test]
fn apns_jwt_header_and_claims() {
    let http = FakeTransport::new();
    let sender = ApnsSender::with_transport(
        "TEAM12345".into(),
        "KEY12345AB".into(),
        ec_p8(),
        "app.bundle.id".into(),
        false,
        http,
    )
    .unwrap();
    let jwt = sender.build_jwt().expect("jwt");
    let (header, claims) = jwt_parts(&jwt);
    assert_eq!(header["alg"], "ES256");
    assert_eq!(header["kid"], "KEY12345AB");
    assert_eq!(header["typ"], "JWT");
    assert_eq!(claims["iss"], "TEAM12345");
    assert!(claims["iat"].as_i64().is_some());
}

// ---------------------------------------------------------------------------
// APNs: путь, заголовки, тело
// ---------------------------------------------------------------------------

#[tokio::test]
async fn apns_request_path_headers_and_body() {
    let http = FakeTransport::new();
    http.stub("/3/device/", vec![resp(200, &apns_ok_body())]);
    let sender = ApnsSender::with_transport(
        "TEAM".into(),
        "KEY".into(),
        ec_p8(),
        "app.bundle.id".into(),
        false,
        http.clone(),
    )
    .unwrap();

    let mut data = std::collections::BTreeMap::new();
    data.insert("order_id".to_string(), "42".to_string());
    let message = PushMessage {
        title: "Трек играет".into(),
        body: "Сейчас в эфире".into(),
        data,
        collapse_key: Some("now".into()),
        route: Some("request/42".into()),
    };
    let report = sender
        .send(&[ios_token("aabbccdd11223344")], message)
        .await
        .unwrap();
    assert_eq!(report.sent, 1);

    let req = &http.recorded_for("/3/device/")[0];
    assert_eq!(
        req.url,
        "https://api.push.apple.com/3/device/aabbccdd11223344"
    );
    assert!(req.http2);
    let header = |name: &str| {
        req.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .unwrap_or_else(|| panic!("missing header {name}"))
    };
    assert!(header("Authorization").starts_with("Bearer "));
    assert_eq!(header("apns-topic"), "app.bundle.id");
    assert_eq!(header("apns-push-type"), "alert");
    assert_eq!(header("apns-priority"), "10");
    assert_eq!(header("apns-collapse-id"), "now");
    assert_eq!(header("Content-Type"), "application/json");

    let body: Value = serde_json::from_slice(&req.body).unwrap();
    assert_eq!(body["aps"]["alert"]["title"], "Трек играет");
    assert_eq!(body["aps"]["alert"]["body"], "Сейчас в эфире");
    assert_eq!(body["aps"]["sound"], "default");
    assert_eq!(body["route"], "request/42");
    assert_eq!(body["order_id"], "42");
}

#[tokio::test]
async fn apns_sandbox_host() {
    let http = FakeTransport::new();
    http.stub("/3/device/", vec![resp(200, &apns_ok_body())]);
    let sender = ApnsSender::with_transport(
        "TEAM".into(),
        "KEY".into(),
        ec_p8(),
        "app.bundle.id".into(),
        true,
        http.clone(),
    )
    .unwrap();
    sender
        .send(&[ios_token("aabbccdd11223344")], msg("t", "b"))
        .await
        .unwrap();
    let url = &http.recorded_for("/3/device/")[0].url;
    assert!(
        url.starts_with("https://api.sandbox.push.apple.com/3/device/"),
        "{url}"
    );
}

#[tokio::test]
async fn apns_jwt_cached_across_sends() {
    let http = FakeTransport::new();
    http.stub(
        "/3/device/",
        vec![resp(200, &apns_ok_body()), resp(200, &apns_ok_body())],
    );
    let sender = ApnsSender::with_transport(
        "TEAM".into(),
        "KEY".into(),
        ec_p8(),
        "app.bundle.id".into(),
        false,
        http.clone(),
    )
    .unwrap();
    sender
        .send(&[ios_token("aabbccdd11223344")], msg("t", "b"))
        .await
        .unwrap();
    sender
        .send(&[ios_token("eeff001122334455")], msg("t", "b"))
        .await
        .unwrap();
    let calls = http.recorded_for("/3/device/");
    let a = calls[0]
        .headers
        .iter()
        .find(|(k, _)| k == "Authorization")
        .map(|(_, v)| v.clone())
        .unwrap();
    let b = calls[1]
        .headers
        .iter()
        .find(|(k, _)| k == "Authorization")
        .map(|(_, v)| v.clone())
        .unwrap();
    assert_eq!(a, b, "apns jwt must be cached");
}

// ---------------------------------------------------------------------------
// APNs: статусы
// ---------------------------------------------------------------------------

#[tokio::test]
async fn apns_200_ok() {
    let http = FakeTransport::new();
    http.stub("/3/device/", vec![resp(200, &apns_ok_body())]);
    let sender = ApnsSender::with_transport(
        "TEAM".into(),
        "KEY".into(),
        ec_p8(),
        "app.bundle.id".into(),
        false,
        http,
    )
    .unwrap();
    let report = sender
        .send(&[ios_token("aabbccdd11223344")], msg("t", "b"))
        .await
        .unwrap();
    assert_eq!(report.sent, 1);
}

#[tokio::test]
async fn apns_404_marks_invalid() {
    let http = FakeTransport::new();
    http.stub(
        "/3/device/",
        vec![resp(404, br#"{"reason":"BadDeviceToken"}"#)],
    );
    let sender = ApnsSender::with_transport(
        "TEAM".into(),
        "KEY".into(),
        ec_p8(),
        "app.bundle.id".into(),
        false,
        http,
    )
    .unwrap();
    let report = sender
        .send(&[ios_token("aabbccdd11223344")], msg("t", "b"))
        .await
        .unwrap();
    assert!(report.outcomes[0].invalid_token);
    assert_eq!(report.invalid_tokens, vec!["aabbccdd11223344".to_string()]);
}

#[tokio::test]
async fn apns_410_marks_invalid() {
    let http = FakeTransport::new();
    http.stub(
        "/3/device/",
        vec![resp(410, br#"{"reason":"Unregistered"}"#)],
    );
    let sender = ApnsSender::with_transport(
        "TEAM".into(),
        "KEY".into(),
        ec_p8(),
        "app.bundle.id".into(),
        false,
        http,
    )
    .unwrap();
    let report = sender
        .send(&[ios_token("aabbccdd11223344")], msg("t", "b"))
        .await
        .unwrap();
    assert!(report.outcomes[0].invalid_token);
}

#[tokio::test]
async fn apns_400_bad_device_token_marks_invalid() {
    let http = FakeTransport::new();
    http.stub(
        "/3/device/",
        vec![resp(400, br#"{"reason":"BadDeviceToken"}"#)],
    );
    let sender = ApnsSender::with_transport(
        "TEAM".into(),
        "KEY".into(),
        ec_p8(),
        "app.bundle.id".into(),
        false,
        http,
    )
    .unwrap();
    let report = sender
        .send(&[ios_token("aabbccdd11223344")], msg("t", "b"))
        .await
        .unwrap();
    assert!(report.outcomes[0].invalid_token);
}

#[tokio::test]
async fn apns_400_other_reason_is_failed() {
    let http = FakeTransport::new();
    http.stub(
        "/3/device/",
        vec![resp(400, br#"{"reason":"MissingTopic"}"#)],
    );
    let sender = ApnsSender::with_transport(
        "TEAM".into(),
        "KEY".into(),
        ec_p8(),
        "app.bundle.id".into(),
        false,
        http,
    )
    .unwrap();
    let report = sender
        .send(&[ios_token("aabbccdd11223344")], msg("t", "b"))
        .await
        .unwrap();
    assert!(!report.outcomes[0].invalid_token);
    assert_eq!(report.failed, 1);
}

#[tokio::test]
async fn apns_403_unauthorized() {
    let http = FakeTransport::new();
    http.stub(
        "/3/device/",
        vec![resp(403, br#"{"reason":"InvalidProviderToken"}"#)],
    );
    let sender = ApnsSender::with_transport(
        "TEAM".into(),
        "KEY".into(),
        ec_p8(),
        "app.bundle.id".into(),
        false,
        http,
    )
    .unwrap();
    let err = sender
        .send(&[ios_token("aabbccdd11223344")], msg("t", "b"))
        .await
        .unwrap_err();
    assert!(matches!(err, PushError::Unauthorized(_)), "{err:?}");
}

#[tokio::test]
async fn apns_503_retries_then_unavailable() {
    let http = FakeTransport::new();
    http.stub("/3/device/", vec![resp(503, b"down"), resp(503, b"down")]);
    let mut sender = ApnsSender::with_transport(
        "TEAM".into(),
        "KEY".into(),
        ec_p8(),
        "app.bundle.id".into(),
        false,
        http.clone(),
    )
    .unwrap();
    sender.set_retry_policy(1, Duration::ZERO);
    let err = sender
        .send(&[ios_token("aabbccdd11223344")], msg("t", "b"))
        .await
        .unwrap_err();
    assert!(matches!(err, PushError::Unavailable(_)), "{err:?}");
    assert_eq!(http.recorded_for("/3/device/").len(), 2);
}

#[tokio::test]
async fn apns_timeout_propagates() {
    let http = Arc::new(TimeoutTransport);
    let mut sender = ApnsSender::with_transport(
        "TEAM".into(),
        "KEY".into(),
        ec_p8(),
        "app.bundle.id".into(),
        false,
        http.clone(),
    )
    .unwrap();
    sender.set_retry_policy(0, Duration::ZERO);
    let err = sender
        .send(&[ios_token("aabbccdd11223344")], msg("t", "b"))
        .await
        .unwrap_err();
    assert!(matches!(err, PushError::Timeout), "{err:?}");
}

/// Транспорт, всегда отвечающий таймаутом.
struct TimeoutTransport;

#[async_trait]
impl HttpTransport for TimeoutTransport {
    async fn execute(&self, _req: HttpRequest) -> Result<HttpResponse, PushError> {
        Err(PushError::Timeout)
    }
}

#[tokio::test]
async fn apns_malformed_token_marked_invalid() {
    let http = FakeTransport::new();
    http.stub("/3/device/", vec![resp(200, &apns_ok_body())]);
    let sender = ApnsSender::with_transport(
        "TEAM".into(),
        "KEY".into(),
        ec_p8(),
        "app.bundle.id".into(),
        false,
        http.clone(),
    )
    .unwrap();
    let report = sender
        .send(&[ios_token("not-hex!!")], msg("t", "b"))
        .await
        .unwrap();
    assert!(report.outcomes[0].invalid_token);
    assert!(http.recorded_for("/3/device/").is_empty());
}

// ---------------------------------------------------------------------------
// RoutingPush
// ---------------------------------------------------------------------------

#[tokio::test]
async fn routing_dispatches_by_platform() {
    let fcm_http = FakeTransport::new();
    fcm_http.stub(
        "oauth2.googleapis.com/token",
        vec![resp(200, &token_resp("ftok", 3600))],
    );
    fcm_http.stub("messages:send", vec![resp(200, &ok_body())]);
    let fcm = FcmSender::with_transport(
        "dropit-prod".into(),
        service_account_json("dropit-prod", "https://oauth2.googleapis.com/token"),
        fcm_http.clone(),
    )
    .unwrap();

    let apns_http = FakeTransport::new();
    apns_http.stub("/3/device/", vec![resp(200, &apns_ok_body())]);
    let apns = ApnsSender::with_transport(
        "TEAM".into(),
        "KEY".into(),
        ec_p8(),
        "app.bundle.id".into(),
        false,
        apns_http.clone(),
    )
    .unwrap();

    let routing = RoutingPush::new(fcm, apns);
    let report = routing
        .send(
            &[android_token("reg-1"), ios_token("aabbccdd11223344")],
            msg("t", "b"),
        )
        .await
        .unwrap();
    assert_eq!(report.sent, 2);
    assert_eq!(fcm_http.recorded_for("messages:send").len(), 1);
    assert_eq!(apns_http.recorded_for("/3/device/").len(), 1);
}

#[tokio::test]
async fn routing_missing_platform_sender_fails_token() {
    // Только FCM настроен; iOS-токен должен получить failed-outcome без сети.
    let fcm_http = FakeTransport::new();
    fcm_http.stub(
        "oauth2.googleapis.com/token",
        vec![resp(200, &token_resp("ftok", 3600))],
    );
    fcm_http.stub("messages:send", vec![resp(200, &ok_body())]);
    let fcm = FcmSender::with_transport(
        "dropit-prod".into(),
        service_account_json("dropit-prod", "https://oauth2.googleapis.com/token"),
        fcm_http,
    )
    .unwrap();
    let routing = RoutingPush::with_senders(Some(fcm), None);
    let report = routing
        .send(
            &[android_token("reg-1"), ios_token("aabbccdd11223344")],
            msg("t", "b"),
        )
        .await
        .unwrap();
    assert_eq!(report.sent, 1);
    assert_eq!(report.failed, 1);
    let ios_outcome = report
        .outcomes
        .iter()
        .find(|o| o.token == "aabbccdd11223344")
        .unwrap();
    assert!(!ios_outcome.ok);
    assert!(!ios_outcome.invalid_token);
    assert!(ios_outcome.error.as_deref().unwrap().contains("apns"));
}

#[test]
fn empty_config_routing_is_none() {
    let config = PushConfig::default();
    assert!(config.routing().unwrap().is_none());
}

#[test]
fn full_config_builds_routing() {
    let config = PushConfig {
        fcm: Some(FcmConfig {
            project_id: "dropit-prod".into(),
            service_account_json: service_account_json(
                "dropit-prod",
                "https://oauth2.googleapis.com/token",
            ),
        }),
        apns: Some(ApnsConfig {
            team_id: "TEAM".into(),
            key_id: "KEY".into(),
            private_key_p8: ec_p8(),
            topic: "app.bundle.id".into(),
            sandbox: false,
        }),
    };
    assert!(config.routing().unwrap().is_some());
}

// ---------------------------------------------------------------------------
// from_env (чистая логика)
// ---------------------------------------------------------------------------

fn env_lookup<'a>(map: &'a HashMap<&'static str, String>) -> impl Fn(&str) -> Option<String> + 'a {
    move |key: &str| map.get(key).cloned()
}

#[test]
fn from_env_empty_is_none() {
    let map: HashMap<&'static str, String> = HashMap::new();
    assert!(crate::routing::from_env_via(env_lookup(&map)).is_none());
}

#[test]
fn from_env_full_fcm_and_apns() {
    let mut map: HashMap<&'static str, String> = HashMap::new();
    map.insert("FCM_PROJECT_ID", "dropit-prod".into());
    map.insert("FCM_SERVICE_ACCOUNT_JSON", "{}".into());
    map.insert("APNS_TEAM_ID", "TEAM".into());
    map.insert("APNS_KEY_ID", "KEY".into());
    map.insert("APNS_PRIVATE_KEY_P8", "p8".into());
    map.insert("APNS_TOPIC", "app.bundle.id".into());
    map.insert("APNS_SANDBOX", "1".into());
    let cfg = crate::routing::from_env_via(env_lookup(&map)).expect("config");
    assert!(cfg.fcm.is_some());
    let apns = cfg.apns.expect("apns");
    assert!(apns.sandbox);
}

#[test]
fn from_env_partial_groups_disabled() {
    let mut map: HashMap<&'static str, String> = HashMap::new();
    map.insert("FCM_PROJECT_ID", "dropit-prod".into());
    // FCM_SERVICE_ACCOUNT_JSON отсутствует → FCM выключен, APNS тоже нет → None
    assert!(crate::routing::from_env_via(env_lookup(&map)).is_none());
}
