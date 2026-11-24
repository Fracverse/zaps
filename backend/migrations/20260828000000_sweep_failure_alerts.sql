-- #737: Track whether the operations channel has been alerted about a
-- user's repeated sweep failures so the alert scheduler can fire once per
-- failing episode instead of re-notifying every poll cycle.
ALTER TABLE sweep_failure_history
    ADD COLUMN IF NOT EXISTS last_alerted_at TIMESTAMP;
