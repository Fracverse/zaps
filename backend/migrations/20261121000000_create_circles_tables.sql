-- Circles: pooled savings groups backed by the Zaps Circle Soroban contract.

CREATE TABLE IF NOT EXISTS circles (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name              TEXT        NOT NULL,
    admin_address     TEXT        NOT NULL,
    contract_address  TEXT,
    token_address     TEXT        NOT NULL,
    balance           BIGINT      NOT NULL DEFAULT 0,
    reserved          BIGINT      NOT NULL DEFAULT 0,
    max_payout        BIGINT      NOT NULL DEFAULT 0,
    threshold         INT         NOT NULL DEFAULT 1,
    total_contributed BIGINT      NOT NULL DEFAULT 0,
    total_paid        BIGINT      NOT NULL DEFAULT 0,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_circles_admin ON circles (admin_address);

CREATE TABLE IF NOT EXISTS circle_members (
    circle_id         UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    address           TEXT        NOT NULL,
    total_contributed BIGINT      NOT NULL DEFAULT 0,
    joined_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (circle_id, address)
);

CREATE INDEX IF NOT EXISTS idx_circle_members_address ON circle_members (address);

CREATE TABLE IF NOT EXISTS circle_approvers (
    circle_id UUID NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    address   TEXT NOT NULL,
    PRIMARY KEY (circle_id, address)
);

CREATE TABLE IF NOT EXISTS circle_contributions (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    circle_id      UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    member_address TEXT        NOT NULL,
    amount         BIGINT      NOT NULL,
    tx_hash        TEXT        NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_circle_contributions_circle ON circle_contributions (circle_id);
CREATE INDEX IF NOT EXISTS idx_circle_contributions_member ON circle_contributions (member_address);

CREATE TABLE IF NOT EXISTS circle_payout_requests (
    id                   UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    circle_id            UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    request_id           BIGINT      NOT NULL,
    requester_address    TEXT        NOT NULL,
    to_address           TEXT        NOT NULL,
    amount               BIGINT      NOT NULL,
    asset_address        TEXT        NOT NULL,
    threshold_at_creation INT        NOT NULL,
    status               TEXT        NOT NULL DEFAULT 'pending',
    is_executed          BOOLEAN     NOT NULL DEFAULT FALSE,
    tx_hash              TEXT,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (circle_id, request_id)
);

CREATE INDEX IF NOT EXISTS idx_circle_payout_requests_circle ON circle_payout_requests (circle_id);
CREATE INDEX IF NOT EXISTS idx_circle_payout_requests_requester ON circle_payout_requests (requester_address);

CREATE TABLE IF NOT EXISTS circle_payout_approvals (
    circle_id        UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    request_id       BIGINT      NOT NULL,
    approver_address TEXT        NOT NULL,
    approved_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (circle_id, request_id, approver_address)
);

CREATE TABLE IF NOT EXISTS circle_goals (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    circle_id      UUID        NOT NULL REFERENCES circles (id) ON DELETE CASCADE,
    name           TEXT        NOT NULL,
    target_amount  BIGINT      NOT NULL,
    current_amount BIGINT      NOT NULL DEFAULT 0,
    deadline       TIMESTAMPTZ,
    completed      BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_circle_goals_circle ON circle_goals (circle_id);
