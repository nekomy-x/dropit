-- DROP IT — миграция 0007: GDPR и retention (F8).
--
-- ПРАВИЛА (техкарта VII, «Безопасность — чек-лист»):
--   * guest-данные — retention 180 дней; финансовые записи НЕ
--     удаляются, а ПСЕВДОНИМИЗИРУЮТСЯ (detach device → anon-id);
--   * DSR (Data Subject Request) = export + анонимизация, не delete;
--   * бэкапы: restore-drill — эксплуатация (не схема).

-- ============================================================
-- Журнал GDPR-действий: экспорт/анонимизация по запросу субъекта
-- и плановая чистка устаревших устройств.
-- ============================================================
CREATE TABLE gdpr_action (
    id            UUID PRIMARY KEY,          -- UUIDv7
    action        TEXT NOT NULL,             -- 'export' | 'anonymize' | 'retention_sweep'
    guest_device_id UUID,                    -- субъект (NULL для sweep-прогона)
    performed_by  UUID,                      -- admin_user.id (NULL = система)
    details       JSONB,                     -- что вошло в экспорт / что затёрто
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_gdpr_action_device ON gdpr_action (guest_device_id) WHERE guest_device_id IS NOT NULL;
