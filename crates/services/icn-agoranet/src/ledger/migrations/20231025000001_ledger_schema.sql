-- Entity table to track all participants in the system
CREATE TABLE entities (
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    federation_id TEXT NOT NULL,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (entity_type, entity_id)
);

-- Balances table for current state
CREATE TABLE balances (
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    balance BIGINT NOT NULL DEFAULT 0,
    last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (entity_type, entity_id),
    FOREIGN KEY (entity_type, entity_id) REFERENCES entities (entity_type, entity_id)
);

-- Transactions table with DAG-friendly structure
CREATE TABLE transfers (
    tx_id UUID PRIMARY KEY,
    federation_id TEXT NOT NULL,
    from_type TEXT NOT NULL,
    from_id TEXT NOT NULL,
    to_type TEXT NOT NULL,
    to_id TEXT NOT NULL,
    amount BIGINT NOT NULL CHECK (amount > 0),
    fee BIGINT NOT NULL DEFAULT 0,
    initiator TEXT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    memo TEXT,
    metadata JSONB DEFAULT '{}',
    
    -- DAG-related fields
    parent_tx_ids UUID[] DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'confirmed',
    consensus_data JSONB DEFAULT '{}',
    merkle_proof BYTEA,
    signature BYTEA,
    
    -- Foreign key constraints
    FOREIGN KEY (from_type, from_id) REFERENCES entities (entity_type, entity_id),
    FOREIGN KEY (to_type, to_id) REFERENCES entities (entity_type, entity_id)
);

-- Federation statistics table
CREATE TABLE federation_stats (
    federation_id TEXT PRIMARY KEY,
    total_transfers BIGINT NOT NULL DEFAULT 0,
    total_volume BIGINT NOT NULL DEFAULT 0,
    total_fees BIGINT NOT NULL DEFAULT 0,
    last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for efficient querying
CREATE INDEX idx_transfers_federation ON transfers(federation_id);
CREATE INDEX idx_transfers_timestamp ON transfers(timestamp);
CREATE INDEX idx_transfers_from ON transfers(from_type, from_id);
CREATE INDEX idx_transfers_to ON transfers(to_type, to_id);
CREATE INDEX idx_transfers_parents ON transfers USING GIN(parent_tx_ids);
CREATE INDEX idx_entities_federation ON entities(federation_id); 