-- #1226: Reconciliation results comparing local circle records with Stellar.
CREATE TABLE IF NOT EXISTS circle_reconciliation_records (
    id               UUID PRIMARY KEY,
    record_type      TEXT NOT NULL,        -- contribution / payout
    local_record_id  UUID NOT NULL,
    circle_id        UUID NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    tx_hash          TEXT,
    local_state      TEXT NOT NULL,        -- local processing state
    on_chain_state   TEXT NOT NULL,        -- success / failed / not_found / unknown
    discrepancy      TEXT NOT NULL,        -- none / missing / delayed / duplicate / inconsistent
    details          TEXT,
    checked_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (record_type, local_record_id)
);

CREATE INDEX IF NOT EXISTS circle_reconciliation_discrepancy_idx
    ON circle_reconciliation_records (discrepancy, checked_at DESC);
CREATE INDEX IF NOT EXISTS circle_reconciliation_tx_idx
    ON circle_reconciliation_records (tx_hash);
