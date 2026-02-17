# Soltrace

A high-performance, IDL-driven Solana event indexer built in Rust. Soltrace indexes Anchor events from Solana programs using the program's IDL for automatic decoding. It supports both real-time tracking (via WebSocket) and historical backfilling, with automatic deduplication and robust error handling.

## Key Features

- **IDL-Driven Decoding**: Provide an Anchor IDL, get decoded events automatically using `anchor_lang` utilities
- **Multi-Program Support**: Index events from multiple Anchor programs simultaneously
- **Real-Time Tracking**: WebSocket-based live event ingestion with exponential backoff auto-reconnect
- **Historical Backfill**: Efficiently backfill historical events using `get_signatures_for_address`
- **Flexible Storage**: SQLite, PostgreSQL (with JSONB), and MongoDB support
- **Event Deduplication**: Automatic duplicate detection and prevention
- **Production Ready**: Docker support, health checks, and comprehensive error handling
- **Type-Safe Decoding**: Uses `anchor_lang` borsh utilities for reliable deserialization

## Table of Contents

- [Tech Stack](#tech-stack)
- [Prerequisites](#prerequisites)
- [Getting Started](#getting-started)
- [Architecture](#architecture)
- [Configuration](#configuration)
- [Environment Variables](#environment-variables)
- [Available Commands](#available-commands)
- [Testing](#testing)
- [Deployment](#deployment)
- [Troubleshooting](#troubleshooting)
- [Additional Documentation](#additional-documentation)

## Tech Stack

- **Language**: Rust 1.70+
- **Blockchain SDK**: Solana Rust SDK (solana-client, solana-sdk)
- **Framework**: Anchor Lang 0.31.1
- **Serialization**: Borsh 1.0
- **Database**: SQLite, PostgreSQL (JSONB), MongoDB (Document-based)
- **Async Runtime**: Tokio 1.0
- **CLI**: Clap 4.x
- **Logging**: Tracing
- **Container**: Docker & Docker Compose

## Prerequisites

Before you begin, ensure you have the following installed:

- **Rust 1.70 or later**: Install via [rustup](https://rustup.rs/)

  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  source $HOME/.cargo/env
  ```

- **Database** (choose one):
  - **SQLite** (default, embedded):
    ```bash
    # macOS
    brew install sqlite
    # Ubuntu/Debian
    sudo apt-get install sqlite3 libsqlite3-dev
    # Arch
    sudo pacman -S sqlite
    ```
  - **PostgreSQL** 14+ (for production/high-concurrency):
    ```bash
    # Ubuntu/Debian
    sudo apt-get install postgresql postgresql-contrib
    # Or use Docker
    docker run -d --name postgres -e POSTGRES_PASSWORD=password -p 5432:5432 postgres:15
    ```
  - **MongoDB** 6+ (for document-based storage):
    ```bash
    # Or use Docker
    docker run -d --name mongodb -p 27017:27017 mongo:7
    ```

- **Docker** (optional, for containerized deployment):

  ```bash
  # Follow instructions at https://docs.docker.com/get-docker/
  ```

- **Solana CLI** (optional, for IDL extraction):
  ```bash
  sh -c "$(curl -sSfL https://release.solana.com/stable/install)"
  ```

## Getting Started

### 1. Clone the Repository

```bash
git clone https://github.com/your-org/soltrace.git
cd soltrace
```

### 2. Build the Project

```bash
# Build all workspace members in release mode
cargo build --release

# Or build in debug mode for development
cargo build
```

The compiled binaries will be available at:

- `target/release/soltrace-live` - Real-time event indexer
- `target/release/soltrace-backfill` - Historical event backfiller

### 3. Prepare IDL Files

Create an `idls/` directory and add your Anchor IDL files:

```bash
mkdir idls

# Option 1: Copy from Anchor workspace
cp target/idl/*.json idls/

# Option 2: Fetch from deployed program
anchor idl fetch <PROGRAM_ID> --provider-cluster mainnet > idls/my_program.json

# Option 3: Manually create IDL
# See docs/IDL_EXAMPLE.md for format
```

### 4. Configure Environment

Copy the example environment file:

```bash
cp .env.example .env
```

Edit `.env` with your configuration:

```env
# Solana RPC Configuration
SOLANA_RPC_URL=https://api.mainnet-beta.solana.com
SOLANA_WS_URL=wss://api.mainnet-beta.solana.com

# Program IDs to index (comma-separated)
PROGRAM_IDS=YourProgramId1,YourProgramId2

# Database Configuration
DB_URL=sqlite:./data/soltrace.db
IDL_DIR=./idls

# Indexer Configuration
COMMITMENT=confirmed
RECONNECT_DELAY=5

# Backfill Configuration
LIMIT=1000
BATCH_SIZE=100
BATCH_DELAY=100

# Logging
LOG_LEVEL=info
```

### 5. Initialize Database

```bash
./target/release/soltrace-live init --db-url sqlite:./soltrace.db
```

This creates the SQLite database with the events table.

### 6. Run Real-Time Indexer

Start indexing events in real-time:

```bash
./target/release/soltrace-live run \
  --programs "YourProgramId1,YourProgramId2" \
  --idl-dir ./idls \
  --db-url sqlite:./soltrace.db \
  --rpc-url https://api.mainnet-beta.solana.com \
  --ws-url wss://api.mainnet-beta.solana.com \
  --commitment confirmed
```

### 7. Backfill Historical Events

To index historical events:

```bash
./target/release/soltrace-backfill \
  --programs "YourProgramId1" \
  --limit 1000 \
  --idl-dir ./idls \
  --db-url sqlite:./soltrace.db \
  --batch-size 100 \
  --batch-delay 100
```

## Architecture

### Directory Structure

```
soltrace/
├── Cargo.toml                    # Workspace configuration
├── Cargo.lock                    # Dependency lock file
├── docker-compose.yml            # Docker Compose services
├── Dockerfile                    # Multi-stage Docker build
├── .env.example                  # Example environment variables
├── soltrace-core/               # Core library crate
│   └── src/
│       ├── lib.rs               # Library exports
│       ├── idl.rs               # IDL parsing and discriminator calculation
│       ├── idl_event.rs         # IDL-based event decoder using anchor_lang
│       ├── event.rs             # EventDecoder with IDL integration
│       ├── db.rs                # SQLite database operations
│       ├── types.rs             # Core types (DecodedEvent, RawEvent, etc.)
│       ├── utils.rs             # Utility functions (load_idls, extract_event_from_log)
│       ├── retry.rs             # Retry logic with exponential backoff
│       ├── validation.rs        # Input validation helpers
│       ├── metrics.rs           # Health check and metrics
│       └── error.rs             # Error types and handling
├── soltrace-live/               # Real-time indexer binary
│   └── src/
│       └── main.rs              # WebSocket subscription and event processing
└── soltrace-backfill/           # Historical backfill binary
    └── src/
        └── main.rs              # RPC-based historical event fetching
```

### Data Flow

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  Solana Network │     │  Soltrace Indexer │     │   SQLite DB     │
│                 │     │                  │     │                 │
│  ┌───────────┐  │     │  ┌─────────────┐  │     │  ┌───────────┐  │
│  │WebSocket  │──┼─────┼─▶│Log Extractor│  │     │  │  events   │  │
│  │  Logs     │  │     │  │             │  │     │  │   table   │  │
│  └───────────┘  │     │  └──────┬──────┘  │     │  └───────────┘  │
│                 │     │         │         │     │                 │
│  ┌───────────┐  │     │  ┌──────▼──────┐  │     │                 │
│  │  RPC API  │──┼─────┼─▶│IDL Decoder  │  │     │                 │
│  │           │  │     │  │(anchor_lang)│  │     │                 │
│  └───────────┘  │     │  └──────┬──────┘  │     │                 │
│                 │     │         │         │     │                 │
└─────────────────┘     │  ┌──────▼──────┐  │     └─────────────────┘
                        │  │  Database   │──┼────────────────────────▶
                        │  │   Insert    │  │
                        │  └─────────────┘  │
                        └───────────────────┘
```

### Key Components

**IDL Parser (`idl.rs`)**

- Loads and parses Anchor IDL JSON files
- Calculates event discriminators using `sha256("event:<name>")[..8]`
- Maps discriminators to event definitions

**Event Decoder (`idl_event.rs` + `event.rs`)**

- Uses `anchor_lang` utilities for type-safe borsh deserialization
- Supports all Anchor types: bool, u8-128, i8-128, string, Pubkey, bytes, Option<T>, Vec<T>, arrays
- Falls back to hex encoding on decoding failures

**Database (`db/`)**

- Trait-based abstraction supporting SQLite, PostgreSQL, and MongoDB
- Automatic backend selection based on URL scheme
- SQLite: JSON string storage with SQLx
- PostgreSQL: JSONB column for efficient querying
- MongoDB: Document-based with nested data structure
- Event deduplication via unique constraints

**Real-Time Indexer (`soltrace-live`)**

- WebSocket connection to Solana via `PubsubClient`
- Exponential backoff reconnection (capped at 15 minutes)
- Async log processing with bounded channel
- Supports multiple program subscriptions

**Historical Backfill (`soltrace-backfill`)**

- Uses `get_signatures_for_address` for historical data
- Concurrent transaction processing (configurable)
- Rate limit handling with retry
- Deduplication across programs

### Database Schema

Soltrace supports three database backends with the following schemas:

**SQLite** (Default)

```sql
CREATE TABLE events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    slot INTEGER NOT NULL,
    signature TEXT NOT NULL UNIQUE,
    program_id TEXT NOT NULL,
    event_name TEXT NOT NULL,
    discriminator TEXT NOT NULL,
    data TEXT NOT NULL,  -- JSON-encoded event data
    timestamp TEXT NOT NULL
);
```

**PostgreSQL** (with JSONB support)

```sql
CREATE TABLE events (
    id BIGSERIAL PRIMARY KEY,
    slot BIGINT NOT NULL,
    signature TEXT NOT NULL UNIQUE,
    program_id TEXT NOT NULL,
    event_name TEXT NOT NULL,
    discriminator TEXT NOT NULL,
    data JSONB NOT NULL,  -- Binary JSON for efficient querying
    timestamp TIMESTAMPTZ NOT NULL
);

-- GIN index for JSONB queries
CREATE INDEX idx_data_gin ON events USING GIN (data);
```

**MongoDB** (Document-based)

```javascript
// Collection: events
{
    _id: ObjectId,
    slot: NumberLong,
    signature: String,
    program_id: String,
    event_name: String,
    discriminator: String,
    data: {              // Nested document structure
        // Event-specific fields
    },
    timestamp: ISODate
}
```

## Configuration

### IDL File Format

Soltrace expects IDL files in the following format:

```json
{
  "version": "0.1.0",
  "name": "my_program",
  "address": "YourProgramId1111111111111111111111111111111",
  "events": [
    {
      "name": "Transfer",
      "fields": [
        {
          "name": "from",
          "type": "publicKey"
        },
        {
          "name": "to",
          "type": "publicKey"
        },
        {
          "name": "amount",
          "type": "u64"
        }
      ]
    }
  ]
}
```

Supported field types:

- `bool` - Boolean
- `u8`, `u16`, `u32` - Unsigned integers (JSON number)
- `u64`, `u128` - Large unsigned integers (JSON string)
- `i8`, `i16`, `i32` - Signed integers (JSON number)
- `i64`, `i128` - Large signed integers (JSON string)
- `string` - UTF-8 string
- `publicKey`, `pubkey`, `Pubkey` - Solana public key (32 bytes)
- `bytes` - Byte array (hex-encoded)
- `option<T>` - Optional value
- `vec<T>` - Vector of type T
- `[T; N]` - Fixed-size array

## Environment Variables

| Variable          | Description                                             | Default                               |
| ----------------- | ------------------------------------------------------- | ------------------------------------- |
| `SOLANA_RPC_URL`  | Solana HTTP RPC endpoint                                | `https://api.mainnet-beta.solana.com` |
| `SOLANA_WS_URL`   | Solana WebSocket endpoint                               | `wss://api.mainnet-beta.solana.com`   |
| `PROGRAM_IDS`     | Comma-separated program IDs to index                    | (required)                            |
| `DB_URL`          | Database connection string (sqlite:/postgres:/mongodb:) | `sqlite:./data/soltrace.db`           |
| `IDL_DIR`         | Directory containing IDL files                          | `./idls`                              |
| `COMMITMENT`      | Solana commitment level                                 | `confirmed`                           |
| `RECONNECT_DELAY` | WebSocket reconnect delay (seconds)                     | `5`                                   |
| `LIMIT`           | Number of signatures to backfill                        | `1000`                                |
| `BATCH_SIZE`      | Concurrent fetch batch size                             | `100`                                 |
| `BATCH_DELAY`     | Delay between batches (ms)                              | `100`                                 |
| `LOG_LEVEL`       | Logging verbosity                                       | `info`                                |

## Available Commands

### soltrace-live

```bash
# Initialize database
soltrace-live init --db-url <DATABASE_URL>

# Start real-time indexing
soltrace-live run \
  --programs <PROGRAM_IDS> \
  --ws-url <WS_URL> \
  --rpc-url <RPC_URL> \
  --db-url <DB_URL> \
  --idl-dir <IDL_DIR> \
  --commitment <confirmed|processed|finalized> \
  --reconnect-delay <SECONDS> \
  --max-reconnects <COUNT>
```

### soltrace-backfill

```bash
soltrace-backfill \
  --programs <PROGRAM_IDS> \
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

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific package tests
cargo test --package soltrace-core

# Run tests in release mode
cargo test --release
```

### Test Coverage

The project includes unit tests for:

- IDL parsing and discriminator calculation
- Event decoding (all supported types)
- Database operations
- Retry logic
- Validation functions
- Utility functions

### Code Quality

```bash
# Format code
cargo fmt

# Run linter
cargo clippy

# Run linter with all features
cargo clippy --all-features

# Check for security vulnerabilities
cargo audit
```

## Deployment

### Docker (Recommended)

Build and run with Docker Compose:

```bash
# Build images
docker-compose build

# Start services
docker-compose up -d

# View logs
docker-compose logs -f soltrace-live

# Stop services
docker-compose down

# Remove volumes (WARNING: deletes database)
docker-compose down -v
```

### Docker Configuration

The `docker-compose.yml` includes two services:

1. **soltrace-live**: Real-time indexer with auto-restart
2. **soltrace-backfill**: One-time historical backfill (runs after live starts)

Both services share:

- Named volume for database persistence
- IDL directory mount (read-only)
- Environment variables from `.env`

### Manual Deployment

```bash
# Build release binary
cargo build --release

# Copy binary to server
scp target/release/soltrace-live user@server:/opt/soltrace/

# Run with systemd (example service file)
cat > /etc/systemd/system/soltrace.service << 'EOF'
[Unit]
Description=Soltrace Live Indexer
After=network.target

[Service]
Type=simple
User=soltrace
WorkingDirectory=/opt/soltrace
Environment=DB_URL=sqlite:./data/soltrace.db
Environment=PROGRAM_IDS=YourProgramId
ExecStart=/opt/soltrace/soltrace-live run --programs ${PROGRAM_IDS} --db-url ${DB_URL}
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

# Enable and start
systemctl enable soltrace
systemctl start soltrace
```

### Kubernetes

Example deployment manifest:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: soltrace-live
spec:
  replicas: 1
  selector:
    matchLabels:
      app: soltrace-live
  template:
    metadata:
      labels:
        app: soltrace-live
    spec:
      containers:
        - name: soltrace
          image: your-registry/soltrace:latest
          env:
            - name: PROGRAM_IDS
              value: "YourProgramId"
            - name: DB_URL
              value: "sqlite:/data/soltrace.db"
          volumeMounts:
            - name: data
              mountPath: /data
            - name: idls
              mountPath: /idls
      volumes:
        - name: data
          persistentVolumeClaim:
            claimName: soltrace-data
        - name: idls
          configMap:
            name: soltrace-idls
```

## Troubleshooting

### WebSocket Connection Issues

**Error**: `Failed to connect to WebSocket`

**Solutions**:

1. Verify WebSocket URL uses `wss://` for secure connections
2. Check firewall rules allow outbound WebSocket connections
3. Try alternate RPC endpoints (Helius, QuickNode, etc.)
4. Increase `--reconnect-delay` for unstable connections

### Events Not Decoding

**Error**: `No event found with discriminator` or `Failed to decode event`

**Solutions**:

1. Verify IDL file matches the deployed program version
2. Check program ID in IDL matches actual on-chain program
3. Ensure IDL events array includes all event definitions
4. Check logs for discriminator mismatches
5. Verify field types in IDL match program implementation

### RPC Rate Limiting

**Error**: `429 Too Many Requests`

**Solutions**:

1. Increase `--batch-delay` in backfill mode
2. Reduce `--batch-size` and `--concurrency`
3. Use dedicated RPC provider (Helius, QuickNode, Alchemy)
4. Enable retry logic with `--max-retries 5`

### Database Issues

**Error**: `UNIQUE constraint failed` (SQLite/PostgreSQL)

**This is normal** - indicates duplicate event detection working correctly. Events are only inserted once.

**Error**: `database is locked` (SQLite)

**Solutions**:

1. Use PostgreSQL or MongoDB for high-concurrency scenarios
2. Increase SQLite busy timeout
3. Reduce concurrency settings

**Error**: Connection refused (PostgreSQL/MongoDB)

**Solutions**:

1. Verify database server is running and accessible
2. Check connection URL format
3. Verify credentials and permissions
4. Check firewall rules

### Build Errors

**Error**: `linker cc not found`

**Solutions**:

```bash
# Ubuntu/Debian
sudo apt-get install build-essential

# macOS
xcode-select --install

# Arch
sudo pacman -S base-devel
```

**Error**: `cannot find -lsqlite3`

**Solutions**:

```bash
# Ubuntu/Debian
sudo apt-get install libsqlite3-dev

# macOS
brew install sqlite
```

## Querying Events

### SQLite

```bash
# All events from a program
sqlite3 soltrace.db "SELECT * FROM events WHERE program_id = 'YourProgramId' LIMIT 10;"

# Events by type
sqlite3 soltrace.db "SELECT * FROM events WHERE event_name = 'Transfer';"

# Events in slot range
sqlite3 soltrace.db "SELECT * FROM events WHERE slot BETWEEN 123456 AND 123500;"

# Event count by program
sqlite3 soltrace.db "SELECT program_id, event_name, COUNT(*) FROM events GROUP BY program_id, event_name;"

# Recent events
sqlite3 soltrace.db "SELECT * FROM events ORDER BY timestamp DESC LIMIT 100;"
```

### PostgreSQL

```sql
-- All events from a program
SELECT * FROM events WHERE program_id = 'YourProgramId' LIMIT 10;

-- Query JSONB data (e.g., events where amount > 1000)
SELECT * FROM events
WHERE event_name = 'Transfer'
  AND data->>'amount' > '1000';

-- Query nested JSONB fields
SELECT * FROM events
WHERE data->>'from' = 'SenderPubkey';

-- Events in slot range
SELECT * FROM events WHERE slot BETWEEN 123456 AND 123500;

-- Event count by program
SELECT program_id, event_name, COUNT(*)
FROM events
GROUP BY program_id, event_name;
```

### MongoDB

```javascript
// All events from a program
db.events.find({ program_id: "YourProgramId" }).limit(10);

// Events by type
db.events.find({ event_name: "Transfer" });

// Query nested data (e.g., amount > 1000)
db.events.find({
  event_name: "Transfer",
  "data.amount": { $gt: 1000 },
});

// Events in slot range
db.events.find({
  slot: { $gte: 123456, $lte: 123500 },
});

// Event count by program
db.events.aggregate([{ $group: { _id: "$program_id", count: { $sum: 1 } } }]);
```

## Additional Documentation

- [Docker Setup Guide](docs/DOCKER_SETUP.md) - Detailed Docker deployment instructions
- [Docker Operations](docs/DOCKER.md) - Docker commands and troubleshooting
- [Technical Documentation](docs/TECHNICAL.md) - Architecture and implementation details
- [Live Refactor Guide](docs/LIVE_REFACTOR.md) - WebSocket implementation details
- [Backfill Refactor Guide](docs/BACKFILL_REFACTOR.md) - Historical indexing implementation
- [IDL Example](docs/IDL_EXAMPLE.md) - Example IDL format
- [PubsubClient Guide](docs/PUBSUB_CLIENT.md) - Solana PubsubClient API usage

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Built with [Anchor](https://www.anchor-lang.com/)
- Uses [Solana Rust SDK](https://github.com/solana-labs/solana)
- Database powered by [SQLx](https://github.com/launchbadge/sqlx)
- Async runtime by [Tokio](https://tokio.rs/)

---

Made with ❤️ for the Solana ecosystem
