-- #560: store the Privy DID and auth metadata on user profile records.
--
-- 20260726000001_privy_identity.sql introduced privy_did / privy_linked_at
-- with a bare `ALTER TABLE ... ADD COLUMN`, which aborts if either column is
-- already present. This migration re-states the same end state idempotently so
-- it applies cleanly on a database where the columns were added out of band,
-- and ships a matching rollback script under migrations/rollback/.

ALTER TABLE users ADD COLUMN IF NOT EXISTS privy_did VARCHAR(255);
ALTER TABLE users ADD COLUMN IF NOT EXISTS privy_linked_at TIMESTAMP;

-- Nullable UNIQUE: a Privy DID maps to at most one account, but an account may
-- exist without a linked DID. `ADD CONSTRAINT` has no IF NOT EXISTS form, so
-- the catalog is checked first.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'users'::regclass
          AND contype = 'u'
          AND conname = 'users_privy_did_key'
    ) THEN
        ALTER TABLE users ADD CONSTRAINT users_privy_did_key UNIQUE (privy_did);
    END IF;
END
$$;

-- Lookup path for `SELECT address FROM users WHERE privy_did = $1` in
-- api::auth::privy_auth. Partial, so the rows with no linked DID stay out of it.
CREATE INDEX IF NOT EXISTS idx_users_privy_did
    ON users (privy_did)
    WHERE privy_did IS NOT NULL;
