DROP TABLE IF EXISTS circle_contribution_rule_versions;

ALTER TABLE circle_contribution_rules
    DROP CONSTRAINT IF EXISTS circle_contribution_rules_late_policy_check,
    DROP COLUMN IF EXISTS updated_by,
    DROP COLUMN IF EXISTS member_overrides,
    DROP COLUMN IF EXISTS allowed_assets,
    DROP COLUMN IF EXISTS late_penalty_bps,
    DROP COLUMN IF EXISTS late_policy,
    DROP COLUMN IF EXISTS grace_period_hours,
    DROP COLUMN IF EXISTS due_offset_hours,
    DROP COLUMN IF EXISTS deadline,
    DROP COLUMN IF EXISTS fixed_amount,
    DROP COLUMN IF EXISTS version;
