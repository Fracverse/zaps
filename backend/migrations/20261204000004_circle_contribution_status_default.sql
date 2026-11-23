-- A contribution row only counts once it has been verified on Stellar, so the
-- column default must not be a counted state. Rows that existed before the
-- contribution pipeline were backfilled as 'confirmed' (migration
-- 20261204000001) and stay that way; every new row now starts as 'pending'
-- unless the writer states a status explicitly (the legacy /contribute
-- endpoint does).

ALTER TABLE circle_contributions
    ALTER COLUMN status SET DEFAULT 'pending';
