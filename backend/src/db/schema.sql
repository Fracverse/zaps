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

-- Bulk Disbursement Tables (BE-555)
--
-- Three tables because they answer three different questions and have three
-- different write patterns: payout_batches is the unit a caller creates and
-- queries, batch_recipients is the unit the worker claims and submits, and
-- dispatch_logs is an append-only audit of every attempt. Attempts are kept out
-- of batch_recipients on purpose — that row is updated in place, so recording
-- history there would overwrite it.

CREATE TABLE IF NOT EXISTS payout_batches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    idempotency_key VARCHAR(128) UNIQUE NOT NULL, -- retried create returns the existing batch
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING'
        CHECK (status IN ('PENDING', 'PROCESSING', 'COMPLETED', 'PARTIALLY_FAILED', 'FAILED', 'CANCELLED')),
    currency VARCHAR(10) NOT NULL DEFAULT 'NGN',
    total_recipients INTEGER NOT NULL DEFAULT 0 CHECK (total_recipients >= 0),
    total_amount BIGINT NOT NULL DEFAULT 0 CHECK (total_amount >= 0),
    succeeded_count INTEGER NOT NULL DEFAULT 0 CHECK (succeeded_count >= 0),
    failed_count INTEGER NOT NULL DEFAULT 0 CHECK (failed_count >= 0),
    started_at TIMESTAMP,
    completed_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    CONSTRAINT payout_batches_counts_within_total
        CHECK (succeeded_count + failed_count <= total_recipients)
);

CREATE TABLE IF NOT EXISTS batch_recipients (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    batch_id UUID NOT NULL REFERENCES payout_batches(id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL, -- NULL for a raw address payout
    destination_address VARCHAR(56),
    amount BIGINT NOT NULL CHECK (amount > 0),
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING'
        CHECK (status IN ('PENDING', 'SUBMITTED', 'CONFIRMED', 'FAILED')),
    sdp_payment_id VARCHAR(128),
    tx_hash VARCHAR(64),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    last_error TEXT,
    locked_at TIMESTAMP, -- worker lease; lets a peer reclaim rows from a dead process
    locked_by VARCHAR(64),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    CONSTRAINT batch_recipients_has_destination
        CHECK (user_id IS NOT NULL OR destination_address IS NOT NULL)
);

CREATE TABLE IF NOT EXISTS dispatch_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    batch_id UUID NOT NULL REFERENCES payout_batches(id) ON DELETE CASCADE,
    recipient_id UUID REFERENCES batch_recipients(id) ON DELETE CASCADE, -- NULL for batch-level events
    attempt INTEGER NOT NULL DEFAULT 1 CHECK (attempt >= 1),
    event VARCHAR(30) NOT NULL
        CHECK (event IN ('CLAIMED', 'SUBMITTED', 'CONFIRMED', 'FAILED', 'RETRY_SCHEDULED', 'CANCELLED')),
    sdp_response_code VARCHAR(20),
    detail TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Indexes
-- Duplicate-payout guards: a retried batch submission is rejected by the
-- database rather than silently paying a recipient twice.
CREATE UNIQUE INDEX IF NOT EXISTS idx_batch_recipients_unique_user
    ON batch_recipients(batch_id, user_id) WHERE user_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_batch_recipients_unique_address
    ON batch_recipients(batch_id, destination_address) WHERE destination_address IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_payout_batches_status_created ON payout_batches(status, created_at);
CREATE INDEX IF NOT EXISTS idx_payout_batches_created_by ON payout_batches(created_by);
-- Partial, so the worker's hot path index shrinks as rows reach terminal status.
CREATE INDEX IF NOT EXISTS idx_batch_recipients_pending
    ON batch_recipients(batch_id, created_at) WHERE status = 'PENDING';
CREATE INDEX IF NOT EXISTS idx_batch_recipients_batch_status ON batch_recipients(batch_id, status);
CREATE INDEX IF NOT EXISTS idx_batch_recipients_locked
    ON batch_recipients(locked_at) WHERE locked_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_dispatch_logs_batch ON dispatch_logs(batch_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_dispatch_logs_recipient
    ON dispatch_logs(recipient_id, created_at DESC) WHERE recipient_id IS NOT NULL;
