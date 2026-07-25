-- ZAPS Social Payment Database Schema
-- SQL database migrations for PostgreSQL

CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    address VARCHAR(56) UNIQUE NOT NULL, -- Stellar public G-address
    username VARCHAR(30) UNIQUE NOT NULL, -- Zaps ID (e.g. ebube)
    display_name VARCHAR(100),
    bio VARCHAR(255),
    avatar_url TEXT,
    auto_earn_enabled BOOLEAN NOT NULL DEFAULT false,
    last_daily_yield_report_at TIMESTAMP,
    last_weekly_yield_report_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS payments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tx_hash VARCHAR(64) UNIQUE NOT NULL, -- Stellar transaction hash
    sender_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    receiver_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    amount BIGINT NOT NULL, -- In micro-units (e.g., 500000 = N5,000.00 if scale is 2)
    currency VARCHAR(10) NOT NULL DEFAULT 'NGN',
    memo TEXT NOT NULL,
    visibility VARCHAR(10) NOT NULL DEFAULT 'PUBLIC', -- PUBLIC, FRIENDS, PRIVATE
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS likes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    payment_id UUID NOT NULL REFERENCES payments(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE (payment_id, user_id)
);

CREATE TABLE IF NOT EXISTS comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    payment_id UUID NOT NULL REFERENCES payments(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS friendships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    friend_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING', -- PENDING, ACCEPTED
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, friend_id)
);

CREATE TABLE IF NOT EXISTS bridge_transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_tx_hash VARCHAR(128) UNIQUE NOT NULL, -- Source chain deposit tx hash / id
    source_chain VARCHAR(20) NOT NULL DEFAULT 'STLR', -- Allbridge chain symbol (e.g. STLR, ETH, BSC)
    destination_chain VARCHAR(20),
    destination_address VARCHAR(128),
    amount VARCHAR(78), -- Raw amount as string (supports big integers / decimals)
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING', -- PENDING, SUCCESS, FAILED
    confirmations INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_payments_visibility ON payments(visibility);
CREATE INDEX IF NOT EXISTS idx_payments_created_at ON payments(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_payments_sender_id ON payments(sender_id);
CREATE INDEX IF NOT EXISTS idx_payments_receiver_id ON payments(receiver_id);
CREATE INDEX IF NOT EXISTS idx_users_display_name ON users(display_name);
CREATE INDEX IF NOT EXISTS idx_bridge_tx_status ON bridge_transactions(status);
CREATE INDEX IF NOT EXISTS idx_bridge_tx_created_at ON bridge_transactions(created_at DESC);

-- Yield Tracking Tables
CREATE TABLE IF NOT EXISTS user_yield_balances (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    available_balance BIGINT NOT NULL DEFAULT 0 CHECK (available_balance >= 0),
    earning_balance BIGINT NOT NULL DEFAULT 0 CHECK (earning_balance >= 0),
    last_yield_sync_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS user_push_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expo_push_token TEXT NOT NULL,
    platform VARCHAR(20),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, expo_push_token)
);

CREATE TABLE IF NOT EXISTS yield_transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tx_hash VARCHAR(64) UNIQUE NOT NULL,
    type VARCHAR(20) NOT NULL, -- DEPOSIT, WITHDRAW, EARNED
    amount BIGINT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS yield_rates_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    apy INTEGER NOT NULL, -- APY in basis points (e.g., 500 = 5.00%)
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_yield_tx_user_id ON yield_transactions(user_id);
CREATE INDEX IF NOT EXISTS idx_yield_tx_created_at ON yield_transactions(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_yield_rates_created_at ON yield_rates_history(created_at DESC);
