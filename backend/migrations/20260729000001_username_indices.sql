-- #541: index username mappings and enforce DB-level registration uniqueness.

-- users.username already carries a plain UNIQUE constraint, but that constraint
-- is case-sensitive: `Ebube` and `ebube` can both register and then resolve to
-- different accounts. This makes the registry case-insensitively unique at the
-- database level, so a racing pair of registrations cannot both commit.
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_username_lower
    ON users (LOWER(username));

-- Prefix lookup for the search/suggestion endpoints (`LIKE 'ebu%'`). A btree in
-- the default collation cannot serve LIKE, so the prefix path gets its own
-- text_pattern_ops index over the same lowercased expression.
CREATE INDEX IF NOT EXISTS idx_users_username_lower_pattern
    ON users (LOWER(username) text_pattern_ops);
