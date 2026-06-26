-- Create user_yield_balances table
CREATE TABLE IF NOT EXISTS user_yield_balances (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    available_balance BIGINT NOT NULL DEFAULT 0 CHECK (available_balance >= 0),
    earning_balance BIGINT NOT NULL DEFAULT 0 CHECK (earning_balance >= 0),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Create yield_transactions table
CREATE TABLE IF NOT EXISTS yield_transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tx_hash VARCHAR(64) UNIQUE NOT NULL,
    type VARCHAR(20) NOT NULL, -- DEPOSIT, WITHDRAW, EARNED
    amount BIGINT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Create yield_rates_history table
CREATE TABLE IF NOT EXISTS yield_rates_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    apy INTEGER NOT NULL, -- APY in basis points (e.g., 500 = 5.00%)
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_yield_tx_user_id ON yield_transactions(user_id);
CREATE INDEX IF NOT EXISTS idx_yield_tx_created_at ON yield_transactions(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_yield_rates_created_at ON yield_rates_history(created_at DESC);
