//! Антифрод-правила (F8; спецификация — техкарта Часть VII
//! «Антифрод-грань лимитов», лимиты вводятся в F2/F4, реализация — F8).
//!
//! Грани:
//!   * max N запросов/устройство/сессию (шторм от одного девайса);
//!   * max M устройств/сессию (QR-шаринг массовый — алерт клубу);
//!   * повторный QR-скан = re-grant (легален, не флуд).
//!
//! Шаринг QR-ссылкой — ПРИНЯТАЯ модель (треки платные): гостей
//! не ограничиваем, кроме лимитов выше. Правила возвращают
//! решение allow/deny + счётчик для алерта.

/// Лимит запросов на устройство в сессии (MVP-дефолт; конфиг).
pub const MAX_REQUESTS_PER_DEVICE_PER_SESSION: i64 = 20;
/// Лимит уникальных устройств в сессии (алерт клубу сверх).
pub const MAX_DEVICES_PER_SESSION: i64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FraudVerdict {
    /// Пропустить.
    Allow,
    /// Отклонить: лимит устройства.
    DenyDeviceRequestLimit { count: i64, limit: i64 },
    /// Пропустить, но уведомить клуб (массовый шаринг).
    AlertDevices { count: i64, limit: i64 },
}

/// Проверка лимита запросов устройства в сессии.
pub fn check_request_limit(count: i64) -> FraudVerdict {
    if count >= MAX_REQUESTS_PER_DEVICE_PER_SESSION {
        FraudVerdict::DenyDeviceRequestLimit {
            count,
            limit: MAX_REQUESTS_PER_DEVICE_PER_SESSION,
        }
    } else {
        FraudVerdict::Allow
    }
}

/// Проверка числа устройств в сессии (после join).
pub fn check_session_devices(count: i64) -> FraudVerdict {
    if count > MAX_DEVICES_PER_SESSION {
        FraudVerdict::AlertDevices {
            count,
            limit: MAX_DEVICES_PER_SESSION,
        }
    } else {
        FraudVerdict::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_limit_denies_at_threshold() {
        assert_eq!(check_request_limit(0), FraudVerdict::Allow);
        assert_eq!(check_request_limit(19), FraudVerdict::Allow);
        assert_eq!(
            check_request_limit(20),
            FraudVerdict::DenyDeviceRequestLimit {
                count: 20,
                limit: 20
            }
        );
    }

    #[test]
    fn devices_alert_not_deny() {
        // Шаринг QR — принятая модель: не deny, а алерт.
        assert_eq!(check_session_devices(300), FraudVerdict::Allow);
        assert_eq!(
            check_session_devices(301),
            FraudVerdict::AlertDevices {
                count: 301,
                limit: 300
            }
        );
    }
}
