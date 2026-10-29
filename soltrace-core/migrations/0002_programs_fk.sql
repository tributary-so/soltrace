-- v2 cutover: program identity moves out of event_name into a programs
-- dimension table. `events` becomes `event_rows` (bare event_name +
-- program_id FK); a VIEW named `events` preserves the legacy shape.
--
-- Legacy rows are NOT rewritten here: the prefix->program mapping comes from
-- operator config (--program-prefixes), which static SQL cannot see. The
-- seed_programs() startup step does the rewrite; until then rows keep their
-- prefixed event_name with program_id NULL and the VIEW passes it through.

CREATE TABLE IF NOT EXISTS programs (
    id SERIAL PRIMARY KEY,
    address TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL
);

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = 'events'
    ) THEN
        ALTER TABLE events RENAME TO event_rows;
    END IF;
END
$$;

-- No-op on databases already renamed (column exists); adds the FK on the
-- freshly-renamed table. Indexes (incl. GIN) follow the rename automatically.
ALTER TABLE event_rows ADD COLUMN IF NOT EXISTS program_id INTEGER
    REFERENCES programs(id);

CREATE INDEX IF NOT EXISTS idx_program_id ON event_rows(program_id);
-- Partial index keeps the startup backfill cheap: empty forever once every
-- legacy row has been mapped.
CREATE INDEX IF NOT EXISTS idx_event_rows_unmapped ON event_rows(program_id)
    WHERE program_id IS NULL;

-- Legacy compat surface: byte-identical to the pre-v2 events table for
-- migrated rows (name || '_' || bare) and unmigrated ones (name as stored).
-- Kept indefinitely; documented as deprecated.
CREATE OR REPLACE VIEW events AS
SELECT e.id,
       e.slot,
       e.signature,
       COALESCE(p.name || '_' || e.event_name, e.event_name) AS event_name,
       e.data,
       e.timestamp
FROM event_rows e
LEFT JOIN programs p ON p.id = e.program_id;
