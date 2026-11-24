-- #1203: versioned contribution rules.
--
-- `circle_contribution_rules` keeps holding the *current* rule of a circle so
-- existing readers (circle detail, reminder scheduler) keep working. Every
-- change bumps `version` and appends an immutable snapshot below.

ALTER TABLE circle_contribution_rules
    ADD COLUMN IF NOT EXISTS version            INT         NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS fixed_amount       BIGINT,
    ADD COLUMN IF NOT EXISTS deadline           TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS due_offset_hours   INT,
    ADD COLUMN IF NOT EXISTS grace_period_hours INT         NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS late_policy        TEXT        NOT NULL DEFAULT 'accept',
    ADD COLUMN IF NOT EXISTS late_penalty_bps   INT         NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS allowed_assets     TEXT[]      NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS member_overrides   JSONB       NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS updated_by         TEXT;

ALTER TABLE circle_contribution_rules
    DROP CONSTRAINT IF EXISTS circle_contribution_rules_late_policy_check;
ALTER TABLE circle_contribution_rules
    ADD CONSTRAINT circle_contribution_rules_late_policy_check
    CHECK (late_policy IN ('accept', 'reject', 'penalty'));

CREATE TABLE IF NOT EXISTS circle_contribution_rule_versions (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    rule_id    UUID        NOT NULL REFERENCES circle_contribution_rules (id) ON DELETE CASCADE,
    circle_id  UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    version    INT         NOT NULL,
    spec       JSONB       NOT NULL,
    created_by TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (rule_id, version)
);

CREATE INDEX IF NOT EXISTS idx_circle_contribution_rule_versions_circle
    ON circle_contribution_rule_versions (circle_id, created_at DESC);

-- Existing rules become version 1.
INSERT INTO circle_contribution_rule_versions (rule_id, circle_id, version, spec, created_by, created_at)
SELECT r.id, r.circle_id, 1,
       jsonb_build_object(
           'min_amount', r.min_amount::text,
           'max_amount', r.max_amount::text,
           'frequency', r.frequency,
           'is_mandatory', r.is_mandatory
       ),
       NULL, r.created_at
  FROM circle_contribution_rules r
 WHERE NOT EXISTS (
     SELECT 1 FROM circle_contribution_rule_versions v WHERE v.rule_id = r.id
 );
