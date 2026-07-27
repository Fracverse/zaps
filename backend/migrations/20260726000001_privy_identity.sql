-- Add Privy DID identity linking support
ALTER TABLE users
ADD COLUMN privy_did VARCHAR(255) UNIQUE,
ADD COLUMN privy_linked_at TIMESTAMP;

-- Index for quick DID lookups
CREATE INDEX IF NOT EXISTS idx_users_privy_did
    ON users(privy_did)
    WHERE privy_did IS NOT NULL;
