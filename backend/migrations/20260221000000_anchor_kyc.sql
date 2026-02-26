-- Add KYC status and SEP-24 interactive URL tracking to withdrawals
ALTER TABLE withdrawals
    ADD COLUMN IF NOT EXISTS kyc_status VARCHAR(20) NOT NULL DEFAULT 'PENDING',
    ADD COLUMN IF NOT EXISTS sep24_interactive_url TEXT;

-- Index for querying by KYC status during compliance reviews
CREATE INDEX IF NOT EXISTS idx_withdrawals_kyc_status ON withdrawals(kyc_status);

-- Index for efficient per-user withdrawal lookups
CREATE INDEX IF NOT EXISTS idx_withdrawals_user_id ON withdrawals(user_id);
