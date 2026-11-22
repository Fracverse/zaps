-- Rollback for 20260924000002_add_payments_receiver_created_at_index.sql
-- Run outside a transaction (psql -f does not wrap by default).
DROP INDEX CONCURRENTLY IF EXISTS idx_payments_receiver_id_created_at;
