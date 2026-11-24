-- #1207: contribution verification.

-- One transaction can back at most one live contribution. Scoped to rows
-- created by the new flow so historical data cannot block the migration.
CREATE UNIQUE INDEX IF NOT EXISTS idx_circle_contributions_live_tx
    ON circle_contributions (tx_hash)
    WHERE reference IS NOT NULL AND tx_hash IS NOT NULL
      AND status IN ('submitted', 'confirmed');

CREATE TABLE IF NOT EXISTS circle_contribution_verifications (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    contribution_id UUID        NOT NULL REFERENCES circle_contributions (id) ON DELETE CASCADE,
    circle_id       UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    tx_hash         TEXT        NOT NULL,
    outcome         TEXT        NOT NULL
                                CHECK (outcome IN ('confirmed', 'failed', 'not_found', 'unavailable')),
    reason          TEXT,
    ledger          BIGINT,
    checked_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_circle_contribution_verifications_contribution
    ON circle_contribution_verifications (contribution_id, checked_at DESC);
