-- DROP IT — миграция 0001: скелет схемы (F0A).
-- Полная модель данных — Часть II технической карты; таблицы вводятся
-- по фазам (identity → F1, sessions/QR → F2, деньги → F5) через
-- expand-миграции. Здесь только фундамент: платформенные настройки
-- и аудит-лог, чтобы зафиксировать базовые инварианты с первого дня.
--
-- ПРАВИЛА (ADR-003):
--   * суммы BIGINT в минорных единицах (центы), float запрещён;
--   * время TIMESTAMPTZ UTC; ID — UUID v7;
--   * значения комиссии/цены здесь НЕ задаются — их задаёт владелец.

CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- ============================================================
-- platform_settings: конфигурация платформы (одна строка, id=1).
-- Значения комиссий/пределов устанавливаются владельцем через
-- админ-панель (F7); в коде нет захардкоженных бизнес-значений.
-- ============================================================
CREATE TABLE platform_settings (
    id                      SMALLINT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    commission_mode         TEXT NOT NULL DEFAULT 'unset',       -- 'percent' | 'fixed' | 'unset'
    commission_percent_bp   INTEGER,                             -- бипсисы, 10000 = 100%
    commission_fixed_minor  BIGINT,                              -- фикс в минорных единицах
    min_price_minor         BIGINT,
    max_price_minor         BIGINT,
    dj_share_of_remainder_bp INTEGER,                            -- доля DJ в остатке, бипсисы
    currency                TEXT NOT NULL DEFAULT 'EUR',
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Пустая строка настроек: значения NULL = «не заданы владельцем».
INSERT INTO platform_settings (id, commission_mode) VALUES (1, 'unset');

-- ============================================================
-- audit_log: append-only журнал административных действий.
-- Партиционирование по месяцам — отдельная миграция в F6/F8
-- (partition-manager), сейчас одна стартовая партиция.
-- ============================================================
CREATE TABLE audit_log (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_type  TEXT NOT NULL,           -- 'admin' | 'club' | 'dj' | 'system'
    actor_id    UUID,
    action      TEXT NOT NULL,
    entity      TEXT NOT NULL,
    diff        JSONB,
    at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_audit_log_at ON audit_log (at DESC);
