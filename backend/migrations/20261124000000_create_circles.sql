CREATE TABLE circles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    name VARCHAR(120) NOT NULL,
    description TEXT NOT NULL,
    objective TEXT NOT NULL,
    goal_amount BIGINT NOT NULL CHECK (goal_amount > 0),
    currency VARCHAR(10) NOT NULL DEFAULT 'NGN',
    contribution_amount BIGINT NOT NULL CHECK (contribution_amount > 0),
    contribution_frequency VARCHAR(10) NOT NULL
        CHECK (contribution_frequency IN ('DAILY', 'WEEKLY', 'MONTHLY')),
    contribution_rules TEXT NOT NULL DEFAULT '',
    membership_restrictions TEXT NOT NULL DEFAULT '',
    visibility VARCHAR(10) NOT NULL CHECK (visibility IN ('PUBLIC', 'PRIVATE')),
    join_enabled BOOLEAN NOT NULL DEFAULT true,
    member_limit INTEGER CHECK (member_limit IS NULL OR member_limit > 0),
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE circle_memberships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    circle_id UUID NOT NULL REFERENCES circles(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    joined_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE (circle_id, user_id)
);

CREATE TABLE circle_contributions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    circle_id UUID NOT NULL,
    user_id UUID NOT NULL,
    amount BIGINT NOT NULL CHECK (amount > 0),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    FOREIGN KEY (circle_id, user_id)
        REFERENCES circle_memberships(circle_id, user_id) ON DELETE CASCADE
);

CREATE INDEX idx_circle_memberships_user_id ON circle_memberships(user_id);
CREATE INDEX idx_circle_contributions_circle_id ON circle_contributions(circle_id);
