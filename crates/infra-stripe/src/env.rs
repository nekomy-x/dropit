//! Чтение учётных данных Stripe из переменных окружения.

/// Переменная окружения с секретным ключом Stripe.
pub const ENV_API_KEY: &str = "STRIPE_API_KEY";

/// Переменная окружения с секретом webhook-подписки.
pub const ENV_WEBHOOK_SECRET: &str = "STRIPE_WEBHOOK_SECRET";

/// Читает пару `(api_key, webhook_secret)` из окружения через обобщённый getter.
///
/// Вынесено отдельно от [`from_env`], чтобы логику валидации можно было
/// покрыть unit-тестами без mutating реального процесса (`std::env::set_var`
/// небезопасен при параллельных тестах).
pub(crate) fn read_credentials<F>(get: F) -> Option<(String, String)>
where
    F: Fn(&str) -> Option<String>,
{
    let key = get(ENV_API_KEY)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())?;
    let secret = get(ENV_WEBHOOK_SECRET)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())?;
    Some((key, secret))
}

/// Читает `STRIPE_API_KEY` / `STRIPE_WEBHOOK_SECRET` из процесса.
///
/// Возвращает `None`, если хотя бы одна переменная отсутствует или пуста.
/// Возвращаемые значения обрезаны от пробелов. Используется [`crate::LiveStripe::from_env`]
/// и gateway-api для выбора между live-клиентом и моком.
pub fn from_env() -> Option<(String, String)> {
    read_credentials(|key| std::env::var(key).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_present_returns_pair() {
        let pair = read_credentials(|k| match k {
            ENV_API_KEY => Some("sk_test_abc ".into()),
            ENV_WEBHOOK_SECRET => Some(" whsec_xyz".into()),
            _ => None,
        });
        assert_eq!(pair, Some(("sk_test_abc".into(), "whsec_xyz".into())));
    }

    #[test]
    fn missing_key_returns_none() {
        let pair = read_credentials(|k| match k {
            ENV_API_KEY => None,
            ENV_WEBHOOK_SECRET => Some("whsec_xyz".into()),
            _ => None,
        });
        assert_eq!(pair, None);
    }

    #[test]
    fn missing_secret_returns_none() {
        let pair = read_credentials(|k| match k {
            ENV_API_KEY => Some("sk_test_abc".into()),
            ENV_WEBHOOK_SECRET => None,
            _ => None,
        });
        assert_eq!(pair, None);
    }

    #[test]
    fn empty_after_trim_returns_none() {
        let pair = read_credentials(|k| match k {
            ENV_API_KEY => Some("   ".into()),
            ENV_WEBHOOK_SECRET => Some("whsec_xyz".into()),
            _ => None,
        });
        assert_eq!(pair, None);
    }

    #[test]
    fn from_env_does_not_panic() {
        // В CI переменные могут отсутствовать — это None, не паника.
        let _ = from_env();
    }
}
