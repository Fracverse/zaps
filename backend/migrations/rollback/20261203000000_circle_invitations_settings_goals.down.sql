-- Reverts 20261203000000_circle_invitations_settings_goals.sql.

DROP INDEX IF EXISTS idx_circle_goals_circle_open;
ALTER TABLE circle_goals DROP CONSTRAINT IF EXISTS circle_goals_target_check;
ALTER TABLE circle_goals
    DROP COLUMN IF EXISTS description,
    DROP COLUMN IF EXISTS created_by,
    DROP COLUMN IF EXISTS completed_at,
    DROP COLUMN IF EXISTS completed_by,
    DROP COLUMN IF EXISTS updated_at;

ALTER TABLE circles DROP CONSTRAINT IF EXISTS circles_settings_check;
ALTER TABLE circles
    DROP COLUMN IF EXISTS invitation_expiry_hours,
    DROP COLUMN IF EXISTS allow_join_requests,
    DROP COLUMN IF EXISTS max_active_goals;

DROP TABLE IF EXISTS circle_invitation_redemptions;
DROP INDEX IF EXISTS idx_circle_invitations_status_expiry;
DROP INDEX IF EXISTS idx_circle_invitations_token_hash;
DROP INDEX IF EXISTS idx_circle_invitations_active_invitee;

-- Link invitations have no invitee and cannot survive the old schema.
DELETE FROM circle_invitations WHERE invite_type = 'link';
UPDATE circle_invitations SET status = 'expired' WHERE status = 'revoked';

ALTER TABLE circle_invitations DROP CONSTRAINT IF EXISTS circle_invitations_uses_check;
ALTER TABLE circle_invitations DROP CONSTRAINT IF EXISTS circle_invitations_invite_type_check;
ALTER TABLE circle_invitations DROP CONSTRAINT IF EXISTS circle_invitations_status_check;
ALTER TABLE circle_invitations
    ADD CONSTRAINT circle_invitations_status_check
    CHECK (status IN ('pending', 'accepted', 'declined', 'expired'));
ALTER TABLE circle_invitations
    DROP COLUMN IF EXISTS invite_type,
    DROP COLUMN IF EXISTS token_hash,
    DROP COLUMN IF EXISTS max_uses,
    DROP COLUMN IF EXISTS use_count,
    DROP COLUMN IF EXISTS responded_by,
    DROP COLUMN IF EXISTS responded_at,
    DROP COLUMN IF EXISTS updated_at,
    ALTER COLUMN invitee_address SET NOT NULL;
