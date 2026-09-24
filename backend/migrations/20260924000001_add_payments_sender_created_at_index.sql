-- no-transaction
-- #948: Zero-downtime compound index for a user's sent-payment history
-- (`WHERE sender_id = $1 ORDER BY created_at DESC`). See
-- 20260924000000_add_yield_transactions_user_created_at_index.sql for why
-- this runs outside a transaction and in its own file.
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_payments_sender_id_created_at
    ON payments (sender_id, created_at DESC);
