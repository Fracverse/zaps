-- BE-017: Cross-chain bridge transaction tracking
-- Stores deposits submitted to Allbridge so their status can be polled periodically.

CREATE TABLE IF NOT EXISTS bridge_transactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_tx_hash VARCHAR(128) UNIQUE NOT NULL, -- Source chain deposit tx hash / id
    source_chain VARCHAR(20) NOT NULL DEFAULT 'STLR', -- Allbridge chain symbol (e.g. STLR, ETH, BSC)
    destination_chain VARCHAR(20),
    destination_address VARCHAR(128),
    amount VARCHAR(78), -- Raw amount as string (supports big integers / decimals)
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING', -- PENDING, SUCCESS, FAILED
    confirmations INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- Used by the background poller to find non-terminal transactions efficiently.
CREATE INDEX IF NOT EXISTS idx_bridge_tx_status ON bridge_transactions(status);
CREATE INDEX IF NOT EXISTS idx_bridge_tx_created_at ON bridge_transactions(created_at DESC);
