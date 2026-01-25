-- Migration: add_pin_hash
-- Created: 2026-01-25 12:29:06 UTC

-- Add pin_hash column for bcrypt-hashed PIN authentication
ALTER TABLE users ADD COLUMN IF NOT EXISTS pin_hash VARCHAR(72) NOT NULL;

-- Add index on user_id for faster auth lookups
CREATE INDEX IF NOT EXISTS idx_users_user_id ON users(user_id);


