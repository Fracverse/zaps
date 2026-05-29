-- Merchant Payout System
-- Migration: 20260529000000_merchant_payouts.sql

-- -------------------------------------------------------------------------
-- payouts table
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS payouts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id     VARCHAR(255) NOT NULL,
    batch_id        UUID,
    amount          BIGINT NOT NULL,
    currency        VARCHAR(3) NOT NULL,
    destination_address VARCHAR(255) NOT NULL,
    status          VARCHAR(50) NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'scheduled', 'processing', 'completed', 'failed', 'cancelled')),
    tx_hash         VARCHAR(255),
    failure_reason  TEXT,
    retry_count     INTEGER NOT NULL DEFAULT 0,
    scheduled_at    TIMESTAMP WITH TIME ZONE,
    processed_at    TIMESTAMP WITH TIME ZONE,
    created_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW() NOT NULL,
    updated_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW() NOT NULL,

    CONSTRAINT fk_payout_merchant
        FOREIGN KEY (merchant_id) REFERENCES merchants(merchant_id) ON DELETE CASCADE,

    CONSTRAINT chk_payout_amount CHECK (amount > 0),
    CONSTRAINT chk_payout_retry CHECK (retry_count >= 0)
);

-- -------------------------------------------------------------------------
-- payout_batches table
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS payout_batches (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id     VARCHAR(255) NOT NULL,
    total_amount    BIGINT NOT NULL,
    currency        VARCHAR(3) NOT NULL,
    payout_count    INTEGER NOT NULL,
    status          VARCHAR(50) NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'scheduled', 'processing', 'completed', 'failed', 'cancelled')),
    scheduled_at    TIMESTAMP WITH TIME ZONE NOT NULL,
    processed_at    TIMESTAMP WITH TIME ZONE,
    created_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW() NOT NULL,
    updated_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW() NOT NULL,

    CONSTRAINT fk_batch_merchant
        FOREIGN KEY (merchant_id) REFERENCES merchants(merchant_id) ON DELETE CASCADE,

    CONSTRAINT chk_batch_amount CHECK (total_amount > 0),
    CONSTRAINT chk_batch_count CHECK (payout_count > 0)
);

-- -------------------------------------------------------------------------
-- payout_reconciliations table
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS payout_reconciliations (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    payout_id       UUID NOT NULL UNIQUE,
    anchor_tx_id    VARCHAR(255),
    status          VARCHAR(50) NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'reconciled', 'discrepancy', 'failed')),
    discrepancy     TEXT,
    reconciled_at   TIMESTAMP WITH TIME ZONE,
    created_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW() NOT NULL,

    CONSTRAINT fk_recon_payout
        FOREIGN KEY (payout_id) REFERENCES payouts(id) ON DELETE CASCADE
);

-- -------------------------------------------------------------------------
-- Indexes
-- -------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_payouts_merchant_id ON payouts(merchant_id);
CREATE INDEX IF NOT EXISTS idx_payouts_batch_id ON payouts(batch_id);
CREATE INDEX IF NOT EXISTS idx_payouts_status ON payouts(status);
CREATE INDEX IF NOT EXISTS idx_payouts_scheduled_at ON payouts(scheduled_at);
CREATE INDEX IF NOT EXISTS idx_payouts_created_at ON payouts(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_payout_batches_merchant_id ON payout_batches(merchant_id);
CREATE INDEX IF NOT EXISTS idx_payout_batches_status ON payout_batches(status);
CREATE INDEX IF NOT EXISTS idx_payout_batches_scheduled_at ON payout_batches(scheduled_at);

CREATE INDEX IF NOT EXISTS idx_payout_reconciliations_status ON payout_reconciliations(status);
CREATE INDEX IF NOT EXISTS idx_payout_reconciliations_anchor_tx_id ON payout_reconciliations(anchor_tx_id);

-- -------------------------------------------------------------------------
-- Triggers for updated_at
-- -------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION update_payouts_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_payouts_updated_at
    BEFORE UPDATE ON payouts
    FOR EACH ROW
    EXECUTE FUNCTION update_payouts_updated_at();

CREATE OR REPLACE FUNCTION update_payout_batches_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_payout_batches_updated_at
    BEFORE UPDATE ON payout_batches
    FOR EACH ROW
    EXECUTE FUNCTION update_payout_batches_updated_at();
