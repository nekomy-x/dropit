//! Отправитель через Apple Push Notification service (APNs HTTP/2).
//!
//! Поток:
//! 1. Из `team_id`/`key_id`/`.p8` собирается JWT ES256
//!    (`header.kid = key_id`, `claims.iss = team_id`, `claims.iat = now`).
//!    JWT кешируется ~45 минут (Apple лимитирует перевыпуск).
//! 2. Каждое сообщение отправляется POST
//!    `{host}/3/device/{token}` с заголовками `apns-topic`,
//!    `apns-push-type: alert`, `apns-priority: 10`, `apns-collapse-id`.
//! 3. Ответы 404/410/`BadDeviceToken`/`Unregistered` помечают токен невалидным.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::sync::Mutex;

use crate::model::{PushMessage, PushOutcome, PushReport, PushToken};
use crate::transport::{sanitize_body, HttpRequest, ReqwestTransport, SharedTransport};
use crate::PushError;

/// Продакшн-хост APNs.
const HOST_PRODUCTION: &str = "https://api.push.apple.com";
/// Песочница APNs.
const HOST_SANDBOX: &str = "https://api.sandbox.push.apple.com";
/// Срок жизни APNs JWT перед перевыпуском (Apple допускает до 1 часа).
const JWT_REFRESH_AFTER: Duration = Duration::from_secs(45 * 60);

/// Отправитель push через APNs HTTP/2.
pub struct ApnsSender {
    team_id: String,
    key_id: String,
    topic: String,
    encoding_key: jsonwebtoken::EncodingKey,
    host: String,
    http: SharedTransport,
    jwt_cache: Mutex<Option<CachedJwt>>,
    max_retries: u32,
    retry_delay: Duration,
}

impl std::fmt::Debug for ApnsSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApnsSender")
            .field("host", &self.host)
            .field("topic", &self.topic)
            .finish_non_exhaustive()
    }
}

struct CachedJwt {
    token: String,
    issued_at: Instant,
}

/// Результат классификации HTTP-ответа APNs по одному токену.
enum SendResult {
    /// Успешно доставлено.
    Ok,
    /// Токен невалиден — требует удаления.
    Invalid(String),
    /// Ошибка отправки, токен остаётся валидным.
    Failed(String),
}

impl ApnsSender {
    /// Создать отправитель APNs.
    ///
    /// `p8_key` — содержимое Apple-ключа (`.p8`) в PKCS#8 PEM. Ключ проверяется
    /// немедленно: невалидный EC P-256 ключ → [`PushError::Config`].
    /// `sandbox = true` направляет запросы на `api.sandbox.push.apple.com`.
    pub fn new(
        team_id: String,
        key_id: String,
        p8_key: String,
        topic: String,
        sandbox: bool,
    ) -> Result<Self, PushError> {
        require_non_empty("team_id", &team_id)?;
        require_non_empty("key_id", &key_id)?;
        require_non_empty("topic", &topic)?;
        require_non_empty("p8_key", &p8_key)?;

        let encoding_key =
            jsonwebtoken::EncodingKey::from_ec_pem(p8_key.as_bytes()).map_err(|_| {
                PushError::Config("APNs p8 key is not a valid EC (P-256) PEM key".into())
            })?;
        let host = if sandbox {
            HOST_SANDBOX
        } else {
            HOST_PRODUCTION
        };
        let http: SharedTransport = Arc::new(ReqwestTransport::new(Duration::from_secs(10))?);

        Ok(Self {
            team_id,
            key_id,
            topic,
            encoding_key,
            host: host.to_string(),
            http,
            jwt_cache: Mutex::new(None),
            max_retries: 2,
            retry_delay: Duration::from_millis(200),
        })
    }

    /// Конструктор для тестов с внедряемым транспортом (виден только в крейте).
    #[cfg(test)]
    pub(crate) fn with_transport(
        team_id: String,
        key_id: String,
        p8_key: String,
        topic: String,
        sandbox: bool,
        http: SharedTransport,
    ) -> Result<Self, PushError> {
        let mut sender = Self::new(team_id, key_id, p8_key, topic, sandbox)?;
        sender.http = http;
        Ok(sender)
    }

    /// Переопределить политику повтора (только для тестов внутри крейта).
    #[cfg(test)]
    pub(crate) fn set_retry_policy(&mut self, max_retries: u32, delay: Duration) {
        self.max_retries = max_retries;
        self.retry_delay = delay;
    }

    /// Построить APNs JWT (ES256).
    pub(crate) fn build_jwt(&self) -> Result<String, PushError> {
        let now = chrono::Utc::now().timestamp();
        let claims = ApnsClaims {
            iss: self.team_id.clone(),
            iat: now,
        };
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
        header.kid = Some(self.key_id.clone());
        header.typ = Some("JWT".to_string());
        jsonwebtoken::encode(&header, &claims, &self.encoding_key)
            .map_err(|e| PushError::Config(format!("apns jwt encode failed: {e}")))
    }

    /// Получить заголовок `Authorization` (с кешированием JWT).
    async fn authorization(&self) -> Result<String, PushError> {
        let mut guard = self.jwt_cache.lock().await;
        let now = Instant::now();
        if let Some(cached) = guard.as_ref() {
            if now.duration_since(cached.issued_at) < JWT_REFRESH_AFTER {
                return Ok(format!("Bearer {}", cached.token));
            }
        }
        let token = self.build_jwt()?;
        *guard = Some(CachedJwt {
            token: token.clone(),
            issued_at: now,
        });
        Ok(format!("Bearer {token}"))
    }

    /// URL отправки для конкретного device token.
    fn device_url(&self, token: &str) -> String {
        format!("{}/3/device/{}", self.host.trim_end_matches('/'), token)
    }

    /// Отправить сообщение одному токену.
    async fn send_one(&self, token: &str, message: &PushMessage) -> Result<PushOutcome, PushError> {
        if !is_valid_device_token(token) {
            return Ok(PushOutcome::invalid(token, "malformed device token"));
        }

        let authorization = self.authorization().await?;
        let body = build_apns_body(message);
        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| PushError::Decode(format!("encode apns body: {e}")))?;

        let mut headers = vec![
            ("Authorization".to_string(), authorization),
            ("apns-topic".to_string(), self.topic.clone()),
            ("apns-push-type".to_string(), "alert".to_string()),
            ("apns-priority".to_string(), "10".to_string()),
            ("Content-Type".to_string(), "application/json".to_string()),
        ];
        if let Some(collapse_key) = &message.collapse_key {
            headers.push(("apns-collapse-id".to_string(), collapse_key.clone()));
        }

        let req = HttpRequest {
            url: self.device_url(token),
            headers,
            body: body_bytes,
            http2: true,
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
                        "apns endpoint",
                        &resp.body,
                    )))
                }
                410 => return Ok(SendResult::Invalid("Unregistered".to_string())),
                404 => {
                    let reason = parse_apns_reason(&resp.body);
                    return Ok(SendResult::Invalid(reason));
                }
                400 => return Ok(parse_apns_400(&resp.body)),
                429 | 500..=599 if attempt < self.max_retries => continue,
                429 | 500..=599 => {
                    return Err(PushError::Unavailable(sanitize_body(
                        "apns endpoint",
                        &resp.body,
                    )))
                }
                s => return Ok(SendResult::Failed(format!("apns status {s}"))),
            }
        }
        Err(PushError::Unavailable(
            "apns send retries exhausted".to_string(),
        ))
    }
}

/// Клеймы APNs JWT.
#[derive(Serialize)]
struct ApnsClaims {
    iss: String,
    iat: i64,
}

#[async_trait::async_trait]
impl crate::PushSender for ApnsSender {
    async fn send(
        &self,
        tokens: &[PushToken],
        message: PushMessage,
    ) -> Result<PushReport, PushError> {
        let mut report = PushReport::default();
        for tok in tokens {
            let outcome = self.send_one(&tok.token, &message).await?;
            report.add(outcome);
        }
        Ok(report)
    }
}

/// Построить тело APNs-уведомления.
///
/// Структура: `{ "aps": { "alert": { "title", "body" }, "sound": "default" },
/// "route": ..., <data-поля> }`. Ключи `aps` и `route` в `data` зарезервированы.
fn build_apns_body(message: &PushMessage) -> Value {
    let mut root = Map::new();
    for (key, value) in &message.data {
        if key == "aps" || key == "route" {
            continue;
        }
        root.insert(key.clone(), Value::String(value.clone()));
    }
    if let Some(route) = &message.route {
        root.insert("route".to_string(), Value::String(route.clone()));
    }

    let mut alert = Map::new();
    alert.insert("title".to_string(), Value::String(message.title.clone()));
    alert.insert("body".to_string(), Value::String(message.body.clone()));

    let mut aps = Map::new();
    aps.insert("alert".to_string(), Value::Object(alert));
    aps.insert("sound".to_string(), Value::String("default".to_string()));

    root.insert("aps".to_string(), Value::Object(aps));
    Value::Object(root)
}

/// Извлечь `reason` из тела ошибки APNs (или вернуть текст тела).
fn parse_apns_reason(body: &[u8]) -> String {
    #[derive(Deserialize)]
    struct ApnsError {
        #[serde(default)]
        reason: Option<String>,
    }
    serde_json::from_slice::<ApnsError>(body)
        .ok()
        .and_then(|e| e.reason)
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| String::from_utf8_lossy(body).trim().to_string())
}

/// Классифицировать ответ 400: `BadDeviceToken` → невалиден, иначе обычная ошибка.
fn parse_apns_400(body: &[u8]) -> SendResult {
    let reason = parse_apns_reason(body);
    if matches!(
        reason.as_str(),
        "BadDeviceToken" | "Unregistered" | "DeviceTokenNotForTopic"
    ) {
        SendResult::Invalid(reason)
    } else {
        SendResult::Failed(reason)
    }
}

/// Проверить, что device token пригоден для URL-пути (hex, без разделителей).
fn is_valid_device_token(token: &str) -> bool {
    !token.is_empty() && token.bytes().all(|b| b.is_ascii_hexdigit()) && token.len() >= 8
}

/// Проверить непустоту конфигурационного поля.
fn require_non_empty(name: &str, value: &str) -> Result<(), PushError> {
    if value.trim().is_empty() {
        return Err(PushError::Config(format!("APNs {name} must not be empty")));
    }
    Ok(())
}
