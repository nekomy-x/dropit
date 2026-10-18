//! Реле outbox → шина: [`OutboxRelay`] и отчёт [`RelayReport`].

use std::sync::Arc;

use uuid::Uuid;

use crate::bus::MessageBus;
use crate::error::NatsError;
use crate::outbox::{subject_for, OutboxStore};

/// Отчёт одного цикла [`OutboxRelay::run_once`].
///
/// Инвариант: `claimed == published + failed + skipped`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RelayReport {
    /// Захвачено строк из хранилища.
    pub claimed: usize,
    /// Успешно опубликовано и помечено `mark_published`.
    pub published: usize,
    /// Провалилось при публикации (увеличено `attempts`).
    pub failed: usize,
    /// Пропущено: `attempts >= max_attempts` (poisoned/исчерпаны).
    pub skipped: usize,
}

/// Реле transactional outbox (ADR-002).
///
/// Читает батч невыпущенных событий из `S` ([`OutboxStore`]), публикует каждое
/// в шину `bus` ([`MessageBus`]) и помечает результат. Безопасно вызывать
/// повторно: опубликованные строки исключаются хранилищем из следующих батчей.
///
/// # Семантика частичного сбоя
///
/// Если публикация одной строки упала, реле:
/// 1. помечает эту строку через [`OutboxStore::mark_failed`] (`attempts++`,
///    причина сохраняется);
/// 2. **продолжает** остаток батча — оставшиеся строки всё равно пытаются
///    опубликоваться, ни одна не теряется;
/// 3. успешно опубликованные в этом цикле помечаются одним пакетным вызовом
///    [`OutboxStore::mark_published`].
///
/// Таким образом failing-строка изолируется, а rest-батч обрабатывается.
///
/// # Доставка
///
/// Доставка **at-least-once**: если `mark_published` упадёт после успешной
/// публикации в NATS, строка будет переиздана следующим циклом. Потребители
/// обязаны быть идемпотентными по `(aggregate_id, event_type)` (ADR-002).
///
/// # Исчерпание попыток
///
/// Строки с `attempts >= max_attempts` не публикуются и попадают в
/// [`RelayReport::skipped`] (poisoned). Они остаются в хранилище невыпущенными;
/// продакшен-стороне следует отрабатывать их в DLQ.
pub struct OutboxRelay<S: OutboxStore + ?Sized> {
    store: Arc<S>,
    bus: Arc<dyn MessageBus>,
    batch_size: i64,
    max_attempts: i32,
}

impl<S: OutboxStore + ?Sized> OutboxRelay<S> {
    /// Создать реле.
    ///
    /// `batch_size <= 0` означает пустой батч (реля простаивает); `max_attempts`
    /// задаёт порог skip: строка с `attempts >= max_attempts` не публикуется.
    /// При `max_attempts <= 0` все строки считаются исчерпавшими попытки.
    pub fn new(
        store: Arc<S>,
        bus: Arc<dyn MessageBus>,
        batch_size: i64,
        max_attempts: i32,
    ) -> Self {
        Self {
            store,
            bus,
            batch_size,
            max_attempts,
        }
    }

    /// Выполнить один цикл релия.
    ///
    /// Возвращает [`RelayReport`] с разбивкой по исходам. Пустой батч
    /// возвращает `Ok(RelayReport::default())` без обращений к шине.
    pub async fn run_once(&self) -> Result<RelayReport, NatsError> {
        let rows = self.store.claim_batch(self.batch_size).await?;
        let mut report = RelayReport {
            claimed: rows.len(),
            ..RelayReport::default()
        };

        if rows.is_empty() {
            return Ok(report);
        }

        let mut published_ids: Vec<Uuid> = Vec::new();
        for row in rows {
            // Исчерпавшие попытки — не публикуем, в skip.
            if row.attempts >= self.max_attempts {
                report.skipped += 1;
                continue;
            }

            let subject = subject_for(&row.event_type);
            let payload = serde_json::to_vec(&row.payload)
                .map_err(|e| NatsError::Poisoned(format!("payload encode: {e}")))?;

            match self.bus.publish(&subject, &payload).await {
                Ok(()) => published_ids.push(row.id),
                Err(err) => {
                    let reason = err.to_string();
                    // mark_failed — сбой хранилища критичен: прерываем цикл.
                    // Остаток батча остаётся невыпущенным и будет переиздан.
                    self.store.mark_failed(row.id, &reason).await?;
                    report.failed += 1;
                    // Продолжаем остаток батча — он не теряется.
                }
            }
        }

        if !published_ids.is_empty() {
            let marked = self.store.mark_published(&published_ids).await?;
            if (marked as usize) < published_ids.len() {
                tracing::warn!(
                    marked,
                    expected = published_ids.len(),
                    "mark_published updated fewer rows than published"
                );
            }
            report.published = published_ids.len();
        }

        Ok(report)
    }
}
