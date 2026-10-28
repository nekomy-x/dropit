//! Публичный статус стенда: `GET /v1/status`.
//!
//! Один и тот же бандл панелей стоит на тестовом стенде и в проде, поэтому
//! «какой сейчас режим» (test / maintenance / normal) определяет сервер.
//! Нижний статус-бар панелей опрашивает этот роут один раз при загрузке
//! страницы; авторизация не нужна — это те же факты, что отдаёт `/readyz`.
//!
//! Источник правды — `platform_settings.runtime_mode` (миграция 0016),
//! которую владелец задаёт в админ-панели. Недоступная БД не валит роут:
//! режим деградирует до env-флага или адаптивных фактов стенда, ответ
//! всегда `200`.
//!
//! Чистое ядро [`resolve_mode`] отделено от окружения и БД — матрица
//! разрешения проверяется unit-тестами без сети и без Postgres.

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use serde_json::json;

use crate::routes::{payments_mode, ApiEnv};

/// Допустимые значения `platform_settings.runtime_mode` (контракт
/// `PATCH /v1/admin/platform-settings` и `GET /v1/status`).
pub const RUNTIME_MODES: [&str; 4] = ["auto", "test", "maintenance", "normal"];

/// Таймаут чтения настройки из БД: страницы опрашивают `/v1/status` при
/// каждой загрузке, зависший коннект не должен вешать запрос.
const DB_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1_500);

/// Валидно ли значение `runtime_mode` из тела PATCH (или из БД).
pub fn is_valid_runtime_mode(value: &str) -> bool {
    RUNTIME_MODES.contains(&value)
}

/// Результат разрешения режима стенда — тело `GET /v1/status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedMode {
    /// Эффективный режим для статус-бара: `test` | `maintenance` | `normal`.
    pub mode: &'static str,
    /// Откуда он взялся: `admin` | `env` | `adaptive`.
    pub source: &'static str,
    /// Сырое значение настройки: `auto` | `test` | `maintenance` | `normal`.
    pub runtime_mode: &'static str,
}

/// Чистое разрешение режима — без БД и env, поэтому вся матрица
/// проверяется unit-тестами.
///
/// Порядок (контракт `GET /v1/status`):
///
/// 1. `db_runtime_mode` прочитан и это не `auto` → берём его как есть,
///    `source = "admin"` (ручное решение владельца сильнее всего);
/// 2. `auto` или БД недоступна (`db_runtime_mode == None`) и
///    `DROPIT_MAINTENANCE` включён → `maintenance`, `source = "env"`;
/// 3. иначе адаптивно: открытая регистрация (`open_signup`) или
///    mock-платежи (`payments_mock`) → `test`, иначе `normal`,
///    `source = "adaptive"`.
///
/// `None` означает «БД не сконфигурирована/недоступна/строки нет»:
/// публичный статус обязан отвечать `200` даже без БД, поэтому None
/// трактуется как `auto` — тогда работают шаги 2 и 3.
pub fn resolve_mode(
    db_runtime_mode: Option<&str>,
    maintenance_env: bool,
    open_signup: bool,
    payments_mock: bool,
) -> ResolvedMode {
    // Неизвестное значение (в БД его не пустит CHECK, но защищаемся от
    // старых/битых строк) трактуем как `auto`, а не как режим.
    let runtime_mode = match db_runtime_mode {
        Some("test") => "test",
        Some("maintenance") => "maintenance",
        Some("normal") => "normal",
        _ => "auto",
    };
    if runtime_mode != "auto" {
        return ResolvedMode {
            mode: runtime_mode,
            source: "admin",
            runtime_mode,
        };
    }
    if maintenance_env {
        return ResolvedMode {
            mode: "maintenance",
            source: "env",
            runtime_mode,
        };
    }
    let mode = if open_signup || payments_mock {
        "test"
    } else {
        "normal"
    };
    ResolvedMode {
        mode,
        source: "adaptive",
        runtime_mode,
    }
}

/// Включающие значения `DROPIT_MAINTENANCE` (без учёта регистра, после
/// `trim`): `1`, `true`, `yes`, `on`. Всё остальное — выключено, включая
/// отсутствие переменной (безопасный дефолт: прод не уходит в maintenance
/// из-за опечатки в значении).
pub fn env_truthy(raw: Option<&str>) -> bool {
    raw.map(|v| v.trim().to_ascii_lowercase())
        .is_some_and(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
}

/// `DROPIT_MAINTENANCE` из окружения процесса (читается на каждый запрос —
/// переключение флага на живом процессе не требует рестарта).
fn maintenance_from_env() -> bool {
    env_truthy(std::env::var("DROPIT_MAINTENANCE").ok().as_deref())
}

/// Сырое `platform_settings.runtime_mode` или `None`, если БД не
/// сконфигурирована/недоступна/строки нет. Ошибки и таймаут не
/// пробрасываются: None для [`resolve_mode`] = `auto` (шаги 2/3).
async fn read_db_runtime_mode(state: &ApiEnv) -> Option<String> {
    let pool = state.repo.as_ref()?.pool();
    let query =
        sqlx::query_scalar::<_, String>("SELECT runtime_mode FROM platform_settings WHERE id = 1")
            .fetch_optional(&pool);
    match tokio::time::timeout(DB_READ_TIMEOUT, query).await {
        Ok(Ok(mode)) => mode,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "status: runtime_mode read failed; falling back to auto");
            None
        }
        Err(_) => {
            tracing::warn!("status: runtime_mode read timed out; falling back to auto");
            None
        }
    }
}

/// `GET /v1/status` — публичный, без авторизации.
async fn runtime_status(State(state): State<ApiEnv>) -> impl IntoResponse {
    let payments = payments_mode();
    let db_mode = read_db_runtime_mode(&state).await;
    let resolved = resolve_mode(
        db_mode.as_deref(),
        maintenance_from_env(),
        state.open_signup,
        payments == "mock",
    );
    (
        StatusCode::OK,
        Json(json!({
            "mode": resolved.mode,
            "source": resolved.source,
            "runtime_mode": resolved.runtime_mode,
            "open_signup": state.open_signup,
            "payments": payments,
        })),
    )
}

/// Публичный роутер статуса: монтируется в общий публичный роутер.
pub fn status_router() -> Router<ApiEnv> {
    Router::new().route("/v1/status", get(runtime_status))
}

#[cfg(test)]
mod tests {
    use super::{env_truthy, is_valid_runtime_mode, resolve_mode};

    /// Админ задал режим вручную — он побеждает env и адаптивные факты,
    /// включая включённый `DROPIT_MAINTENANCE`.
    #[test]
    fn admin_mode_wins_over_env_and_adaptive() {
        for mode in ["test", "maintenance", "normal"] {
            let r = resolve_mode(Some(mode), true, true, true);
            assert_eq!(r.mode, mode, "admin mode {mode} must win");
            assert_eq!(r.source, "admin");
            assert_eq!(r.runtime_mode, mode);
        }
    }

    /// `auto` + включённый env — maintenance из env (не адаптивный test).
    #[test]
    fn auto_with_env_is_maintenance() {
        let r = resolve_mode(Some("auto"), true, true, true);
        assert_eq!(r.mode, "maintenance");
        assert_eq!(r.source, "env");
        assert_eq!(r.runtime_mode, "auto");
    }

    /// `auto`, env выключен, открытая регистрация → адаптивный test.
    #[test]
    fn auto_open_signup_is_adaptive_test() {
        let r = resolve_mode(Some("auto"), false, true, false);
        assert_eq!(r.mode, "test");
        assert_eq!(r.source, "adaptive");
        assert_eq!(r.runtime_mode, "auto");
    }

    /// `auto`, env выключен, закрытая регистрация, mock-платежи → test.
    #[test]
    fn auto_mock_payments_is_adaptive_test() {
        let r = resolve_mode(Some("auto"), false, false, true);
        assert_eq!(r.mode, "test");
        assert_eq!(r.source, "adaptive");
    }

    /// `auto`, env выключен, живой стенд (регистрация закрыта, платежи
    /// live) → normal.
    #[test]
    fn auto_live_closed_signup_is_normal() {
        let r = resolve_mode(Some("auto"), false, false, false);
        assert_eq!(r.mode, "normal");
        assert_eq!(r.source, "adaptive");
        assert_eq!(r.runtime_mode, "auto");
    }

    /// БД недоступна (None): роут деградирует, а не падает. Env включён →
    /// maintenance/env.
    #[test]
    fn db_unavailable_with_env_is_maintenance() {
        let r = resolve_mode(None, true, false, false);
        assert_eq!(r.mode, "maintenance");
        assert_eq!(r.source, "env");
        assert_eq!(r.runtime_mode, "auto", "no DB row → auto");
    }

    /// БД недоступна и env выключен → адаптивные факты стенда.
    #[test]
    fn db_unavailable_without_env_is_adaptive() {
        let test = resolve_mode(None, false, true, false);
        assert_eq!(test.mode, "test");
        assert_eq!(test.source, "adaptive");

        let normal = resolve_mode(None, false, false, false);
        assert_eq!(normal.mode, "normal");
        assert_eq!(normal.source, "adaptive");
    }

    /// Неизвестное/битое значение в БД не ломает разрешение: трактуется как
    /// `auto` (защита от старых строк в обход CHECK).
    #[test]
    fn unknown_db_value_falls_back_to_auto() {
        let r = resolve_mode(Some("production"), false, false, false);
        assert_eq!(r.runtime_mode, "auto");
        assert_eq!(r.mode, "normal");
        assert_eq!(r.source, "adaptive");
    }

    /// Разбор `DROPIT_MAINTENANCE`: включающие значения — 1/true/yes/on
    /// без учёта регистра и пробелов; остальное (включая отсутствие) —
    /// выключено.
    #[test]
    fn env_truthy_matrix() {
        for raw in ["1", "true", "TRUE", " yes ", "On", " on\n"] {
            assert!(env_truthy(Some(raw)), "{raw:?} must enable maintenance");
        }
        for raw in ["0", "false", "no", "off", "", "  ", "enable", "2"] {
            assert!(!env_truthy(Some(raw)), "{raw:?} must not enable");
        }
        assert!(!env_truthy(None));
    }

    /// Enum runtime_mode: ровно четыре значения контракта.
    #[test]
    fn runtime_mode_enum() {
        for mode in ["auto", "test", "maintenance", "normal"] {
            assert!(is_valid_runtime_mode(mode), "{mode} must be valid");
        }
        for mode in ["production", "", "Auto", "maintenance "] {
            assert!(!is_valid_runtime_mode(mode), "{mode:?} must be invalid");
        }
    }
}
