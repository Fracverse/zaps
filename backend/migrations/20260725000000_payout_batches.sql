-- BE-555: bulk disbursement schema — batches, recipients, and dispatch logs.
--
-- Three tables rather than one, because they answer three different questions
-- and have three different write patterns:
--
--   payout_batches    — one row per submitted batch. The unit a caller creates,
--                       queries and cancels. Low write volume.
--   batch_recipients  — one row per payout line. The unit the worker claims and
--                       submits to SDP. High write volume, hot status column.
--   dispatch_logs     — append-only audit of every dispatch attempt, including
--                       failures and retries. Never updated.
--
-- Keeping attempts out of batch_recipients is deliberate: a recipient row holds
-- current state and is updated in place, so overwriting it would destroy the
-- history of what was tried. A payout system needs to answer "what did we send,
-- when, and what came back" long after the row reached its terminal status.

-- ─── Batches ─────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS payout_batches (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Caller-supplied idempotency key. A retried create returns the existing
    -- batch instead of double-paying every recipient in it.
    idempotency_key VARCHAR(128) UNIQUE NOT NULL,
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    -- PENDING -> PROCESSING -> COMPLETED | PARTIALLY_FAILED | FAILED | CANCELLED
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING'
        CHECK (status IN ('PENDING', 'PROCESSING', 'COMPLETED', 'PARTIALLY_FAILED', 'FAILED', 'CANCELLED')),
    currency VARCHAR(10) NOT NULL DEFAULT 'NGN',
    -- Denormalised counters. Recomputable from batch_recipients, but the worker
    -- and the status endpoint both read them on every cycle; the alternative is
    -- an aggregate over the whole batch each time.
    total_recipients INTEGER NOT NULL DEFAULT 0 CHECK (total_recipients >= 0),
    total_amount BIGINT NOT NULL DEFAULT 0 CHECK (total_amount >= 0),
    succeeded_count INTEGER NOT NULL DEFAULT 0 CHECK (succeeded_count >= 0),
    failed_count INTEGER NOT NULL DEFAULT 0 CHECK (failed_count >= 0),
    -- Set when the worker claims the batch; NULL means unclaimed.
    started_at TIMESTAMP,
    completed_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    -- Counters can never exceed the batch size, whatever a buggy worker does.
    CONSTRAINT payout_batches_counts_within_total
        CHECK (succeeded_count + failed_count <= total_recipients)
);

-- ─── Recipients ──────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS batch_recipients (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    batch_id UUID NOT NULL REFERENCES payout_batches(id) ON DELETE CASCADE,
    -- Nullable: a payout may target a raw Stellar address for someone who has
    -- no Zaps account yet. At least one of the two must be present.
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    destination_address VARCHAR(56),
    amount BIGINT NOT NULL CHECK (amount > 0),
    -- PENDING -> SUBMITTED -> CONFIRMED | FAILED
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING'
        CHECK (status IN ('PENDING', 'SUBMITTED', 'CONFIRMED', 'FAILED')),
    -- SDP's identifier for the disbursement, once accepted.
    sdp_payment_id VARCHAR(128),
    tx_hash VARCHAR(64),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    last_error TEXT,
    -- Worker lease. Set when a worker claims the row, cleared on terminal
    -- status; lets a second worker detect and reclaim rows abandoned by a
    -- process that died mid-dispatch.
    locked_at TIMESTAMP,
    locked_by VARCHAR(64),
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    CONSTRAINT batch_recipients_has_destination
        CHECK (user_id IS NOT NULL OR destination_address IS NOT NULL)
);

-- One payout per user per batch. This is the guard that makes a retried batch
-- submission safe: a duplicated recipient line is rejected by the database
-- rather than silently paying twice.
CREATE UNIQUE INDEX IF NOT EXISTS idx_batch_recipients_unique_user
    ON batch_recipients(batch_id, user_id)
    WHERE user_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_batch_recipients_unique_address
    ON batch_recipients(batch_id, destination_address)
    WHERE destination_address IS NOT NULL;

-- ─── Dispatch logs ───────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS dispatch_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    batch_id UUID NOT NULL REFERENCES payout_batches(id) ON DELETE CASCADE,
    -- Nullable so a batch-level event (claimed, completed, cancelled) can be
    -- logged without inventing a recipient for it.
    recipient_id UUID REFERENCES batch_recipients(id) ON DELETE CASCADE,
    attempt INTEGER NOT NULL DEFAULT 1 CHECK (attempt >= 1),
    -- The outcome being recorded, not the resulting row state.
    event VARCHAR(30) NOT NULL
        CHECK (event IN ('CLAIMED', 'SUBMITTED', 'CONFIRMED', 'FAILED', 'RETRY_SCHEDULED', 'CANCELLED')),
    sdp_response_code VARCHAR(20),
    detail TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- ─── Indexes ─────────────────────────────────────────────────────────────────

-- The worker's claim query: oldest unclaimed batch in a given status.
CREATE INDEX IF NOT EXISTS idx_payout_batches_status_created
    ON payout_batches(status, created_at);
CREATE INDEX IF NOT EXISTS idx_payout_batches_created_by
    ON payout_batches(created_by);

-- The hot path: next N pending rows for the batch being processed. Partial, so
-- the index stays small as rows reach terminal status and drop out of it —
-- which matters when a batch has 100k recipients and 99% are done.
CREATE INDEX IF NOT EXISTS idx_batch_recipients_pending
    ON batch_recipients(batch_id, created_at)
    WHERE status = 'PENDING';

CREATE INDEX IF NOT EXISTS idx_batch_recipients_batch_status
    ON batch_recipients(batch_id, status);

-- Reclaiming rows abandoned by a dead worker.
CREATE INDEX IF NOT EXISTS idx_batch_recipients_locked
    ON batch_recipients(locked_at)
    WHERE locked_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_dispatch_logs_batch
    ON dispatch_logs(batch_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_dispatch_logs_recipient
    ON dispatch_logs(recipient_id, created_at DESC)
    WHERE recipient_id IS NOT NULL;
