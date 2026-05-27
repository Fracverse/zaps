-- Payment Batch Processing Service
-- Creates tables for batch processing, tracking, and status management.

-- -------------------------------------------------------------------------
-- payment_batches
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS payment_batches (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    batch_key       VARCHAR(255) UNIQUE NOT NULL,
    merchant_id     VARCHAR(255) NOT NULL,
    status          VARCHAR(50) NOT NULL DEFAULT 'pending',
    total_amount    BIGINT NOT NULL,
    total_count     INTEGER NOT NULL,
    processed_count INTEGER NOT NULL DEFAULT 0,
    failed_count    INTEGER NOT NULL DEFAULT 0,
    asset           VARCHAR(56) NOT NULL,
    created_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    completed_at    TIMESTAMP WITH TIME ZONE,

    CONSTRAINT fk_batch_merchant
        FOREIGN KEY (merchant_id) REFERENCES merchants(merchant_id) ON DELETE RESTRICT,

    CONSTRAINT chk_batch_status CHECK (
        status IN ('pending', 'processing', 'completed', 'failed', 'partial_failure')
    ),

    CONSTRAINT chk_batch_counts CHECK (
        processed_count + failed_count <= total_count
    )
);

-- -------------------------------------------------------------------------
-- batch_items
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS batch_items (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    batch_id        UUID NOT NULL,
    payment_id      UUID NOT NULL,
    status          VARCHAR(50) NOT NULL DEFAULT 'pending',
    error_message   TEXT,
    retry_count     INTEGER NOT NULL DEFAULT 0,
    created_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW(),

    CONSTRAINT fk_batch_item_batch
        FOREIGN KEY (batch_id) REFERENCES payment_batches(id) ON DELETE CASCADE,

    CONSTRAINT fk_batch_item_payment
        FOREIGN KEY (payment_id) REFERENCES payments(id) ON DELETE RESTRICT,

    CONSTRAINT chk_batch_item_status CHECK (
        status IN ('pending', 'processing', 'completed', 'failed')
    ),

    CONSTRAINT chk_retry_count CHECK (retry_count >= 0)
);

-- -------------------------------------------------------------------------
-- batch_status_history
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS batch_status_history (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    batch_id        UUID NOT NULL,
    old_status      VARCHAR(50),
    new_status      VARCHAR(50) NOT NULL,
    reason          TEXT,
    created_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW(),

    CONSTRAINT fk_status_history_batch
        FOREIGN KEY (batch_id) REFERENCES payment_batches(id) ON DELETE CASCADE
);

-- -------------------------------------------------------------------------
-- Indexes
-- -------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_batches_merchant_id
    ON payment_batches(merchant_id);

CREATE INDEX IF NOT EXISTS idx_batches_status
    ON payment_batches(status);

CREATE INDEX IF NOT EXISTS idx_batches_created_at
    ON payment_batches(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_batch_items_batch_id
    ON batch_items(batch_id);

CREATE INDEX IF NOT EXISTS idx_batch_items_payment_id
    ON batch_items(payment_id);

CREATE INDEX IF NOT EXISTS idx_batch_items_status
    ON batch_items(status);

CREATE INDEX IF NOT EXISTS idx_status_history_batch_id
    ON batch_status_history(batch_id);

-- -------------------------------------------------------------------------
-- Trigger: auto-update updated_at on payment_batches
-- -------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION update_batch_updated_at()
RETURNS TRIGGER AS $
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_batch_updated_at ON payment_batches;
CREATE TRIGGER trg_batch_updated_at
    BEFORE UPDATE ON payment_batches
    FOR EACH ROW
    EXECUTE FUNCTION update_batch_updated_at();

-- -------------------------------------------------------------------------
-- Trigger: auto-update updated_at on batch_items
-- -------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION update_batch_item_updated_at()
RETURNS TRIGGER AS $
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_batch_item_updated_at ON batch_items;
CREATE TRIGGER trg_batch_item_updated_at
    BEFORE UPDATE ON batch_items
    FOR EACH ROW
    EXECUTE FUNCTION update_batch_item_updated_at();
