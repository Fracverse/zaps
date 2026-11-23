-- #1205: contribution initiation.
--
-- Contributions get a lifecycle and pin what the on-chain check has to match
-- (amount, asset, destination, rule version, reference). Rows written before
-- this migration were recorded as already-credited, so they default to
-- `confirmed`. New rows start `pending` and are only credited once verified
-- on Stellar (#1207).

ALTER TABLE circle_contributions
    ALTER COLUMN tx_hash DROP NOT NULL,
    ADD COLUMN IF NOT EXISTS status         TEXT        NOT NULL DEFAULT 'confirmed',
    ADD COLUMN IF NOT EXISTS reference      TEXT,
    ADD COLUMN IF NOT EXISTS asset          TEXT,
    ADD COLUMN IF NOT EXISTS destination    TEXT,
    ADD COLUMN IF NOT EXISTS rule_id        UUID,
    ADD COLUMN IF NOT EXISTS rule_version   INT,
    ADD COLUMN IF NOT EXISTS is_late        BOOLEAN     NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS late_fee       BIGINT      NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS due_at         TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS expires_at     TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS verified_at    TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS failure_reason TEXT,
    ADD COLUMN IF NOT EXISTS updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW();

ALTER TABLE circle_contributions
    DROP CONSTRAINT IF EXISTS circle_contributions_status_check;
ALTER TABLE circle_contributions
    ADD CONSTRAINT circle_contributions_status_check
    CHECK (status IN ('pending', 'submitted', 'confirmed', 'failed', 'expired'));

-- The reference is the Stellar text memo the payer attaches.
CREATE UNIQUE INDEX IF NOT EXISTS idx_circle_contributions_reference
    ON circle_contributions (reference)
    WHERE reference IS NOT NULL;

-- A member cannot hold two identical open initiations in the same circle.
CREATE UNIQUE INDEX IF NOT EXISTS idx_circle_contributions_open_duplicate
    ON circle_contributions (circle_id, member_address, asset, amount)
    WHERE status = 'pending';
