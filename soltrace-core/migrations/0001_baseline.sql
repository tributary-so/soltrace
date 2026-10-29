-- Baseline: the pre-v2 `events` schema exactly as the hand-rolled
-- run_migrations created it. Existing deployments already have this;
-- fresh databases create it here and are transformed by 0002 immediately.
CREATE TABLE IF NOT EXISTS events (
    id BYTEA PRIMARY KEY,
    slot BIGINT NOT NULL,
    signature VARCHAR(88) NOT NULL,
    event_name VARCHAR(64) NOT NULL,
    data JSONB NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_slot ON events(slot);
CREATE INDEX IF NOT EXISTS idx_event_name ON events(event_name);
CREATE INDEX IF NOT EXISTS idx_timestamp ON events(timestamp);
CREATE INDEX IF NOT EXISTS idx_signature ON events(signature);
CREATE INDEX IF NOT EXISTS idx_data_gin ON events USING GIN (data);
