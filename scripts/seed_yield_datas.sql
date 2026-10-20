-- =============================================================================
-- Seed: Yield Data for Developer Testing
-- Populates a test user with realistic yield balances and transaction history.
-- Run AFTER all migrations have been applied.
-- =============================================================================

BEGIN;

-- -----------------------------------------------------------------------------
-- 1. Seed user (idempotent via ON CONFLICT DO NOTHING)
-- -----------------------------------------------------------------------------
INSERT INTO users (
    id,
    address,
    username,
    display_name,
    bio,
    avatar_url,
    auto_earn_enabled,
    created_at
)
VALUES (
    '00000000-0000-0000-0000-000000000001',
    'GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN',  -- sample Stellar G-address
    'devuser',
    'Dev User',
    'Seed account for yield testing.',
    NULL,
    true,
    NOW() - INTERVAL '60 days'
)
ON CONFLICT (id) DO NOTHING;

-- -----------------------------------------------------------------------------
-- 2. Seed yield balance (available + earning)
--    Amounts are in micro-units (scale 2), so:
--      5_000_000 = N50,000.00
--      2_000_000 = N20,000.00
-- -----------------------------------------------------------------------------
INSERT INTO user_yield_balances (
    user_id,
    available_balance,
    earning_balance,
    last_yield_sync_at,
    updated_at
)
VALUES (
    '00000000-0000-0000-0000-000000000001',
    5000000,   -- N50,000.00 available
    2000000,   -- N20,000.00 actively earning
    NOW(),
    NOW()
)
ON CONFLICT (user_id) DO UPDATE
    SET available_balance  = EXCLUDED.available_balance,
        earning_balance    = EXCLUDED.earning_balance,
        last_yield_sync_at = EXCLUDED.last_yield_sync_at,
        updated_at         = EXCLUDED.updated_at;

-- -----------------------------------------------------------------------------
-- 3. Seed yield transactions: realistic history over the past 30 days
-- -----------------------------------------------------------------------------

-- Initial deposit (30 days ago)
INSERT INTO yield_transactions (id, user_id, tx_hash, type, amount, created_at)
VALUES (
    '00000000-0000-0000-0001-000000000001',
    '00000000-0000-0000-0000-000000000001',
    'seed_tx_hash_deposit_001',
    'DEPOSIT',
    3000000,   -- N30,000.00
    NOW() - INTERVAL '30 days'
)
ON CONFLICT (tx_hash) DO NOTHING;

-- Top-up deposit (20 days ago)
INSERT INTO yield_transactions (id, user_id, tx_hash, type, amount, created_at)
VALUES (
    '00000000-0000-0000-0001-000000000002',
    '00000000-0000-0000-0000-000000000001',
    'seed_tx_hash_deposit_002',
    'DEPOSIT',
    2000000,   -- N20,000.00
    NOW() - INTERVAL '20 days'
)
ON CONFLICT (tx_hash) DO NOTHING;

-- Yield earned (15 days ago)
INSERT INTO yield_transactions (id, user_id, tx_hash, type, amount, created_at)
VALUES (
    '00000000-0000-0000-0001-000000000003',
    '00000000-0000-0000-0000-000000000001',
    'seed_tx_hash_earned_001',
    'EARNED',
    125000,    -- N1,250.00 accrued yield
    NOW() - INTERVAL '15 days'
)
ON CONFLICT (tx_hash) DO NOTHING;

-- Partial withdrawal (10 days ago)
INSERT INTO yield_transactions (id, user_id, tx_hash, type, amount, created_at)
VALUES (
    '00000000-0000-0000-0001-000000000004',
    '00000000-0000-0000-0000-000000000001',
    'seed_tx_hash_withdraw_001',
    'WITHDRAW',
    500000,    -- N5,000.00 withdrawn
    NOW() - INTERVAL '10 days'
)
ON CONFLICT (tx_hash) DO NOTHING;

-- Yield earned (5 days ago)
INSERT INTO yield_transactions (id, user_id, tx_hash, type, amount, created_at)
VALUES (
    '00000000-0000-0000-0001-000000000005',
    '00000000-0000-0000-0000-000000000001',
    'seed_tx_hash_earned_002',
    'EARNED',
    87500,     -- N875.00 accrued yield
    NOW() - INTERVAL '5 days'
)
ON CONFLICT (tx_hash) DO NOTHING;

-- Most recent deposit (1 day ago)
INSERT INTO yield_transactions (id, user_id, tx_hash, type, amount, created_at)
VALUES (
    '00000000-0000-0000-0001-000000000006',
    '00000000-0000-0000-0000-000000000001',
    'seed_tx_hash_deposit_003',
    'DEPOSIT',
    2500000,   -- N25,000.00
    NOW() - INTERVAL '1 day'
)
ON CONFLICT (tx_hash) DO NOTHING;

-- -----------------------------------------------------------------------------
-- 4. Seed yield rate history (APY in basis points; 500 = 5.00%)
-- -----------------------------------------------------------------------------
INSERT INTO yield_rates_history (id, apy, created_at)
VALUES
    ('00000000-0000-0000-0002-000000000001', 480, NOW() - INTERVAL '30 days'),
    ('00000000-0000-0000-0002-000000000002', 490, NOW() - INTERVAL '20 days'),
    ('00000000-0000-0000-0002-000000000003', 500, NOW() - INTERVAL '10 days'),
    ('00000000-0000-0000-0002-000000000004', 510, NOW() - INTERVAL '1 day')
ON CONFLICT (id) DO NOTHING;

COMMIT;
