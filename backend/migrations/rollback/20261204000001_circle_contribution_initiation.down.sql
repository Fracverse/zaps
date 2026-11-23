-- Contributions that never received a transaction hash cannot satisfy the
-- restored NOT NULL constraint, so they are removed first.

DROP INDEX IF EXISTS idx_circle_contributions_open_duplicate;
DROP INDEX IF EXISTS idx_circle_contributions_reference;

DELETE FROM circle_contributions WHERE tx_hash IS NULL;

ALTER TABLE circle_contributions
    DROP CONSTRAINT IF EXISTS circle_contributions_status_check,
    DROP COLUMN IF EXISTS updated_at,
    DROP COLUMN IF EXISTS failure_reason,
    DROP COLUMN IF EXISTS verified_at,
    DROP COLUMN IF EXISTS expires_at,
    DROP COLUMN IF EXISTS due_at,
    DROP COLUMN IF EXISTS late_fee,
    DROP COLUMN IF EXISTS is_late,
    DROP COLUMN IF EXISTS rule_version,
    DROP COLUMN IF EXISTS rule_id,
    DROP COLUMN IF EXISTS destination,
    DROP COLUMN IF EXISTS asset,
    DROP COLUMN IF EXISTS reference,
    DROP COLUMN IF EXISTS status,
    ALTER COLUMN tx_hash SET NOT NULL;
