-- Rollback for 20260729000001_username_indices.sql (#541).
--
-- Non-destructive: these are indices only, no user data is touched. Dropping
-- idx_users_username_lower gives up case-insensitive registration uniqueness,
-- so `Ebube` and `ebube` become registerable as separate accounts again.

DROP INDEX IF EXISTS idx_users_username_lower_pattern;
DROP INDEX IF EXISTS idx_users_username_lower;
