//! Джобы и антифрод-счётчики (F8).
//!
//! ТРИ ДЖОБЫ (прод: cron-воркеры; здесь — методы, вызываемые
//! админ-триггерами и тестами):
//!   * авто-закрытие сессий: 07:00 в IANA-зоне клуба при
//!     неактивности ≥ 2 ч (домен qr.rs, Q5);
//!   * TTL-интенты: payment(intent) старше 15 мин → cancelling
//!     (отмена У ПРОВАЙДЕРА — вызывающий слой);
//!   * GDPR: retention 180 дней — псевдонимизация гостевых записей
//!     (финансы НЕ удаляются, device → anon).
//!
//! Антифрод-счётчики: запросов/устройство/сессию, устройств/сессию —
//! для домен-правил fraud.rs.

use chrono::Utc;
use dropit_domain::{
    check_request_limit, check_session_devices, qr, FraudVerdict, SessionTransition,
};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum JobsError {
    #[error("database error")]
    Db(#[from] sqlx::Error),
    #[error("crypto error")]
    Crypto(#[from] dropit_common::SecretCryptoError),
    #[error("malformed timezone")]
    BadTimezone,
}

#[derive(Debug, Clone)]
pub struct ClosedSession {
    pub session_id: Uuid,
    pub reason: &'static str, // "auto_close_planned" | "auto_close_inactivity"
}

#[derive(Clone)]
pub struct JobsRepo {
    pool: PgPool,
}

impl JobsRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    // ---------------------------------------------------------
    // Джоба 1: авто-закрытие сессий (07:00 IANA, Q5)
    // ---------------------------------------------------------

    /// Закрывает сессии, для которых выполнены условия Q5:
    /// достигнуто плановое время закрытия (QR expires_at) И
    /// неактивность ≥ 2 ч. «Активность» = last request/payment
    /// (created_at) —_WS-события пока не считаются (F9-метрики).
    /// Возвращает закрытые сессии (для уведомлений/лога).
    pub async fn auto_close_sessions(&self) -> Result<Vec<ClosedSession>, JobsError> {
        // Незакрытые сессии с их плановым временем (QR) и активностью.
        #[derive(sqlx::FromRow)]
        struct OpenRow {
            id: Uuid,
            #[allow(dead_code)]
            timezone: String,
            planned: Option<chrono::DateTime<Utc>>,
            last_activity: Option<chrono::DateTime<Utc>>,
        }
        let rows: Vec<OpenRow> = sqlx::query_as(
                "SELECT s.id, s.timezone, \
                 (SELECT max(q.expires_at) FROM qr_token q WHERE q.session_id = s.id) AS planned, \
                 (SELECT max(r.created_at) FROM request r WHERE r.session_id = s.id) AS last_activity \
                 FROM session s WHERE s.status <> 'closed'",
            )
            .fetch_all(&self.pool)
            .await?;
        let mut closed = Vec::new();
        for OpenRow {
            id: session_id,
            timezone: tz_name,
            planned,
            last_activity,
        } in rows
        {
            let Some(planned) = planned else {
                continue; // нет QR — нет планового закрытия
            };
            // Плановое время по Q5: 07:00 локально в зоне клуба —
            // QR expires_at уже вычислен по IANA-зоне (F2: qr.rs).
            let now = Utc::now();
            let last = last_activity.unwrap_or(planned);
            if qr::should_auto_close(last, planned, now)
                || now >= planned && (now - last) >= dropit_domain::qr::INACTIVITY_THRESHOLD
            {
                let res = sqlx::query(
                    "UPDATE session SET status = 'closed', closed_at = now() \
                     WHERE id = $1 AND status <> 'closed'",
                )
                .bind(session_id)
                .execute(&self.pool)
                .await?;
                if res.rows_affected() == 1 {
                    // Гасим pending-запросы закрытой сессии (expired).
                    sqlx::query(
                        "UPDATE request SET status = 'expired' \
                         WHERE session_id = $1 AND status IN ('pending', 'accepted')",
                    )
                    .bind(session_id)
                    .execute(&self.pool)
                    .await?;
                    closed.push(ClosedSession {
                        session_id,
                        reason: "auto_close_planned",
                    });
                }
            }
            let _ = &tz_name;
        }
        Ok(closed)
    }

    // ---------------------------------------------------------
    // Джоба 2: TTL-интенты (15 мин) → cancelling
    // ---------------------------------------------------------

    /// TTL интентов (техкарта V: гость закрыл экран, 15 мин).
    /// intent → cancelling; фактическая отмена У ПРОВАЙДЕРА —
    /// вызывающий воркер (после — webhook cancelled).
    /// Возвращает payment_id, переведённые в cancelling.
    pub async fn expire_stale_intents(&self) -> Result<Vec<Uuid>, JobsError> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            "UPDATE payment SET status = 'cancelling' \
             WHERE status IN ('intent', 'processing') AND created_at < now() - interval '15 minutes' \
             RETURNING id",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(u,)| u).collect())
    }

    // ---------------------------------------------------------
    // Антифрод-счётчики (для fraud.rs)
    // ---------------------------------------------------------

    /// Запросов устройства в сессии.
    pub async fn request_count(
        &self,
        session_id: Uuid,
        guest_device_id: Uuid,
    ) -> Result<i64, JobsError> {
        let (c,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM request WHERE session_id = $1 AND guest_device_id = $2",
        )
        .bind(session_id)
        .bind(guest_device_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(c)
    }

    /// Уникальных устройств в сессии (по грантам).
    pub async fn session_device_count(&self, session_id: Uuid) -> Result<i64, JobsError> {
        let (c,): (i64,) = sqlx::query_as(
            "SELECT count(DISTINCT guest_device_id) FROM session_guest \
             WHERE session_id = $1 AND revoked_at IS NULL",
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(c)
    }

    /// Верификатор запроса: антифрод-грань + домен-правило.
    /// Deny → Some(верdict) — вызывающий отвечает 429.
    pub async fn fraud_check_request(
        &self,
        session_id: Uuid,
        guest_device_id: Uuid,
    ) -> Result<Option<FraudVerdict>, JobsError> {
        let count = self.request_count(session_id, guest_device_id).await?;
        Ok(match check_request_limit(count) {
            FraudVerdict::Allow => None,
            deny => Some(deny),
        })
    }

    /// Алерт клубу при массовом шаринге (после join).
    pub async fn session_devices_alert(
        &self,
        session_id: Uuid,
    ) -> Result<Option<FraudVerdict>, JobsError> {
        let count = self.session_device_count(session_id).await?;
        Ok(match check_session_devices(count) {
            FraudVerdict::Allow => None,
            alert => Some(alert),
        })
    }

    // ---------------------------------------------------------
    // Джоба 3: GDPR (retention 180 дней, псевдонимизация)
    // ---------------------------------------------------------

    /// Псевдонимизация устройства: финансовые записи НЕ удаляются —
    /// guest_device.platform/app_version затираются, гранты ревок,
    /// DSR-трейл в gdpr_action. Возвращает число затронутых записей.
    pub async fn anonymize_guest_device(
        &self,
        guest_device_id: Uuid,
        performed_by: Option<Uuid>,
    ) -> Result<i64, JobsError> {
        let mut tx = self.pool.begin().await?;
        let res = sqlx::query(
            "UPDATE guest_device SET platform = 'anonymized', app_version = NULL, \
             account_id = NULL WHERE id = $1",
        )
        .bind(guest_device_id)
        .execute(&mut *tx)
        .await?;
        if res.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(0); // устройства нет — ничего не делаем
        }
        sqlx::query(
            "UPDATE session_guest SET revoked_at = now() WHERE guest_device_id = $1 AND revoked_at IS NULL",
        )
        .bind(guest_device_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO gdpr_action (id, action, guest_device_id, performed_by, details) \
             VALUES ($1, 'anonymize', $2, $3, $4)",
        )
        .bind(Uuid::now_v7())
        .bind(guest_device_id)
        .bind(performed_by)
        .bind(serde_json::json!({ "financials": "kept", "device_fields": "erased" }))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(1)
    }

    /// Retention-sweep: устройства, неактивные 180 дней, —
    /// плановая псевдонимизация (та же механика, без субъекта).
    pub async fn retention_sweep(&self) -> Result<Vec<Uuid>, JobsError> {
        let ids: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM guest_device WHERE last_seen < now() - interval '180 days' \
             AND platform <> 'anonymized'",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut done = Vec::new();
        for (id,) in ids {
            if self.anonymize_guest_device(id, None).await? > 0 {
                done.push(id);
            }
        }
        Ok(done)
    }

    /// Экспорт данных субъекта (DSR): запросы, гранты, платежи —
    /// всё, что связано с устройством. Финансы остаются в БД.
    pub async fn export_guest_data(
        &self,
        guest_device_id: Uuid,
    ) -> Result<serde_json::Value, JobsError> {
        let requests: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT jsonb_build_object('request_id', r.id, 'status', r.status, \
             'created_at', r.created_at, 'title', t.title, 'artist', t.artist) \
             FROM request r JOIN track_cache t ON t.id = r.track_cache_id \
             WHERE r.guest_device_id = $1 ORDER BY r.created_at DESC",
        )
        .bind(guest_device_id)
        .fetch_all(&self.pool)
        .await?;
        let grants: Vec<(Uuid, chrono::DateTime<Utc>)> = sqlx::query_as(
            "SELECT session_id, joined_at FROM session_guest WHERE guest_device_id = $1",
        )
        .bind(guest_device_id)
        .fetch_all(&self.pool)
        .await?;
        let payments: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT jsonb_build_object('payment_id', p.id, 'status', p.status, \
             'amount_minor', p.amount_minor, 'currency', p.currency, 'request_id', p.request_id) \
             FROM payment p JOIN request r ON r.id = p.request_id \
             WHERE r.guest_device_id = $1",
        )
        .bind(guest_device_id)
        .fetch_all(&self.pool)
        .await?;
        sqlx::query(
            "INSERT INTO gdpr_action (id, action, guest_device_id, details) \
             VALUES ($1, 'export', $2, $3)",
        )
        .bind(Uuid::now_v7())
        .bind(guest_device_id)
        .bind(serde_json::json!({ "requests": requests.len(), "payments": payments.len() }))
        .execute(&self.pool)
        .await?;
        Ok(serde_json::json!({
            "guest_device_id": guest_device_id,
            "requests": requests,
            "session_grants": grants.into_iter().map(|(s, j)| serde_json::json!({"session_id": s, "joined_at": j})).collect::<Vec<_>>(),
            "payments": payments,
        }))
    }

    // ---------------------------------------------------------
    // Push-токены гостей (для диспетчера уведомлений воркера)
    // ---------------------------------------------------------

    /// Активные (не отозванные) push-токены гостя.
    ///
    /// Возвращает токены с платформой (`'android'` / `'ios'`) из таблицы
    /// `guest_push_token` (миграция 0009). Отозванные (`revoked_at IS NOT
    /// NULL`) исключаются — по ним повторная отправка не выполняется.
    pub async fn active_push_tokens(
        &self,
        guest_id: Uuid,
    ) -> Result<Vec<PushTokenRow>, JobsError> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT token, platform FROM guest_push_token \
             WHERE guest_id = $1 AND revoked_at IS NULL",
        )
        .bind(guest_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(token, platform)| PushTokenRow { token, platform })
            .collect())
    }

    /// Отозвать невалидный push-токен: `revoked_at = now()`.
    ///
    /// Идемпотентно: повторный вызов по уже отозванному токену — no-op
    /// (возвращает `false`). Вызывается диспетчером push, когда транспорт
    /// признал токен невалидным (`UNREGISTERED`/`BadDeviceToken`).
    pub async fn revoke_push_token(&self, token: &str) -> Result<bool, JobsError> {
        let res = sqlx::query(
            "UPDATE guest_push_token SET revoked_at = now() \
             WHERE token = $1 AND revoked_at IS NULL",
        )
        .bind(token)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() > 0)
    }
}

/// Активный push-токен гостя (для диспетчера уведомлений).
///
/// `platform` — строка из БД (`'android'` / `'ios'`); вызывающий слой
/// (воркер) сопоставляет её с [`dropit_infra_push::Platform`].
#[derive(Debug, Clone)]
pub struct PushTokenRow {
    /// Регистрационный токен устройства.
    pub token: String,
    /// Платформа: `'android'` или `'ios'`.
    pub platform: String,
}

// Стейт-переход используется вызывающими воркерами.
#[allow(dead_code)]
fn _session_transition_hint() -> Option<SessionTransition> {
    None
}
