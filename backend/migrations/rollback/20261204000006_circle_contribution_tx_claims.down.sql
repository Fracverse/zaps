-- The previous index also covered unconfirmed claims, so duplicate claims have
-- to be closed before it can be recreated.
UPDATE circle_contributions c
   SET status = 'failed', failure_reason = 'transaction_used_by_another_contribution'
 WHERE c.status = 'submitted' AND c.reference IS NOT NULL
   AND EXISTS (
       SELECT 1 FROM circle_contributions o
        WHERE o.id <> c.id AND o.tx_hash = c.tx_hash AND o.reference IS NOT NULL
          AND o.status IN ('submitted', 'confirmed')
   );

DROP INDEX IF EXISTS idx_circle_contributions_tx_hash;
DROP INDEX IF EXISTS idx_circle_contributions_confirmed_tx;

CREATE UNIQUE INDEX IF NOT EXISTS idx_circle_contributions_live_tx
    ON circle_contributions (tx_hash)
    WHERE reference IS NOT NULL AND tx_hash IS NOT NULL
      AND status IN ('submitted', 'confirmed');
