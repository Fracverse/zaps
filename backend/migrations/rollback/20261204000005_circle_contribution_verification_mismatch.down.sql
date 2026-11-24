UPDATE circle_contribution_verifications SET outcome = 'failed' WHERE outcome = 'mismatch';

ALTER TABLE circle_contribution_verifications
    DROP CONSTRAINT IF EXISTS circle_contribution_verifications_outcome_check;
ALTER TABLE circle_contribution_verifications
    ADD CONSTRAINT circle_contribution_verifications_outcome_check
    CHECK (outcome IN ('confirmed', 'failed', 'not_found', 'unavailable'));
