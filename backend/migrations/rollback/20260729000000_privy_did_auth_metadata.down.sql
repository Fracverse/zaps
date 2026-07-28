-- Rollback for 20260729000000_privy_did_auth_metadata.sql (#560).
--
-- Dropping the columns is destructive: every linked Privy identity is lost and
-- users have to re-link. Only the index and constraint are dropped here, which
-- returns the schema to its pre-#560 shape without discarding data.
--
-- To also remove the columns (and the identity data with them), uncomment the
-- ALTER TABLE at the bottom.

DROP INDEX IF EXISTS idx_users_privy_did;

ALTER TABLE users DROP CONSTRAINT IF EXISTS users_privy_did_key;

-- ALTER TABLE users
--     DROP COLUMN IF EXISTS privy_did,
--     DROP COLUMN IF EXISTS privy_linked_at;
