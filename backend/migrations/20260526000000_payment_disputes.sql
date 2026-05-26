-- Payment Dispute Management
-- Creates tables for payment disputes and associated evidence.

-- -------------------------------------------------------------------------
-- payment_disputes
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS payment_disputes (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    payment_id      UUID NOT NULL,
    filed_by_user_id VARCHAR(255) NOT NULL,
    reason          VARCHAR(50) NOT NULL,
    description     TEXT NOT NULL,
    status          VARCHAR(50) NOT NULL DEFAULT 'open',
    disputed_amount BIGINT NOT NULL,
    resolution_notes TEXT,
    resolved_by     VARCHAR(255),
    created_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at      TIMESTAMP WITH TIME ZONE DEFAULT NOW(),

    CONSTRAINT fk_dispute_payment
        FOREIGN KEY (payment_id) REFERENCES payments(id) ON DELETE RESTRICT,

    CONSTRAINT fk_dispute_filed_by
        FOREIGN KEY (filed_by_user_id) REFERENCES users(user_id) ON DELETE RESTRICT,

    CONSTRAINT chk_dispute_status CHECK (
        status IN ('open', 'under_review', 'resolved_customer', 'resolved_merchant', 'closed')
    ),

    CONSTRAINT chk_dispute_reason CHECK (
        reason IN ('unauthorized', 'not_delivered', 'not_as_described', 'incorrect_amount', 'duplicate', 'other')
    ),

    CONSTRAINT chk_disputed_amount CHECK (disputed_amount > 0)
);

-- -------------------------------------------------------------------------
-- dispute_evidence
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS dispute_evidence (
    id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    dispute_id            UUID NOT NULL,
    submitted_by_user_id  VARCHAR(255) NOT NULL,
    evidence_type         VARCHAR(100) NOT NULL,
    description           TEXT NOT NULL,
    file_url              TEXT,
    created_at            TIMESTAMP WITH TIME ZONE DEFAULT NOW(),

    CONSTRAINT fk_evidence_dispute
        FOREIGN KEY (dispute_id) REFERENCES payment_disputes(id) ON DELETE CASCADE,

    CONSTRAINT fk_evidence_submitted_by
        FOREIGN KEY (submitted_by_user_id) REFERENCES users(user_id) ON DELETE RESTRICT
);

-- -------------------------------------------------------------------------
-- Indexes
-- -------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_disputes_payment_id
    ON payment_disputes(payment_id);

CREATE INDEX IF NOT EXISTS idx_disputes_filed_by
    ON payment_disputes(filed_by_user_id);

CREATE INDEX IF NOT EXISTS idx_disputes_status
    ON payment_disputes(status);

CREATE INDEX IF NOT EXISTS idx_disputes_created_at
    ON payment_disputes(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_evidence_dispute_id
    ON dispute_evidence(dispute_id);

-- -------------------------------------------------------------------------
-- Trigger: auto-update updated_at on payment_disputes
-- -------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION update_dispute_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_dispute_updated_at ON payment_disputes;
CREATE TRIGGER trg_dispute_updated_at
    BEFORE UPDATE ON payment_disputes
    FOR EACH ROW
    EXECUTE FUNCTION update_dispute_updated_at();
