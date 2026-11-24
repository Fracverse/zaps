-- #1212: Circle payout execution ledger.
-- One row per approved payout request. The UNIQUE constraint is the hard guard
-- against duplicate transfers when execution is retried or submitted twice.

CREATE TABLE IF NOT EXISTS circle_payout_executions (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    circle_id        UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    request_id       BIGINT      NOT NULL,
    tx_hash          TEXT        NOT NULL,
    expected_to      TEXT        NOT NULL,
    expected_amount  BIGINT      NOT NULL,
    expected_asset   TEXT        NOT NULL,
    status           TEXT        NOT NULL DEFAULT 'submitted'
                                 CHECK (status IN ('submitted', 'verified', 'failed')),
    failure_reason   TEXT,
    executed_by      TEXT        NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    verified_at      TIMESTAMPTZ,
    UNIQUE (circle_id, request_id)
);

CREATE INDEX IF NOT EXISTS idx_circle_payout_executions_tx ON circle_payout_executions (tx_hash);
