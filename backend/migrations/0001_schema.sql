-- ZAPS Social Payment Database Schema
-- SQL database migrations for PostgreSQL

CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    address VARCHAR(56) UNIQUE NOT NULL, -- Stellar public G-address
    username VARCHAR(30) UNIQUE NOT NULL, -- Zaps ID (e.g. ebube)
    display_name VARCHAR(100),
    bio VARCHAR(255),
    avatar_url TEXT,
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

-- Indexes
CREATE INDEX IF NOT EXISTS idx_payments_visibility ON payments(visibility);
CREATE INDEX IF NOT EXISTS idx_payments_created_at ON payments(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_payments_sender_id ON payments(sender_id);
CREATE INDEX IF NOT EXISTS idx_payments_receiver_id ON payments(receiver_id);
CREATE INDEX IF NOT EXISTS idx_users_display_name ON users(display_name);
