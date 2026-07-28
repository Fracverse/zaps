-- DB migration to optimize querying users by registration time
CREATE INDEX IF NOT EXISTS idx_users_created_at ON users(created_at DESC);
