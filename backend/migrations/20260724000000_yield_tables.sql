-- #546: historical log of on-chain yield-vault accrual events.
--
-- Deposits are already tracked in yield_transactions, and administrator/
-- rate-setter APY changes are already tracked in yield_rates_history (both
-- from 20260625000000_create_yield_tables.sql). Neither captures the vault's
-- own periodic compounding: the indexer's YieldAccrued handling
-- (indexer::worker::process_event_batch_with_guard) currently only logs
-- these via tracing and marks the platform yield cache dirty — the event
-- data itself is never persisted, so there is no historical record of how
-- much yield accrued, over how many ledgers, or what the new index was.
--
-- This table is platform-wide (not per-user), matching the vault-level
-- nature of the on-chain event; individual user balances continue to be
-- read from user_yield_balances / yield_transactions as before.
CREATE TABLE IF NOT EXISTS yield_accrual_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tx_hash VARCHAR(64) UNIQUE NOT NULL,
    added_yield BIGINT NOT NULL,
    elapsed_ledgers BIGINT NOT NULL,
    new_index BIGINT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_yield_accrual_log_created_at ON yield_accrual_log(created_at DESC);
