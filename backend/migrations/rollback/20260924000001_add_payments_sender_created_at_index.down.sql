-- Rollback for 20260924000001_add_payments_sender_created_at_index.sql
-- Run outside a transaction (psql -f does not wrap by default).
DROP INDEX CONCURRENTLY IF EXISTS idx_payments_sender_id_created_at;
