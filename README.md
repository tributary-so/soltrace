# Soltrace

A protocol-agnostic Solana event indexer built in Rust. Soltrace reads Anchor
IDLs supplied by the **operator** (the person running it), decodes both
`emit!` and `emit_cpi!` events from those programs, and stores each decoded
occurrence as a row keyed by where in the transaction it came from. Soltrace
itself knows nothing about any specific Solana program — no IDLs are vendored,
the operator installs them.

> [!IMPORTANT]
> Soltrace is **protocol-agnostic**. It ships with an empty `idls/` directory.
> The operator must fetch and install the Anchor IDL for every program they
> want to index. See [IDL Directory](#idl-directory).

## Key Features

- **Dual emission-path coverage**: indexes both `emit!` (log-scraped) and
  `emit_cpi!` (inner-instruction-extracted) Anchor events
- **IDL-driven decoding**: provide an Anchor IDL per program, get decoded
  events using a borsh decoder that handles primitives, options, vecs, arrays,
  enums, and nested structs
- **Multi-program indexing**: index events from every Anchor program whose IDL
  is in the IDL directory
- **Real-time tracking** (`soltrace-live`): WebSocket ingestion with
  exponential-backoff auto-reconnect and startup catch-up of the gap since the
  last stored signature
- **Historical backfill** (`soltrace-backfill`): paginated `get_signatures_for_address`
  walk with configurable concurrency and rate-limit retry
- **Idempotent reprocess**: every signature can be re-run through the pipeline;
  duplicate Event Rows are silently dropped on the dedup key
- **Flexible storage**: SQLite (default), PostgreSQL (JSONB), MongoDB (documents)
- **Optional Kafka fan-out**: dynamic per-event-name topics
- **Production-ready**: Docker, health checks, comprehensive error handling

## Table of Contents

- [Tech Stack](#tech-stack)
- [Prerequisites](#prerequisites)
- [Getting Started](#getting-started)
- [Architecture](#architecture)
- [IDL Directory](#idl-directory)
- [Configuration](#configuration)
- [Environment Variables](#environment-variables)
- [Available Commands](#available-commands)
- [Testing](#testing)
- [Deployment](#deployment)
- [Troubleshooting](#troubleshooting)
- [Querying Events](#querying-events)

## Tech Stack

- **Language**: Rust 1.70+
- **Blockchain SDK**: Solana Rust SDK (`solana-client`, `solana-sdk`,
  `solana-transaction-status`, `solana-pubsub-client`)
- **Serialization**: Borsh (custom IDL-driven decoder in `soltrace-core`)
- **Anchor hash**: `anchor-lang` 0.31 (for `sha256("event:<name>")[..8]` only)
- **Database**: SQLite, PostgreSQL (JSONB), MongoDB
- **Async runtime**: Tokio 1.0
- **CLI**: Clap 4.x
- **Logging**: `tracing`
- **Container**: Docker & Docker Compose

## Prerequisites

- **Rust 1.70+** ([rustup](https://rustup.rs/)):

  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  source $HOME/.cargo/env
  ```

- **Database** (pick one):

  - **SQLite** (default, embedded):
    ```bash
    # macOS
    brew install sqlite
    # Ubuntu/Debian
    sudo apt-get install sqlite3 libsqlite3-dev
    ```
  - **PostgreSQL 14+**:
    ```bash
    docker run -d --name postgres -e POSTGRES_PASSWORD=password -p 5432:5432 postgres:15
    ```
  - **MongoDB 6+**:
    ```bash
    docker run -d --name mongodb -p 27017:27017 mongo:7
    ```

- **Anchor CLI** (optional, for IDL fetch — see [IDL Directory](#idl-directory)):
  ```bash
  npm install -g @coral-xyz/anchor-cli
  ```

## Getting Started

### 1. Clone and Build

```bash
git clone https://github.com/your-org/soltrace.git
cd soltrace
cargo build --release
```

Binaries land at `target/release/soltrace-live` and `target/release/soltrace-backfill`.

### 2. Install IDLs

Soltrace ships with an empty `idls/` directory. Fetch the Anchor IDL for every
program you want to index:

```bash
anchor idl fetch <PROGRAM_ADDRESS> --provider.cluster https://api.mainnet-beta.solana.com \
  > idls/<name>.json
```

Filename is free-form — soltrace keys by the IDL's top-level `address` field,
not by filename. See [IDL Directory](#idl-directory) for the load contract.

### 3. Configure the program→prefix mapping

Event rows are named `<prefix>_<EventName>`. Configure the prefix per program
via `PROGRAM_PREFIXES` (env) or `--program-prefixes` (CLI):

```bash
PROGRAM_PREFIXES=<program_id_1>:tributary,<program_id_2>:token
```

Programs absent from the mapping fall back to the `default` prefix.

### 4. Initialize the database

```bash
./target/release/soltrace-live init --db-url sqlite:./soltrace.db
```

### 5. Run the real-time indexer

```bash
./target/release/soltrace-live run \
  --program-prefixes "$PROGRAM_PREFIXES" \
  --idl-dir ./idls \
  --db-url sqlite:./soltrace.db \
  --rpc-url https://api.mainnet-beta.solana.com \
  --ws-url wss://api.mainnet-beta.solana.com \
  --commitment confirmed
```

### 6. (Optional) Backfill historical events

```bash
./target/release/soltrace-backfill \
  --program-prefixes "$PROGRAM_PREFIXES" \
  --idl-dir ./idls \
  --db-url sqlite:./soltrace.db \
  --rpc-url https://api.mainnet-beta.solana.com
```

## Architecture

### Workspace layout

```
soltrace/
├── Cargo.toml                    # Workspace
├── Cargo.lock
├── Dockerfile                    # Multi-stage Docker build
├── docker-compose.yml            # Live + backfill services
├── .env.example                  # Example environment variables
├── CONTEXT.md                    # Domain glossary (ubiquitous language)
├── idls/                         # IDL directory (operator-installed; empty by default)
│   └── README.md                 # Operator install guide
├── examples/
│   └── tributary/                # Reference deployment example (Tributary protocol)
├── soltrace-core/                # Core library crate
│   └── src/
│       ├── lib.rs                # Public exports
│       ├── idl.rs                # IdlParser — load + lookup by discriminator
│       ├── idl_event.rs          # IdlEventDecoder — borsh decode of one event
│       ├── event.rs              # EventDecoder — orchestrates lookup → decode → prefix
│       ├── utils.rs              # load_idls, process_transaction, CPI extraction
│       ├── types.rs              # DecodedEvent, RawEvent, CpiEvent, InnerInstructionInfo, …
│       ├── db/                   # sqlite / postgres / mongodb backends + factory
│       ├── retry.rs              # Retry with rate-limit-aware backoff
│       ├── validation.rs         # Input validation helpers
│       ├── metrics.rs            # Health check + metrics
│       └── error.rs              # Error types
├── soltrace-live/                # Real-time indexer binary (WebSocket)
│   └── src/main.rs
└── soltrace-backfill/            # Historical backfill binary (paginated RPC)
    └── src/main.rs
```

### Data flow

Soltrace extracts events via **two emission paths**, both routed through the
same IDL decoder:

```
                         ┌─────────────────────────────┐
                         │  Solana RPC / WebSocket     │
                         └──────────────┬──────────────┘
                                        │
            ┌───────────────────────────┼───────────────────────────┐
            ▼                           ▼                           ▼
   emit!  logs                  emit_cpi! inner ixs         signature notification
   ("Program data: <b64>")      (self-CPI to __event_       (Live mode: triggers
                                authority PDA)              full tx fetch for CPI)
            │                           │
            ▼                           ▼
   extract_event_from_log      extract_inner_instructions
                                 + extract_cpi_events
                                 (strip 8-byte event_cpi
                                  wrapper discriminator)
            │                           │
            └───────────┬───────────────┘
                        ▼
                EventDecoder::decode_event
                   (discriminator → IDL → borsh)
                        │
                        ▼
                  <prefix>_<Event>
                        │
                ┌───────┴───────┐
                ▼               ▼
            Database         Kafka (optional,
            (dedup on         dynamic topic per
            sig+index+name)   event name)
```

### Emission paths

Soltrace supports both Anchor emission macros. The path determines where the
EventPayload surfaces in transaction metadata:

| Path         | Anchor macro | Where it surfaces                                                   | Extraction                                          |
| ------------ | ------------ | ------------------------------------------------------------------- | --------------------------------------------------- |
| **Log path** | `emit!`      | `Program data: <base64>` lines in `meta.logMessages`                | Log-scraping                                        |
| **CPI path** | `emit_cpi!`  | self-CPI to `["__event_authority"]` PDA in `meta.innerInstructions` | Inner-instruction inspection + 8-byte wrapper strip |

Both paths route the unwrapped `<event-disc(8)><borsh>` bytes through the same
`EventDecoder`. A single signature may carry events from both paths; each
becomes its own Event Row (dedup key differs — see [Dedup](#deduplication)).

### Key components

**`IdlParser` (`idl.rs`)** — Loads every `*.json` in the IDL directory at
startup, keyed by each IDL's `address` field. Looks up event definitions by
the 8-byte Anchor discriminator (`sha256("event:<name>")[..8]`).

**`IdlEventDecoder` (`idl_event.rs`)** — Borsh decoder driven by IDL field
definitions. Supports `bool`, `u8`–`u128`, `i8`–`i128`, `string`, `publicKey`,
`bytes`, `option<T>`, `vec<T>`, `[T; N]`, enums (named and tuple variants),
and nested `defined` structs. Falls back to hex on decode failure (with
diagnostic metadata in the row).

**`EventDecoder` (`event.rs`)** — Orchestrates: discriminator lookup → borsh
decode → `<prefix>_<EventName>` naming. Prefix comes from `ProgramPrefixConfig`
(`PROGRAM_PREFIXES`).

**`extract_inner_instructions` / `extract_cpi_events` (`utils.rs`)** —
Resolves inner-instruction account keys against the versioned-tx-aware key
table, filters self-CPIs to `__event_authority` PDAs, strips the 8-byte
`event_cpi` wrapper discriminator (`0x1d9acb512ea545e4`).

**`cpi_dedup_index` (`utils.rs`)** — Derives a stable dedup index for CPI
events from their on-chain `(outer_ix, inner_ix)` position. See
[Deduplication](#deduplication).

**Database (`db/`)** — Trait-based abstraction with SQLite, PostgreSQL, and
MongoDB backends, auto-selected by URL scheme. SQLite stores data as JSON
text; PostgreSQL uses JSONB + GIN index; MongoDB stores nested documents.

**Real-time indexer (`soltrace-live`)** — Subscribes to `logsSubscribe` for
the configured programs, decodes `emit!` events from the log stream, and
fetches the full transaction on every notification to extract `emit_cpi!`
events from inner instructions. Runs startup catch-up (gap-fill) concurrently
with the WebSocket subscription.

**Historical backfill (`soltrace-backfill`)** — Paginates
`get_signatures_for_address` backwards from the chain tip per program, fetches
each transaction, processes it. Concurrent fetches with rate-limit-aware retry.

### Deduplication

Every Event Row is identified by `(signature, index, event_name)`:

- **signature**: the Solana transaction signature (one signature → many rows)
- **index**: the position of the event within the transaction
  - log-path events: sequential counter starting at 0
  - CPI-path events: `1_000_000_000 + outer_ix * 100_000 + inner_ix` (offset
    to never collide with the log counter; ceiling far above any real tx's
    inner-instruction count given Solana's ~1232-byte tx limit)
- **event_name**: `<prefix>_<EventName>`

This makes reprocessing idempotent (`ON CONFLICT DO NOTHING`) and lets a
signature carry events from both paths without collision.

### Database schema

SQLite (default):

```sql
CREATE TABLE events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id    TEXT NOT NULL UNIQUE,           -- hash of (sig, index, name)
    slot        INTEGER NOT NULL,
    signature   TEXT NOT NULL,
    ix_index    INTEGER NOT NULL,               -- log counter or CPI offset
    program_id  TEXT NOT NULL,
    event_name  TEXT NOT NULL,                  -- <prefix>_<EventName>
    discriminator TEXT NOT NULL,                -- 16-char hex
    data        TEXT NOT NULL,                  -- JSON-encoded event fields
    log         TEXT,                           -- source log line (empty for CPI)
    timestamp   TEXT NOT NULL
);
```

PostgreSQL mirrors this with `JSONB` for `data` plus a GIN index. MongoDB
stores `data` as a nested document.

## IDL Directory

The IDL directory (default `./idls`, override via `IDL_DIR` or `--idl-dir`)
holds the Anchor IDL JSON files for every program soltrace should index.

> [!IMPORTANT] > **IDLS are loaded exactly once at process startup.** Adding, removing, or
> modifying an IDL while `soltrace-live` or `soltrace-backfill` is running has
> no effect — restart the process to pick up changes.

Each IDL must be valid JSON with at least:

```jsonc
{
  "address": "<base58 program id>", // required — the lookup key
  "events": [
    /* Anchor event defs */
  ],
  "types": [
    /* referenced structs/enums */
  ],
}
```

If the directory is missing or empty, soltrace logs a warning and continues.
Unknown-discriminator events are skipped at debug level (never crash). See
`idls/README.md` for the install recipe.

## Configuration

### Program→prefix mapping

Format: `<program_id>:<prefix>` pairs, comma-separated.

```bash
# env
PROGRAM_PREFIXES=TRibg8W8zmPHQqWtyAD1rEBRXEdyU13Mu6qX1Sg42tJ:tributary,TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA:token

# CLI (same shape)
--program-prefixes "TRibg8W8zmPHQqWtyAD1rEBRXEdyU13Mu6qX1Sg42tJ:tributary,..."
```

Programs in the IDL directory but absent from this mapping get the `default`
prefix.

### IDL field types supported

`bool`, `u8`, `u16`, `u32`, `u64`, `u128`, `i8`, `i16`, `i32`, `i64`, `i128`,
`string`, `publicKey` / `pubkey` / `Pubkey`, `bytes`, `option<T>`, `vec<T>`,
`[T; N]`, plus the JSON-object forms (`{"option": T}`, `{"vec": T}`,
`{"array": [T, N]}`, `{"defined": "T"}` / `{"defined": {"name": "T"}}`) and
enums (named and tuple-struct variants).

u64/u128/i64/i128 decode to JSON **strings** (lossless); smaller ints decode
to JSON numbers.

## Environment Variables

| Variable                 | Description                                                | Default                               |
| ------------------------ | ---------------------------------------------------------- | ------------------------------------- |
| `SOLANA_RPC_URL`         | Solana HTTP RPC endpoint                                   | `https://api.mainnet-beta.solana.com` |
| `SOLANA_WS_URL`          | Solana WebSocket endpoint (live only)                      | `wss://api.mainnet-beta.solana.com`   |
| `PROGRAM_PREFIXES`       | `program_id:prefix,...` mapping for row naming             | (required)                            |
| `DB_URL`                 | Database URL (`sqlite:` / `postgres:` / `mongodb:`)        | `sqlite:./soltrace.db`                |
| `IDL_DIR`                | IDL directory (loaded once at startup)                     | `./idls`                              |
| `COMMITMENT`             | Solana commitment: `processed` / `confirmed` / `finalized` | `confirmed`                           |
| `RECONNECT_DELAY`        | WebSocket reconnect base delay (seconds)                   | `5`                                   |
| `MAX_RECONNECT_ATTEMPTS` | WS reconnect cap (0 = infinite)                            | `0`                                   |
| `WS_PING_INTERVAL`       | WebSocket read-timeout / keep-alive (seconds; 0 = disable) | `30`                                  |
| `MAX_RETRIES`            | Max retries for RPC calls (live catch-up + CPI fetch)      | `3`                                   |
| `NO_GAP_BACKFILL`        | Disable startup catch-up (live only)                       | unset                                 |
| `KAFKA_BROKERS`          | Kafka brokers (comma-separated; enables fan-out)           | unset                                 |
| `LIMIT`                  | Signatures page size (backfill)                            | `1000`                                |
| `BATCH_SIZE`             | Tx fetch batch size (backfill)                             | `100`                                 |
| `BATCH_DELAY`            | Delay between batches in ms (backfill)                     | `100`                                 |
| `CONCURRENCY`            | Concurrent tx fetches (backfill)                           | `10`                                  |
| `LOG_LEVEL`              | `tracing` verbosity                                        | `info`                                |

## Available Commands

### `soltrace-live`

```bash
soltrace-live init --db-url <DATABASE_URL>

soltrace-live run \
  --program-prefixes <MAPPING> \
  --ws-url <WS_URL> \
  --rpc-url <RPC_URL> \
  --db-url <DB_URL> \
  --idl-dir <IDL_DIR> \
  --commitment <processed|confirmed|finalized> \
  --reconnect-delay <SECONDS> \
  --max-reconnects <COUNT> \
  --ping-interval <SECONDS> \
  --max-retries <COUNT> \
  [--no-gap-backfill] \
  [--kafka-brokers <BROKERS>]
```

### `soltrace-backfill`

```bash
soltrace-backfill \
  --program-prefixes <MAPPING> \
  --rpc-url <RPC_URL> \
  --db-url <DB_URL> \
  --idl-dir <IDL_DIR> \
  --limit <COUNT> \
  --batch-size <SIZE> \
  --batch-delay <MS> \
  --concurrency <COUNT> \
  --max-retries <COUNT>
```

## Testing

```bash
cargo test --workspace                          # all tests
cargo test --workspace -- --nocapture           # with output
cargo test --package soltrace-core              # core only
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

Coverage focuses on:

- `IdlParser`: discriminator calculation, event lookup, type-array field resolution
- `IdlEventDecoder`: every supported borsh type, nested structs, enums (named + tuple), error paths
- `utils`: `extract_event_from_log`, `extract_inner_instructions`, `extract_cpi_events`,
  `decode_cpi_events`, `cpi_dedup_index`, hex fallback on bad borsh
- DB operations, retry logic, validation, metrics

## Deployment

### Docker Compose (recommended)

```bash
docker-compose build
docker-compose up -d
docker-compose logs -f soltrace-live
docker-compose down       # stop
docker-compose down -v    # stop + delete database volume
```

`docker-compose.yml` defines `soltrace-live` (long-running) and
`soltrace-backfill` (one-shot after live starts). Both share a named volume
for the database and a read-only mount for `idls/`.

### Docker

```bash
docker build -t soltrace .
docker run -d --name soltrace-live \
  -p 8080:8080 \
  -v $(pwd)/idls:/idls:ro \
  -v $(pwd)/data:/data \
  -e SOLANA_RPC_URL=https://api.mainnet-beta.solana.com \
  -e SOLANA_WS_URL=wss://api.mainnet-beta.solana.com \
  -e DB_URL=sqlite:/data/soltrace.db \
  -e IDL_DIR=/idls \
  -e PROGRAM_PREFIXES="$PROGRAM_PREFIXES" \
  soltrace soltrace-live run
```

### Manual / VPS / systemd

```bash
cargo build --release
scp target/release/soltrace-live user@server:/opt/soltrace/

cat > /etc/systemd/system/soltrace.service <<'EOF'
[Unit]
Description=Soltrace Live Indexer
After=network.target

[Service]
Type=simple
User=soltrace
WorkingDirectory=/opt/soltrace
EnvironmentFile=/opt/soltrace/.env
ExecStart=/opt/soltrace/soltrace-live run
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

systemctl enable --now soltrace
```

### Kubernetes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: soltrace-live
spec:
  replicas: 1
  selector:
    matchLabels: { app: soltrace-live }
  template:
    metadata:
      labels: { app: soltrace-live }
    spec:
      containers:
        - name: soltrace
          image: your-registry/soltrace:latest
          env:
            - { name: PROGRAM_PREFIXES, value: "<program_id>:<prefix>" }
            - { name: DB_URL, value: "sqlite:/data/soltrace.db" }
            - { name: IDL_DIR, value: "/idls" }
          volumeMounts:
            - { name: data, mountPath: /data }
            - { name: idls, mountPath: /idls }
      volumes:
        - { name: data, persistentVolumeClaim: { claimName: soltrace-data } }
        - { name: idls, configMap: { name: soltrace-idls } }
```

## Troubleshooting

### WebSocket reconnects constantly

- Confirm `wss://` scheme (not `ws://`) for public endpoints
- Check firewall egress on 443
- Try a paid RPC (Helius, QuickNode, Triton) — public endpoints throttle
- Raise `--reconnect-delay`

### Events decode as `{"hex": "..."}` (hex fallback)

`IdlEventDecoder` failed to borsh-decode the payload. Causes:

1. **IDL version mismatch** — the deployed program emits a field layout the
   installed IDL doesn't describe. Re-fetch the IDL.
2. **Wrong IDL keyed** — IDL filename is irrelevant, but the top-level
   `address` field must equal the on-chain program id.
3. **Undecodable type** — soltrace's decoder doesn't cover every borsh shape
   yet. The `decode_error` field in the hex payload names the cause.

### `No event found with discriminator`

No loaded IDL defines an event whose computed discriminator matches the
on-chain bytes for that program. The event will be skipped at debug level.

### `Failed to read IDL directory`

`IDL_DIR` points nowhere or is unreadable. Soltrace continues with no IDLs
(warn-level log); no events will be decoded. Restart after fixing.

### RPC rate limiting (`429`)

- Raise `--batch-delay`, lower `--batch-size` and `--concurrency` (backfill)
- Raise `--max-retries` to let the built-in backoff recover
- Use a paid RPC endpoint

### `UNIQUE constraint failed` / `duplicate key`

**This is normal** — dedup working as designed. The row already exists;
reprocessing is idempotent. No action needed.

### `database is locked` (SQLite)

Concurrency pressure. Either lower `--concurrency` or move to PostgreSQL /
MongoDB for high-throughput programs.

### Build errors

```bash
# missing C toolchain
sudo apt-get install build-essential libsqlite3-dev   # Debian/Ubuntu
xcode-select --install                                  # macOS
```

## Querying Events

### SQLite

```bash
sqlite3 soltrace.db "SELECT * FROM events WHERE program_id = '<program_id>' LIMIT 10;"
sqlite3 soltrace.db "SELECT * FROM events WHERE event_name = '<prefix>_Transfer';"
sqlite3 soltrace.db "SELECT * FROM events WHERE slot BETWEEN 123456 AND 123500;"
sqlite3 soltrace.db "SELECT program_id, event_name, COUNT(*) FROM events GROUP BY program_id, event_name;"
sqlite3 soltrace.db "SELECT * FROM events ORDER BY timestamp DESC LIMIT 100;"
```

### PostgreSQL

```sql
SELECT * FROM events WHERE event_name = '<prefix>_Transfer';
SELECT * FROM events WHERE event_name = '<prefix>_Transfer' AND data->>'amount' > '1000';
SELECT * FROM events WHERE data->>'from' = '<sender_pubkey>';
SELECT program_id, event_name, COUNT(*) FROM events GROUP BY program_id, event_name;
```

### MongoDB

```javascript
db.events.find({ program_id: "<program_id>" }).limit(10);
db.events.find({ event_name: "<prefix>_Transfer" });
db.events.find({
  event_name: "<prefix>_Transfer",
  "data.amount": { $gt: 1000 },
});
db.events.aggregate([{ $group: { _id: "$program_id", count: { $sum: 1 } } }]);
```

## License

MIT — see [LICENSE](LICENSE).

## Acknowledgments

- Built on [Anchor](https://www.anchor-lang.com/)
- Uses the [Solana Rust SDK](https://github.com/solana-labs/solana)
- Database via [SQLx](https://github.com/launchbadge/sqlx)
- Async runtime by [Tokio](https://tokio.rs/)
