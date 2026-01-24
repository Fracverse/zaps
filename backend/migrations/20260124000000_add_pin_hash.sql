-- Add pin_hash column for bcrypt-hashed PIN authentication
ALTER TABLE users ADD COLUMN IF NOT EXISTS pin_hash VARCHAR(72);

-- Add index on user_id for faster auth lookups
CREATE INDEX IF NOT EXISTS idx_users_user_id ON users(user_id);
