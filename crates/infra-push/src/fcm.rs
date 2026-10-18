//! Отправитель через Firebase Cloud Messaging (FCM HTTP v1).
//!
//! Поток:
//! 1. Из сервисного аккаунта Google собирается JWT-bearer (RS256):
//!    `iss = client_email`, `scope = firebase.messaging`,
//!    `aud = https://oauth2.googleapis.com/token`, `exp = iat + 1ч`.
//! 2. JWT обменивается на OAuth2 access-token (POST на `token_uri`).
//!    Токен кешируется с учётом `expires_in` (обновляем за 60с до истечения).
//! 3. Каждое сообщение отправляется отдельным POST
//!    `{base}/v1/projects/{project}/messages:send` с заголовком
//!    `Authorization: Bearer <access-token>`.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::sync::Mutex;

use crate::model::{PushMessage, PushOutcome, PushReport, PushToken};
use crate::transport::{sanitize_body, HttpRequest, ReqwestTransport, SharedTransport};
use crate::PushError;

/// OAuth2 scope для Firebase Messaging.
const FCM_SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";
/// Базовый URL FCM HTTP v1.
const DEFAULT_BASE_URL: &str = "https://fcm.googleapis.com";
/// URL обмена JWT на access-token по умолчанию.
const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
/// Срок жизни OAuth2 JWT (секунд).
const JWT_LIFETIME_SECS: i64 = 3600;

/// Отправитель push через FCM HTTP v1.
pub struct FcmSender {
    project_id: String,
    client_email: String,
    encoding_key: jsonwebtoken::EncodingKey,
    token_uri: String,
    base_url: String,
    http: SharedTransport,
    cache: Mutex<Option<CachedAccessToken>>,
    max_retries: u32,
    retry_delay: Duration,
}

impl std::fmt::Debug for FcmSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FcmSender")
            .field("project_id", &self.project_id)
            .field("token_uri", &self.token_uri)
            .finish_non_exhaustive()
    }
}

struct CachedAccessToken {
    value: String,
    expires_at: Instant,
}

/// Распарсенный сервисный аккаунт Google (только нужные поля).
#[derive(Deserialize)]
struct ServiceAccount {
    #[serde(default)]
    client_email: Option<String>,
    #[serde(default)]
    private_key: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    token_uri: Option<String>,
}

/// Результат классификации HTTP-ответа на отправку одного токена.
enum SendResult {
    /// Успешно доставлено.
    Ok,
    /// Токен невалиден — требует удаления.
    Invalid(String),
    /// Ошибка отправки, токен остаётся валидным.
    Failed(String),
}

impl FcmSender {
    /// Создать отправитель из `project_id` и JSON сервисного аккаунта Google.
    ///
    /// Если `project_id` пуст — берётся `project_id` из JSON аккаунта.
    /// Приватный RSA-ключ проверяется немедленно: невалидный PEM →
    /// [`PushError::Config`].
    pub fn new(project_id: String, service_account_json: String) -> Result<Self, PushError> {
        let parsed = parse_service_account(&service_account_json)?;
        let http: SharedTransport = Arc::new(ReqwestTransport::new(Duration::from_secs(10))?);
        Self::build(project_id, parsed, http)
    }

    /// Конструктор для тестов с внедряемым транспортом (виден только в крейте).
    #[cfg(test)]
    pub(crate) fn with_transport(
        project_id: String,
        service_account_json: String,
        http: SharedTransport,
    ) -> Result<Self, PushError> {
        let parsed = parse_service_account(&service_account_json)?;
        Self::build(project_id, parsed, http)
    }

    fn build(
        project_id: String,
        parsed: ServiceAccount,
        http: SharedTransport,
    ) -> Result<Self, PushError> {
        let client_email = parsed
            .client_email
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| PushError::Config("FCM service account has no client_email".into()))?;
        let private_key = parsed
            .private_key
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| PushError::Config("FCM service account has no private_key".into()))?;
        let encoding_key = jsonwebtoken::EncodingKey::from_rsa_pem(private_key.as_bytes())
            .map_err(|_| PushError::Config("FCM private_key is not a valid RSA PEM key".into()))?;

        let pid = if project_id.trim().is_empty() {
            parsed
                .project_id
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| PushError::Config("FCM project_id is empty".into()))?
        } else {
            project_id
        };
        validate_project_id(&pid)?;

        let token_uri = parsed
            .token_uri
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| DEFAULT_TOKEN_URI.to_string());

        Ok(Self {
            project_id: pid,
            client_email,
            encoding_key,
            token_uri,
            base_url: DEFAULT_BASE_URL.to_string(),
            http,
            cache: Mutex::new(None),
            max_retries: 2,
            retry_delay: Duration::from_millis(200),
        })
    }

    /// Переопределить политику повтора (только для тестов внутри крейта).
    #[cfg(test)]
    pub(crate) fn set_retry_policy(&mut self, max_retries: u32, delay: Duration) {
        self.max_retries = max_retries;
        self.retry_delay = delay;
    }

    /// Построить OAuth2 JWT-bearer для обмена на access-token.
    pub(crate) fn build_assertion(&self) -> Result<String, PushError> {
        let now = chrono::Utc::now().timestamp();
        let claims = FcmOAuthClaims {
            iss: self.client_email.clone(),
            scope: FCM_SCOPE,
            aud: self.token_uri.clone(),
            iat: now,
            exp: now + JWT_LIFETIME_SECS,
        };
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        header.typ = Some("JWT".to_string());
        jsonwebtoken::encode(&header, &claims, &self.encoding_key)
            .map_err(|e| PushError::Config(format!("fcm jwt encode failed: {e}")))
    }

    /// Получить действующий access-token (с кешированием).
    async fn access_token(&self) -> Result<String, PushError> {
        let mut guard = self.cache.lock().await;
        let now = Instant::now();
        if let Some(cached) = guard.as_ref() {
            if cached.expires_at > now {
                return Ok(cached.value.clone());
            }
        }

        let assertion = self.build_assertion()?;
        let body =
            format!("grant_type=urn:ietf:params:oauth:grant-type:jwt-bearer&assertion={assertion}");
        let req = HttpRequest {
            url: self.token_uri.clone(),
            headers: vec![(
                "Content-Type".to_string(),
                "application/x-www-form-urlencoded".to_string(),
            )],
            body: body.into_bytes(),
            http2: false,
        };
        let resp = self.http.execute(req).await?;
        match resp.status {
            200 => {}
            401 | 403 => {
                return Err(PushError::Unauthorized(sanitize_body(
                    "fcm token endpoint",
                    &resp.body,
                )))
            }
            s if (500..600).contains(&s) => {
                return Err(PushError::Unavailable(sanitize_body(
                    "fcm token endpoint",
                    &resp.body,
                )))
            }
            s => {
                return Err(PushError::Decode(format!(
                    "fcm token endpoint returned status {s}"
                )))
            }
        }

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: Option<String>,
            expires_in: Option<u64>,
        }
        let parsed: TokenResponse = serde_json::from_slice(&resp.body)
            .map_err(|e| PushError::Decode(format!("fcm token response parse: {e}")))?;
        let access_token = parsed
            .access_token
            .filter(|s| !s.is_empty())
            .ok_or_else(|| PushError::Decode("fcm token response missing access_token".into()))?;
        let expires_in = parsed.expires_in.unwrap_or(JWT_LIFETIME_SECS as u64);
        // Обновляем за минуту до заявленного истечения; при малом TTL (<= 60с)
        // считаем токен уже истекшим — следующий вызов перевыпустит его.
        let refresh_in = expires_in.saturating_sub(60);

        *guard = Some(CachedAccessToken {
            value: access_token.clone(),
            expires_at: now + Duration::from_secs(refresh_in),
        });
        Ok(access_token)
    }

    /// URL отправки сообщения.
    fn send_url(&self) -> String {
        format!(
            "{}/v1/projects/{}/messages:send",
            self.base_url.trim_end_matches('/'),
            self.project_id
        )
    }

    /// Отправить сообщение одному токену.
    async fn send_one(&self, token: &str, message: &PushMessage) -> Result<PushOutcome, PushError> {
        if token.trim().is_empty() {
            return Ok(PushOutcome::invalid(token, "empty push token"));
        }

        let access_token = self.access_token().await?;
        let body = build_fcm_body(token, message);
        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| PushError::Decode(format!("encode fcm message body: {e}")))?;

        let req = HttpRequest {
            url: self.send_url(),
            headers: vec![
                (
                    "Authorization".to_string(),
                    format!("Bearer {access_token}"),
                ),
                ("Content-Type".to_string(), "application/json".to_string()),
            ],
            body: body_bytes,
            http2: false,
        };

        let result = self.send_with_retry(req).await?;
        Ok(match result {
            SendResult::Ok => PushOutcome::success(token),
            SendResult::Invalid(reason) => PushOutcome::invalid(token, reason),
            SendResult::Failed(reason) => PushOutcome::failed(token, reason),
        })
    }

    /// Выполнить запрос с повтором на транзиентных ошибках.
    async fn send_with_retry(&self, req: HttpRequest) -> Result<SendResult, PushError> {
        for attempt in 0..=self.max_retries {
            if attempt > 0 {
                tokio::time::sleep(self.retry_delay).await;
            }
            let resp = match self.http.execute(req.clone()).await {
                Ok(r) => r,
                Err(PushError::Timeout) | Err(PushError::Unavailable(_))
                    if attempt < self.max_retries =>
                {
                    continue;
                }
                Err(e) => return Err(e),
            };
            match resp.status {
                200 => return Ok(SendResult::Ok),
                401 | 403 => {
                    return Err(PushError::Unauthorized(sanitize_body(
                        "fcm send endpoint",
                        &resp.body,
                    )))
                }
                400 | 404 => return Ok(parse_fcm_error(&resp.body)),
                429 | 500..=599 if attempt < self.max_retries => continue,
                429 | 500..=599 => {
                    return Err(PushError::Unavailable(sanitize_body(
                        "fcm send endpoint",
                        &resp.body,
                    )))
                }
                s => return Ok(SendResult::Failed(format!("fcm status {s}"))),
            }
        }
        Err(PushError::Unavailable(
            "fcm send retries exhausted".to_string(),
        ))
    }
}

/// Клеймы OAuth2 JWT-bearer для FCM.
#[derive(Serialize)]
struct FcmOAuthClaims {
    iss: String,
    scope: &'static str,
    aud: String,
    iat: i64,
    exp: i64,
}

#[async_trait::async_trait]
impl crate::PushSender for FcmSender {
    async fn send(
        &self,
        tokens: &[PushToken],
        message: PushMessage,
    ) -> Result<PushReport, PushError> {
        let mut report = PushReport::default();
        let mut seen: HashSet<&str> = HashSet::new();

        for tok in tokens {
            if !seen.insert(tok.token.as_str()) {
                tracing::debug!(target: "dropit.push.fcm", "duplicate push token skipped in batch");
                continue;
            }
            let outcome = self.send_one(&tok.token, &message).await?;
            report.add(outcome);
        }
        Ok(report)
    }
}

/// Построить тело сообщения FCM (`{ "message": { ... } }`).
fn build_fcm_body(token: &str, message: &PushMessage) -> Value {
    let mut data = Map::new();
    for (key, value) in &message.data {
        if key == "aps" || key == "route" {
            continue;
        }
        data.insert(key.clone(), Value::String(value.clone()));
    }
    if let Some(route) = &message.route {
        data.insert("route".to_string(), Value::String(route.clone()));
    }

    let mut notification = Map::new();
    notification.insert("title".to_string(), Value::String(message.title.clone()));
    notification.insert("body".to_string(), Value::String(message.body.clone()));

    let mut android = Map::new();
    android.insert("priority".to_string(), Value::String("high".to_string()));
    if let Some(collapse_key) = &message.collapse_key {
        android.insert(
            "collapse_key".to_string(),
            Value::String(collapse_key.clone()),
        );
    }

    let mut inner = Map::new();
    inner.insert("token".to_string(), Value::String(token.to_string()));
    inner.insert("notification".to_string(), Value::Object(notification));
    if !data.is_empty() {
        inner.insert("data".to_string(), Value::Object(data));
    }
    inner.insert("android".to_string(), Value::Object(android));

    let mut root = Map::new();
    root.insert("message".to_string(), Value::Object(inner));
    Value::Object(root)
}

/// Разобрать тело ошибки FCM и решить, невалиден ли токен.
fn parse_fcm_error(body: &[u8]) -> SendResult {
    #[derive(Deserialize)]
    struct FcmErrorEnvelope {
        #[serde(default)]
        error: Option<FcmError>,
    }
    #[derive(Deserialize)]
    struct FcmError {
        #[serde(default)]
        status: Option<String>,
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        details: Option<Vec<FcmErrorDetail>>,
    }
    #[derive(Deserialize)]
    struct FcmErrorDetail {
        #[serde(default, rename = "errorCode")]
        error_code: Option<String>,
    }

    let envelope = serde_json::from_slice::<FcmErrorEnvelope>(body).ok();
    if let Some(err) = envelope.and_then(|e| e.error) {
        let status = err.status.as_deref().unwrap_or("");
        let mut invalid = matches!(status, "UNREGISTERED" | "INVALID_ARGUMENT");
        if let Some(details) = &err.details {
            if details.iter().any(|d| {
                d.error_code.as_deref() == Some("UNREGISTERED")
                    || d.error_code.as_deref() == Some("INVALID_ARGUMENT")
            }) {
                invalid = true;
            }
        }
        let text = err
            .message
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| status.to_string());
        return if invalid {
            SendResult::Invalid(text)
        } else {
            SendResult::Failed(text)
        };
    }
    SendResult::Failed(String::from_utf8_lossy(body).trim().to_string())
}

/// Проверить `project_id` на отсутствие символов, опасных для URL-пути.
fn validate_project_id(project_id: &str) -> Result<(), PushError> {
    if project_id.is_empty()
        || project_id.contains('/')
        || project_id.contains('?')
        || project_id.contains('#')
        || project_id.chars().any(|c| c.is_whitespace())
    {
        return Err(PushError::Config(format!(
            "invalid FCM project_id: {:?}",
            project_id
        )));
    }
    Ok(())
}

/// Разобрать JSON сервисного аккаунта.
fn parse_service_account(json: &str) -> Result<ServiceAccount, PushError> {
    serde_json::from_str::<ServiceAccount>(json)
        .map_err(|e| PushError::Config(format!("invalid FCM service account json: {e}")))
}
