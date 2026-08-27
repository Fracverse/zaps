-- #725: Enable pg_trgm extension and add GIN trigram index for
-- fast prefix/similarity search on usernames.

CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- GIN trigram index on lowercased username for ILIKE 'query%' and
-- similarity() queries. This makes autocomplete searches scale to
-- large user tables.
CREATE INDEX IF NOT EXISTS idx_users_username_trgm
    ON users USING GIN (LOWER(username) gin_trgm_ops);
