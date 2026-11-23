-- A transaction hash is only unique among confirmed contributions.
--
-- A hash stored while its transaction is still unconfirmed is a claim, not
-- proof. If such claims held the unique index, anyone could submit another
-- member's transaction hash first and stop the real payer from verifying. Now
-- only a confirmed contribution owns a hash, and confirming one closes the
-- other unconfirmed claims on it.

DROP INDEX IF EXISTS idx_circle_contributions_live_tx;

CREATE UNIQUE INDEX IF NOT EXISTS idx_circle_contributions_confirmed_tx
    ON circle_contributions (tx_hash)
    WHERE reference IS NOT NULL AND tx_hash IS NOT NULL
      AND status = 'confirmed';

CREATE INDEX IF NOT EXISTS idx_circle_contributions_tx_hash
    ON circle_contributions (lower(tx_hash))
    WHERE tx_hash IS NOT NULL;
