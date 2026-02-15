# Soltrace - Technical Documentation

## Architecture Overview

Soltrace consists of three main components:

1. **soltrace-core**: Shared library providing IDL parsing, event decoding, and database operations
2. **soltrace-live**: Real-time event tracker using WebSocket subscriptions
3. **soltrace-backfill**: Historical event backfiller using RPC polling

## Core Components

### IDL Parser (`soltrace-core/src/idl.rs`)

The IDL parser loads Anchor IDL files and provides:

- IDL loading from JSON files/strings
- Event definition extraction
- Discriminator calculation: `sha256("event:<name>")[..8]`
- Event lookup by discriminator

**Key Functions:**
- `load_from_file()`: Load IDL from JSON file
- `calculate_discriminator()`: Compute event discriminator
- `find_event_by_discriminator()`: Find event by discriminator

### Event Decoder (`soltrace-core/src/event.rs`)

Decodes raw Anchor event bytes into structured data:

**Anchor Event Format:**
```
[8 bytes discriminator][borsh-encoded event data]
```

**Current Implementation:**
- Extracts discriminator (first 8 bytes)
- Returns hex-encoded event data
- Planned: Full type-aware decoding based on IDL types

### Database (`soltrace-core/src/db.rs`)

SQLite-based storage with PostgreSQL compatibility:

**Schema:**
- `events` table with indexes on: slot, signature, program_id, event_name, timestamp
- Supports querying by slot range, program, event name

**Key Functions:**
- `insert_event()`: Store decoded event
- `get_events_by_slot_range()`: Query events in slot range
- `get_latest_slot()`: Get last indexed slot for program

## Real-time Tracking (soltrace-live)

### WebSocket Connection

Uses Solana's WebSocket API with log subscription:

```
ws://<rpc-url>
→ subscribe: {"method": "logsSubscribe", "params": [...]}
← receive: log messages in real-time
```

### Event Processing Flow

1. Parse incoming WebSocket log messages
2. Check if logs match configured program IDs
3. Extract event data from program logs
4. Decode events using IDL parser
5. Store in database (deduplicate by signature)

### CLI Usage

```bash
# Initialize database
cargo run --bin soltrace-live -- init

# Start real-time tracking
cargo run --bin soltrace-live -- run \
  --programs "progId1,progId2" \
  --idl-dir ./idls \
  --db-url sqlite:./soltrace.db
```

## Historical Backfill (soltrace-backfill)

### Backfill Strategy

1. Query blocks in slot range using `getBlocks()`
2. For each block, fetch transactions with `getBlock()`
3. Parse transaction logs for program events
4. Decode and store events

### CLI Usage

```bash
cargo run --bin soltrace-backfill \
  --start-slot 123456 \
  --end-slot 123500 \
  --programs "progId1,progId2" \
  --batch-size 100 \
  --batch-delay 100
```

## IDL Format

Anchor IDLs define program events:

```json
{
  "version": "0.31.1",
  "name": "my_program",
  "address": "ProgRamAddress111111111111111111111111",
  "events": [
    {
      "name": "Transfer",
      "fields": [
        {"name": "from", "type": "publicKey"},
        {"name": "to", "type": "publicKey"},
        {"name": "amount", "type": "u64"}
      ]
    }
  ]
}
```

## Event Discriminator Calculation

Anchor uses SHA256 to create event discriminators:

```rust
let preimage = format!("event:{}", event_name);
let hash = sha256(preimage);
let discriminator = hash[..8]; // First 8 bytes
```

Example:
- Event: "Transfer"
- Preimage: "event:Transfer"
- Discriminator: First 8 bytes of SHA256("event:Transfer")

## Database Schema

```sql
CREATE TABLE events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    slot INTEGER NOT NULL,
    signature TEXT NOT NULL,
    program_id TEXT NOT NULL,
    event_name TEXT NOT NULL,
    discriminator TEXT NOT NULL,
    data TEXT NOT NULL,  -- JSON-encoded event data
    timestamp TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('utc'))
);

CREATE INDEX idx_slot ON events(slot);
CREATE INDEX idx_signature ON events(signature);
CREATE INDEX idx_program_id ON events(program_id);
CREATE INDEX idx_event_name ON events(event_name);
CREATE INDEX idx_timestamp ON events(timestamp);
```

## Migration to PostgreSQL

The current SQLite implementation is designed for easy migration to PostgreSQL:

1. Change `db_url` to PostgreSQL connection string
2. Update `Database::new()` to use `sqlx::postgres::PgPool`
3. Adjust SQL syntax (AUTOINCREMENT → SERIAL, datetime('utc') → NOW())
4. Consider TimescaleDB hypertables for time-series optimization

## Future Enhancements

1. **Full Type Decoding**: Decode borsh-encoded data based on IDL types
2. **Event Filters**: Subscribe to specific event types only
3. **Multi-Threaded Processing**: Parallel event decoding and storage
4. **Reorg Handling**: Detect and handle blockchain reorganizations
5. **GraphQL API**: Expose indexed events via GraphQL
6. **Metrics**: Prometheus metrics for indexer health

## Performance Considerations

- **Batch Size**: Tune based on RPC rate limits (default: 100)
- **Batch Delay**: Prevent rate limit errors (default: 100ms)
- **Database Indexing**: Critical for query performance
- **Memory Usage**: Event decoder keeps IDLs in memory

## Error Handling

- **Connection Failures**: Retry with exponential backoff
- **Invalid Data**: Log and continue, don't stop indexing
- **Duplicate Events**: Deduplicate by transaction signature
- **Missing IDL**: Store raw hex data without decoding
