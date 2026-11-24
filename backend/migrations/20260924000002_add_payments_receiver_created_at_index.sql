-- no-transaction
-- #948: Zero-downtime compound index for a user's received-payment history
-- (`WHERE receiver_id = $1 ORDER BY created_at DESC`). See
-- 20260924000000_add_yield_transactions_user_created_at_index.sql for why
-- this runs outside a transaction and in its own file.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_payments_receiver_id_created_at
    ON payments (receiver_id, created_at DESC);
