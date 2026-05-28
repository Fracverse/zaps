-- Payment Reconciliation
-- Creates tables for payment reconciliation and audit logs.

-- -------------------------------------------------------------------------
-- payment_reconciliations
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS payment_reconciliations (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    payment_id          UUID,
    external_id         VARCHAR(255),
    source              VARCHAR(50) NOT NULL,
    amount              BIGINT NOT NULL,
    currency            VARCHAR(10) NOT NULL,
    status              VARCHAR(50) NOT NULL DEFAULT 'pending',
    discrepancy_notes   TEXT,
    resolved_by         UUID,
    resolved_at         TIMESTAMP WITH TIME ZONE,
    created_at          TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at          TIMESTAMP WITH TIME ZONE DEFAULT NOW(),

    CONSTRAINT chk_source CHECK (source IN ('stellar', 'bank', 'manual')),
    CONSTRAINT chk_status CHECK (status IN ('pending', 'matched', 'mismatched', 'manual_review', 'resolved'))
);

-- -------------------------------------------------------------------------
-- reconciliation_audit_logs
-- -------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS reconciliation_audit_logs (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reconciliation_id   UUID NOT NULL REFERENCES payment_reconciliations(id) ON DELETE CASCADE,
    actor_id            UUID NOT NULL,
    action              VARCHAR(100) NOT NULL,
    old_status          VARCHAR(50),
    new_status          VARCHAR(50),
    notes               TEXT,
    created_at          TIMESTAMP WITH TIME ZONE DEFAULT NOW(),

    CONSTRAINT chk_action_status CHECK (old_status IN ('pending', 'matched', 'mismatched', 'manual_review', 'resolved') OR old_status IS NULL),
    CONSTRAINT chk_new_status CHECK (new_status IN ('pending', 'matched', 'mismatched', 'manual_review', 'resolved') OR new_status IS NULL)
);

-- -------------------------------------------------------------------------
-- Indexes
-- -------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_payment_reconciliations_status
    ON payment_reconciliations(status);

CREATE INDEX IF NOT EXISTS idx_payment_reconciliations_payment_id
    ON payment_reconciliations(payment_id);

CREATE INDEX IF NOT EXISTS idx_payment_reconciliations_external_id
    ON payment_reconciliations(external_id);

CREATE INDEX IF NOT EXISTS idx_payment_reconciliations_created_at
    ON payment_reconciliations(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_reconciliation_audit_logs_reconciliation_id
    ON reconciliation_audit_logs(reconciliation_id);

CREATE INDEX IF NOT EXISTS idx_reconciliation_audit_logs_created_at
    ON reconciliation_audit_logs(created_at DESC);
