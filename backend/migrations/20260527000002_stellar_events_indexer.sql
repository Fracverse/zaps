-- Stellar Event Indexing
-- Creates tables for tracking Stellar network events and indexer state.

-- -------------------------------------------------------------------------
-- stellar_events
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS stellar_events (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type      VARCHAR(100) NOT NULL,
    tx_hash         VARCHAR(64) UNIQUE NOT NULL,
    ledger_sequence BIGINT NOT NULL,
    source_account  VARCHAR(56) NOT NULL,
    destination_account VARCHAR(56),
    asset_code      VARCHAR(12),
    amount          BIGINT,
    status          VARCHAR(50) NOT NULL DEFAULT 'pending',
    processed       BOOLEAN NOT NULL DEFAULT false,
    raw_data        TEXT,
    indexed_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    created_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW(),

    CONSTRAINT chk_event_status CHECK (
        status IN ('pending', 'confirmed', 'failed')
    )
);

-- -------------------------------------------------------------------------
-- indexer_state
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS indexer_state (
    key             VARCHAR(255) PRIMARY KEY,
    value           TEXT NOT NULL,
    updated_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- -------------------------------------------------------------------------
-- Indexes
-- -------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_stellar_events_tx_hash
    ON stellar_events(tx_hash);

CREATE INDEX IF NOT EXISTS idx_stellar_events_event_type
    ON stellar_events(event_type);

CREATE INDEX IF NOT EXISTS idx_stellar_events_source_account
    ON stellar_events(source_account);

CREATE INDEX IF NOT EXISTS idx_stellar_events_destination_account
    ON stellar_events(destination_account);

CREATE INDEX IF NOT EXISTS idx_stellar_events_status
    ON stellar_events(status);

CREATE INDEX IF NOT EXISTS idx_stellar_events_processed
    ON stellar_events(processed);

CREATE INDEX IF NOT EXISTS idx_stellar_events_ledger_sequence
    ON stellar_events(ledger_sequence DESC);

CREATE INDEX IF NOT EXISTS idx_stellar_events_created_at
    ON stellar_events(created_at DESC);
