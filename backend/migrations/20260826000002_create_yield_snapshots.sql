-- #724: Build yield accrual audit logging service and snapshot store.
-- Create yield_snapshots table for daily interest accrual reconciliation.

CREATE TABLE IF NOT EXISTS yield_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    earning_balance BIGINT NOT NULL,
    accrued_interest BIGINT NOT NULL,
    apy INTEGER NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Indexing for fast reconciliation lookups
CREATE INDEX IF NOT EXISTS idx_yield_snapshots_user_id ON yield_snapshots(user_id);
CREATE INDEX IF NOT EXISTS idx_yield_snapshots_created_at ON yield_snapshots(created_at DESC);
