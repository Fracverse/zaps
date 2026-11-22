-- Rollback for 20260924000000_add_yield_transactions_user_created_at_index.sql
-- Run outside a transaction (psql -f does not wrap by default).
DROP INDEX CONCURRENTLY IF EXISTS idx_yield_tx_user_id_created_at;
