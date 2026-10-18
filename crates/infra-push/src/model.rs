//! Модели push-сообщений, токенов и отчётов по отправке.

use std::collections::BTreeMap;

/// Платформа получателя — определяет транспорт доставки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    /// Android — доставка через FCM HTTP v1.
    Android,
    /// iOS — доставка через APNs HTTP/2.
    Ios,
}

/// Регистрационный токен устройства (FCM registration token или APNs device token).
#[derive(Debug, Clone)]
pub struct PushToken {
    /// Сам токен устройства. Секрет: не логируется.
    pub token: String,
    /// Платформа получателя.
    pub platform: Platform,
}

/// Тело push-уведомления, единое для всех платформ.
#[derive(Debug, Clone)]
pub struct PushMessage {
    /// Заголовок уведомления (`notification.title` / `aps.alert.title`).
    pub title: String,
    /// Текст уведомления (`notification.body` / `aps.alert.body`).
    pub body: String,
    /// Произвольные строковые данные (data payload). Ключи `aps` и `route`
    /// зарезервированы транспортом и игнорируются здесь.
    pub data: BTreeMap<String, String>,
    /// Ключ сворачивания: `android.collapse_key` для FCM и
    /// заголовок `apns-collapse-id` для APNs.
    pub collapse_key: Option<String>,
    /// Внутренний маршрут приложения для deep-link (например, `request/<id>`).
    /// Попадает в data-payload FCM и в корень тела APNs.
    pub route: Option<String>,
}

/// Результат отправки одному токену.
#[derive(Debug, Clone)]
pub struct PushOutcome {
    /// Токен, к которому относится результат.
    pub token: String,
    /// Успешно доставлено.
    pub ok: bool,
    /// Токен признан невалидным (`UNREGISTERED`/`BadDeviceToken` и т.п.) —
    /// его следует удалить из хранилища устройства.
    pub invalid_token: bool,
    /// Текст ошибки (без секретов), если отправка не удалась.
    pub error: Option<String>,
}

impl PushOutcome {
    /// Конструктор успешного результата.
    pub fn success(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            ok: true,
            invalid_token: false,
            error: None,
        }
    }

    /// Конструктор результата с невалидным токеном.
    pub fn invalid(token: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            ok: false,
            invalid_token: true,
            error: Some(reason.into()),
        }
    }

    /// Конструктор результата с обычной ошибкой (токен остаётся валидным).
    pub fn failed(token: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            ok: false,
            invalid_token: false,
            error: Some(reason.into()),
        }
    }
}

/// Сводный отчёт по пакетной отправке.
#[derive(Debug, Clone, Default)]
pub struct PushReport {
    /// Количество успешно отправленных токенов.
    pub sent: usize,
    /// Количество токенов с ошибкой отправки.
    pub failed: usize,
    /// Токены, признанные невалидными (для удаления из хранилища).
    pub invalid_tokens: Vec<String>,
    /// По-токенные результаты в порядке обработки.
    pub outcomes: Vec<PushOutcome>,
}

impl PushReport {
    /// Добавить одиночный результат в отчёт, обновив счётчики.
    pub fn add(&mut self, outcome: PushOutcome) {
        if outcome.ok {
            self.sent += 1;
        } else {
            self.failed += 1;
        }
        if outcome.invalid_token {
            self.invalid_tokens.push(outcome.token.clone());
        }
        self.outcomes.push(outcome);
    }

    /// Слить другой отчёт в текущий, просуммировав счётчики и дописав элементы.
    pub fn merge(&mut self, other: PushReport) {
        self.sent += other.sent;
        self.failed += other.failed;
        self.invalid_tokens.extend(other.invalid_tokens);
        self.outcomes.extend(other.outcomes);
    }
}
