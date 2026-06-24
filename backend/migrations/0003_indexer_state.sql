CREATE TABLE IF NOT EXISTS indexer_state (
    key TEXT PRIMARY KEY,
    last_ledger_sequence BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);
