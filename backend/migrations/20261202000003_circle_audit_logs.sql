-- #1222: Append-only, tamper-evident circle audit log.
CREATE TABLE IF NOT EXISTS circle_audit_logs (
    seq           BIGSERIAL   PRIMARY KEY,
    id            UUID        NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    circle_id     UUID        NOT NULL REFERENCES circles (id) ON DELETE RESTRICT,
    actor         TEXT        NOT NULL,
    action        TEXT        NOT NULL,
    target        TEXT        NOT NULL,
    details       JSONB       NOT NULL DEFAULT '{}'::jsonb,
    occurred_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    previous_hash TEXT        NOT NULL,
    entry_hash    TEXT        NOT NULL,
    CHECK (entry_hash <> '' AND previous_hash <> '')
);

CREATE INDEX IF NOT EXISTS idx_circle_audit_logs_circle_seq
    ON circle_audit_logs (circle_id, seq DESC);

-- Ordinary application roles cannot mutate audit rows; only INSERT is allowed.
REVOKE UPDATE, DELETE, TRUNCATE ON circle_audit_logs FROM PUBLIC;

CREATE OR REPLACE FUNCTION circle_audit_logs_immutable() RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION 'circle_audit_logs is append-only';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS circle_audit_logs_no_update ON circle_audit_logs;
CREATE TRIGGER circle_audit_logs_no_update
    BEFORE UPDATE OR DELETE OR TRUNCATE ON circle_audit_logs
    FOR EACH STATEMENT EXECUTE FUNCTION circle_audit_logs_immutable();
