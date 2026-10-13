-- DROP IT — миграция 0006: выплаты и сверка (F6, Test Mode).
-- Модель — Часть II техкарты; стейт-машина Payout — Часть III.
--
-- ПРАВИЛА:
--   * payout: scheduled → in_flight → paid
--             in_flight → failed → (retry ≤5, backoff) → on_hold
--   * transfer-вебхуки идемпотентны по stripe_transfer_id;
--     часовой poller незакрытых in_flight — воркер, упавший после
--     Stripe-вызова, не оставит payout висеть навсегда;
--   * порог 10 € (конфиг Q4): ниже — перенос на след. неделю;
--   * финальная выплата БЕЗ порога при блокировке/уходе клуба;
--   * инвариант ПО КАЖДОМУ payee (не глобально):
--     Σ ledger(payable:X) == Σ split(X,pending)
--       + Σ split(X, в выплатах in_flight|paid) − Σ некомпенс. reversed;
--   * сверка (reconciliation_run): расхождение → incident → алерт.

-- ============================================================
-- Выплаты. Одна строка = одна попытка Transfer одному payee
-- за период. Retry — в той же строке (attempts).
-- ============================================================
CREATE TABLE payout (
    id                    UUID PRIMARY KEY,          -- UUIDv7
    payee_type            TEXT NOT NULL,             -- 'club' | 'dj'
    payee_id              UUID NOT NULL,
    period_start          TIMESTAMPTZ NOT NULL,
    period_end            TIMESTAMPTZ NOT NULL,
    amount_minor          BIGINT NOT NULL,           -- НЕТТО (pending − reversed)
    min_threshold_minor   BIGINT NOT NULL DEFAULT 1000, -- порог 10 € (Q4, конфиг)
    status                TEXT NOT NULL DEFAULT 'scheduled', -- scheduled/in_flight/paid/failed/on_hold
    stripe_transfer_id    TEXT,                      -- UNIQUE: идемпотентность вебхуков
    fail_reason           TEXT,
    attempts              INTEGER NOT NULL DEFAULT 0,
    final                 BOOLEAN NOT NULL DEFAULT FALSE, -- финальная выплата без порога
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    paid_at               TIMESTAMPTZ
);

CREATE INDEX idx_payout_payee ON payout (payee_type, payee_id, status);
CREATE INDEX idx_payout_status ON payout (status) WHERE status IN ('in_flight', 'failed');
CREATE UNIQUE INDEX idx_payout_transfer_id ON payout (stripe_transfer_id) WHERE stripe_transfer_id IS NOT NULL;

-- ============================================================
-- Reconciliation: ежедневная сверка (ledger vs события).
-- ============================================================
CREATE TABLE reconciliation_run (
    id                UUID PRIMARY KEY,
    period_start      TIMESTAMPTZ NOT NULL,
    period_end        TIMESTAMPTZ NOT NULL,
    expected_minor    BIGINT,
    actual_minor      BIGINT,
    discrepancies     JSONB,                         -- список расхождений
    status            TEXT NOT NULL DEFAULT 'running', -- running/ok/discrepancies/failed
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ============================================================
-- Инциденты: расхождения сверок, блокировки выплат, disputes.
-- Разбор — админом (POST /v1/admin/incidents/:id/resolve).
-- ============================================================
CREATE TABLE incident (
    id            UUID PRIMARY KEY,
    kind          TEXT NOT NULL,                     -- 'reconciliation' | 'payout_failed' | 'invariant' | 'dispute'
    message       TEXT NOT NULL,
    ref           TEXT,                              -- payout_id / run_id / payment_id
    resolved_by   UUID,                              -- admin_user.id
    resolved_at   TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_incident_open ON incident (created_at DESC) WHERE resolved_at IS NULL;
