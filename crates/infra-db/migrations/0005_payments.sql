-- DROP IT — миграция 0005: платежи, сплиты, ledger (F5 Test Mode).
-- Модель — Часть II техкарты; стейт-машина payment — Часть III.
--
-- ПРАВИЛА:
--   * деньги — BIGINT минорные единицы (ADR-003), float запрещён;
--   * payment.split_snapshot — неизменяем ПОСЛЕ captured: аудит
--     «почему разделили именно так»;
--   * ledger_entry — append-only, двойная запись; баланс агрегатом,
--     ошибки — сторнирующей проводкой, не UPDATE; счёт = (account,
--     currency) — мультивалюта без миграции;
--   * webhook_event (event_id UNIQUE) — приём вебхуков; ИДЕМПОТЕНТНОСТЬ
--     ОБРАБОТКИ — по (aggregate_id, event_type), НЕ по event_id
--     (payment_intent.succeeded и charge.succeeded — разные event_id
--     одного платежа);
--   * Stripe в F5 — Test Mode через мок-клиент (правило 5: помечено
--     явно); прод-включение — только после Q11–Q15 и ADR-004.

-- ============================================================
-- Платежи. Стейт-машина:
--   intent → processing → captured → partially_refunded → refunded
--   intent → failed
--   intent → cancelling → cancelled
--   intent → cancelling → captured (гонка: захват раньше отмены)
--   intent → cancelled
--   captured → captured: NO-OP (дубль вебхука)
-- ============================================================
CREATE TABLE payment (
    id                        UUID PRIMARY KEY,        -- UUIDv7
    request_id                UUID NOT NULL REFERENCES request (id),
    stripe_payment_intent_id  TEXT,                    -- intent id у провайдера
    idempotency_key           TEXT NOT NULL,           -- UNIQUE: replay дублей
    amount_minor              BIGINT NOT NULL,         -- цена запроса (снимок)
    currency                  TEXT NOT NULL DEFAULT 'EUR',
    status                    TEXT NOT NULL DEFAULT 'intent',
    refunded_minor            BIGINT NOT NULL DEFAULT 0,
    failure_reason            TEXT,
    stripe_error_code         TEXT,
    split_snapshot            JSONB,                   -- ПОЛНЫЙ снимок правил разреза
    created_at                TIMESTAMPTZ NOT NULL DEFAULT now(),
    captured_at               TIMESTAMPTZ,
    UNIQUE (idempotency_key)
);

CREATE INDEX idx_payment_request ON payment (request_id);
CREATE INDEX idx_payment_status ON payment (status) WHERE status IN ('intent', 'processing', 'cancelling');

-- ============================================================
-- Сплиты: строки snapshot'а, материализованные для выплат (F6).
-- Инвариант: Σ split == payment.amount_minor.
-- ============================================================
CREATE TABLE split (
    id              UUID PRIMARY KEY,
    payment_id      UUID NOT NULL REFERENCES payment (id),
    payee_type      TEXT NOT NULL,                     -- 'platform' | 'club' | 'dj'
    payee_id        UUID,                              -- NULL для platform
    amount_minor    BIGINT NOT NULL,
    reversed_minor  BIGINT NOT NULL DEFAULT 0,
    status          TEXT NOT NULL DEFAULT 'pending'    -- pending/paid_out/partially_reversed/reversed
);

CREATE INDEX idx_split_payee ON split (payee_type, payee_id, status);
CREATE INDEX idx_split_payment ON split (payment_id);

-- ============================================================
-- Ledger: двойная запись, append-only. Микро-инвариант на каждый
-- txn_id: Σ ledger_entry(txn) == 0.
-- Счета: revenue:platform / payable:club / payable:dj /
--         expense:stripe_fees / tax:collected
-- ============================================================
CREATE TABLE ledger_entry (
    id           UUID PRIMARY KEY,
    txn_id       UUID NOT NULL,                        -- одна проводка = 2+ строки
    account      TEXT NOT NULL,                        -- 'revenue:platform' и т.д.
    currency     TEXT NOT NULL,
    amount_minor BIGINT NOT NULL,                      -- +/- (двойная запись)
    entry_type   TEXT NOT NULL,                        -- 'capture' | 'fee' | 'reversal' | ...
    ref          TEXT,                                 -- payment_id / transfer_id
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_ledger_txn ON ledger_entry (txn_id);
CREATE INDEX idx_ledger_account ON ledger_entry (account, currency);

-- ============================================================
-- Webhook-события: приём (UNIQUE event_id — дедуп доставки),
-- обработка идемпотентна по (aggregate_id, event_type).
-- ============================================================
CREATE TABLE webhook_event (
    id           UUID PRIMARY KEY,
    source       TEXT NOT NULL,                        -- 'stripe'
    event_id     TEXT NOT NULL,                        -- delivery-level дедуп
    event_type   TEXT NOT NULL,                        -- 'payment_intent.succeeded' | ...
    aggregate_id UUID NOT NULL,                        -- payment id
    payload      JSONB NOT NULL,
    processed_at TIMESTAMPTZ,
    received_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (event_id)
);

CREATE UNIQUE INDEX idx_webhook_processed ON webhook_event (aggregate_id, event_type) WHERE processed_at IS NOT NULL;
