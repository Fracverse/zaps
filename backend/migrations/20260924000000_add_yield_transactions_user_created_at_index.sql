-- no-transaction
-- #948: Zero-downtime compound index for per-user transaction history
-- (`WHERE user_id = $1 ORDER BY created_at DESC`).
--
-- CREATE INDEX CONCURRENTLY cannot run inside a transaction block, so the
-- `-- no-transaction` directive above (must be the first line; sqlx 0.7
-- syntax) tells SQLx not to wrap this file. Postgres also runs a
-- multi-statement query string as one implicit transaction, so each
-- concurrent index lives in its own migration file.
--
-- If a concurrent build is interrupted it leaves an INVALID index that
-- IF NOT EXISTS will skip; drop it with DROP INDEX CONCURRENTLY and re-run.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_yield_tx_user_id_created_at
    ON yield_transactions (user_id, created_at DESC);
