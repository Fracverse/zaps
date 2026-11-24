-- A verification attempt can now be rejected as a mismatch without closing the
-- contribution (the submitted transaction does not pay it, the contribution
-- stays open), so the attempt log needs an outcome for it.

ALTER TABLE circle_contribution_verifications
    DROP CONSTRAINT IF EXISTS circle_contribution_verifications_outcome_check;
ALTER TABLE circle_contribution_verifications
    ADD CONSTRAINT circle_contribution_verifications_outcome_check
    CHECK (outcome IN ('confirmed', 'failed', 'mismatch', 'not_found', 'unavailable'));
