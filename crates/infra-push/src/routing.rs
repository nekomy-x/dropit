//! Маршрутизация по платформе и конфигурация из переменных окружения.

use crate::apns::ApnsSender;
use crate::fcm::FcmSender;
use crate::model::{Platform, PushMessage, PushOutcome, PushReport, PushToken};
use crate::PushError;

/// Конфигурация FCM-транспорта.
#[derive(Debug, Clone)]
pub struct FcmConfig {
    /// Идентификатор Firebase-проекта.
    pub project_id: String,
    /// JSON сервисного аккаунта Google (содержит `client_email` и `private_key`).
    pub service_account_json: String,
}

/// Конфигурация APNs-транспорта.
#[derive(Debug, Clone)]
pub struct ApnsConfig {
    /// Apple Team ID (10 символов).
    pub team_id: String,
    /// Key ID из Apple Developer (10 символов).
    pub key_id: String,
    /// Содержимое `.p8`-ключа в PKCS#8 PEM.
    pub private_key_p8: String,
    /// Bundle ID приложения — значение заголовка `apns-topic`.
    pub topic: String,
    /// Использовать sandbox-хост APNs.
    pub sandbox: bool,
}

/// Полная конфигурация push-транспорта.
#[derive(Debug, Clone, Default)]
pub struct PushConfig {
    /// Конфигурация FCM (Android). `None` — FCM не настроен.
    pub fcm: Option<FcmConfig>,
    /// Конфигурация APNs (iOS). `None` — APNs не настроен.
    pub apns: Option<ApnsConfig>,
}

impl PushConfig {
    /// Собрать FCM-отправитель, если FCM настроен.
    pub fn fcm_sender(&self) -> Result<Option<FcmSender>, PushError> {
        match &self.fcm {
            Some(cfg) => Ok(Some(FcmSender::new(
                cfg.project_id.clone(),
                cfg.service_account_json.clone(),
            )?)),
            None => Ok(None),
        }
    }

    /// Собрать APNs-отправитель, если APNs настроен.
    pub fn apns_sender(&self) -> Result<Option<ApnsSender>, PushError> {
        match &self.apns {
            Some(cfg) => Ok(Some(ApnsSender::new(
                cfg.team_id.clone(),
                cfg.key_id.clone(),
                cfg.private_key_p8.clone(),
                cfg.topic.clone(),
                cfg.sandbox,
            )?)),
            None => Ok(None),
        }
    }

    /// Собрать маршрутизирующий отправитель из доступных платформ.
    ///
    /// Возвращает `None`, если не настроена ни одна платформа. Если настроена
    /// только часть платформ, [`RoutingPush`] всё равно создаётся: токены
    /// ненастроенной платформы получат результат с ошибкой конфигурации.
    pub fn routing(&self) -> Result<Option<RoutingPush>, PushError> {
        let fcm = self.fcm_sender()?;
        let apns = self.apns_sender()?;
        if fcm.is_none() && apns.is_none() {
            return Ok(None);
        }
        Ok(Some(RoutingPush { fcm, apns }))
    }
}

/// Маршрутизирующий отправитель: Android → FCM, iOS → APNs.
///
/// Если транспорт для платформы не настроен, токены этой платформы получают
/// результат `ok = false` с описанием ошибки конфигурации (без прерывания
/// всего вызова).
pub struct RoutingPush {
    fcm: Option<FcmSender>,
    apns: Option<ApnsSender>,
}

impl std::fmt::Debug for RoutingPush {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoutingPush")
            .field("fcm", &self.fcm.is_some())
            .field("apns", &self.apns.is_some())
            .finish()
    }
}

impl RoutingPush {
    /// Создать маршрутизатор с обоими транспортами.
    pub fn new(fcm: FcmSender, apns: ApnsSender) -> Self {
        Self {
            fcm: Some(fcm),
            apns: Some(apns),
        }
    }

    /// Конструктор с опциональными транспортами (только для тестов в крейте).
    #[cfg(test)]
    pub(crate) fn with_senders(fcm: Option<FcmSender>, apns: Option<ApnsSender>) -> Self {
        Self { fcm, apns }
    }

    /// Создать маршрутизатор из конфигурации.
    pub fn from_config(config: &PushConfig) -> Result<Option<Self>, PushError> {
        config.routing()
    }
}

#[async_trait::async_trait]
impl crate::PushSender for RoutingPush {
    async fn send(
        &self,
        tokens: &[PushToken],
        message: PushMessage,
    ) -> Result<PushReport, PushError> {
        let mut report = PushReport::default();

        let android_tokens: Vec<PushToken> = tokens
            .iter()
            .filter(|t| t.platform == Platform::Android)
            .cloned()
            .collect();
        let ios_tokens: Vec<PushToken> = tokens
            .iter()
            .filter(|t| t.platform == Platform::Ios)
            .cloned()
            .collect();

        if !android_tokens.is_empty() {
            match &self.fcm {
                Some(sender) => {
                    let part = sender.send(&android_tokens, message.clone()).await?;
                    report.merge(part);
                }
                None => {
                    for tok in &android_tokens {
                        report.add(PushOutcome::failed(
                            &tok.token,
                            "fcm transport is not configured",
                        ));
                    }
                }
            }
        }

        if !ios_tokens.is_empty() {
            match &self.apns {
                Some(sender) => {
                    let part = sender.send(&ios_tokens, message.clone()).await?;
                    report.merge(part);
                }
                None => {
                    for tok in &ios_tokens {
                        report.add(PushOutcome::failed(
                            &tok.token,
                            "apns transport is not configured",
                        ));
                    }
                }
            }
        }

        Ok(report)
    }
}

/// Собрать конфигурацию push из переменных окружения.
///
/// Переменные:
/// * FCM: `FCM_PROJECT_ID`, `FCM_SERVICE_ACCOUNT_JSON`.
/// * APNs: `APNS_TEAM_ID`, `APNS_KEY_ID`, `APNS_PRIVATE_KEY_P8`,
///   `APNS_TOPIC`, опционально `APNS_SANDBOX` (`1`/`true`/`yes`/`on`).
///
/// Возвращает `None`, если не настроено ни одного транспорта. Если часть
/// переменных группы задана, а часть — нет, недонастроенная группа
/// пропускается с предупреждением в лог (секреты не логируются).
pub fn from_env() -> Option<PushConfig> {
    from_env_via(|key| {
        std::env::var(key)
            .ok()
            .filter(|value| !value.trim().is_empty())
    })
}

/// Внутренняя, тестируемая версия [`from_env`] с абстрактным источником значений.
pub(crate) fn from_env_via<F>(get: F) -> Option<PushConfig>
where
    F: Fn(&str) -> Option<String>,
{
    let fcm = build_fcm_config(&get);
    let apns = build_apns_config(&get);
    if fcm.is_none() && apns.is_none() {
        None
    } else {
        Some(PushConfig { fcm, apns })
    }
}

/// Собрать [`FcmConfig`] из источника переменных.
fn build_fcm_config<F>(get: &F) -> Option<FcmConfig>
where
    F: Fn(&str) -> Option<String>,
{
    let project_id = get("FCM_PROJECT_ID");
    let service_account = get("FCM_SERVICE_ACCOUNT_JSON");
    match (project_id, service_account) {
        (Some(project_id), Some(service_account_json)) => Some(FcmConfig {
            project_id,
            service_account_json,
        }),
        (None, None) => None,
        _ => {
            tracing::warn!(
                target: "dropit.push.config",
                "FCM partially configured: missing FCM_PROJECT_ID or FCM_SERVICE_ACCOUNT_JSON, \
                 FCM transport disabled"
            );
            None
        }
    }
}

/// Собрать [`ApnsConfig`] из источника переменных.
fn build_apns_config<F>(get: &F) -> Option<ApnsConfig>
where
    F: Fn(&str) -> Option<String>,
{
    let team_id = get("APNS_TEAM_ID");
    let key_id = get("APNS_KEY_ID");
    let private_key = get("APNS_PRIVATE_KEY_P8");
    let topic = get("APNS_TOPIC");

    let required = [&team_id, &key_id, &private_key, &topic];
    let present = required.iter().filter(|opt| opt.is_some()).count();
    if present == 0 {
        return None;
    }
    if present != required.len() {
        tracing::warn!(
            target: "dropit.push.config",
            "APNs partially configured: missing one of APNS_TEAM_ID/APNS_KEY_ID/\
             APNS_PRIVATE_KEY_P8/APNS_TOPIC, APNs transport disabled"
        );
        return None;
    }
    // Безопасно: выше гарантировано, что все четыре значения — `Some`.
    let (Some(team_id), Some(key_id), Some(private_key_p8), Some(topic)) =
        (team_id, key_id, private_key, topic)
    else {
        return None;
    };
    let sandbox = get("APNS_SANDBOX").map(|v| parse_bool(&v)).unwrap_or(false);
    Some(ApnsConfig {
        team_id,
        key_id,
        private_key_p8,
        topic,
        sandbox,
    })
}

/// Разобрать boolean из строкового значения переменной окружения.
fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}
