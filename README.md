# Soltrace

A flexible, IDL-driven Solana event indexer built in Rust.

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Overview

Soltrace indexes Anchor events from Solana programs using the program's IDL for decoding. It supports multiple programs and handles both real-time tracking (via WebSocket) and historical backfilling.

## Features

- ✅ **IDL-driven decoding**: Provide an IDL, get decoded events automatically
- ✅ **Multi-program support**: Index events from multiple Anchor programs
- ✅ **Real-time tracking**: WebSocket-based live event ingestion with auto-reconnect
- ✅ **Historical backfill**: Backfill historical events using `get_signatures_for_address`
- ✅ **Flexible storage**: SQLite (current) with PostgreSQL compatibility
- ✅ **Event deduplication**: Automatic duplicate event detection
- ✅ **Shared utilities**: Common code between live and backfill applications
- ✅ **Connection health**: Automatic reconnection and subscription renewal

## Quick Start

### Prerequisites

- Rust 1.70 or later
- Solana CLI (optional, for IDL extraction)
- Docker and Docker Compose (optional, for containerized deployment)
- SQLite (for local development) or PostgreSQL

### Installation

```bash
# Clone the repository
git clone https://github.com/your-org/soltrace.git
cd soltrace

# Build project (choose one)
cargo build --release
# OR build with Docker
docker-compose build

# The binaries will be in target/release/
# - soltrace-live: Real-time event tracker
# - soltrace-backfill: Historical event backfiller
```

### Docker Quick Start

```bash
# Configure environment
cp .env.example .env
nano .env  # Set PROGRAM_IDS

# Start with Docker Compose
docker-compose up -d soltrace-live

# View logs
docker-compose logs -f soltrace-live
```

See [DOCKER.md](DOCKER.md) for complete Docker deployment guide.

### 1. Add IDLs

Place your Anchor IDL files in the `idls/` directory:

```bash
mkdir idls

# Option 1: From Anchor workspace
cp target/idl/my_program.json idls/

# Option 2: Download from program
anchor idl fetch <PROGRAM_ID> --provider-cluster mainnet > idls/my_program.json
```

See [IDL_EXAMPLE.md](IDL_EXAMPLE.md) for an example IDL format.

### 2. Initialize Database

```bash
./target/release/soltrace-live init
```

This creates `soltrace.db` in the current directory.

### 3. Real-time Tracking

```bash
./target/release/soltrace-live run \
  --programs "ProgramID1,ProgramID2" \
  --idl-dir ./idls \
  --db-url sqlite:./soltrace.db \
  --rpc-url https://api.mainnet-beta.solana.com \
  --commitment confirmed
```

### 4. Backfill Historical Events

```bash
./target/release/soltrace-backfill \
  --programs "ProgramID1" \
  --limit 1000 \
  --idl-dir ./idls \
  --batch-size 100 \
  --batch-delay 100
```

**Note:** The backfiller uses `get_signatures_for_address` to fetch the latest N transactions that call your program. See [BACKFILL_REFACTOR.md](BACKFILL_REFACTOR.md) for details.

## Configuration

You can also use environment variables or a `.env` file:

```env
SOLANA_RPC_URL=https://api.mainnet-beta.solana.com
SOLANA_WS_URL=wss://api.mainnet-beta.solana.com
DB_URL=sqlite:./soltrace.db
IDL_DIR=./idls
```

## Architecture

```
soltrace/
├── soltrace-core/       # Shared library
│   ├── idl.rs           # IDL parsing
│   ├── event.rs         # Event decoding
│   ├── db.rs            # Database operations
│   └── types.rs         # Shared types
├── soltrace-live/       # Real-time tracker
│   └── main.rs          # WebSocket subscription
└── soltrace-backfill/   # Historical backfiller
    └── main.rs          # RPC polling
```

## Usage Examples

### Index a Single Program

```bash
soltrace-live run \
  --programs "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA" \
  --idl-dir ./idls
```

### Index Multiple Programs

```bash
soltrace-live run \
  --programs "Prog1...,Prog2...,Prog3..." \
  --idl-dir ./idls
```

### Backfill Latest Transactions

```bash
# Backfill last 1000 transactions calling your program
soltrace-backfill \
  --programs "YourProgramID" \
  --limit 1000 \
  --idl-dir ./idls
```

### Backfill Multiple Programs

```bash
soltrace-backfill \
  --programs "Prog1...,Prog2...,Prog3..." \
  --limit 500 \
  --batch-size 50
```

### Query Events

You can query the SQLite database directly:

```bash
# All events from a program
sqlite3 soltrace.db "SELECT * FROM events WHERE program_id = 'YourProgramID' LIMIT 10;"

# Events by type
sqlite3 soltrace.db "SELECT * FROM events WHERE event_name = 'Transfer';"

# Events in slot range
sqlite3 soltrace.db "SELECT * FROM events WHERE slot BETWEEN 123456 AND 123500;"
```

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
    created_at TEXT NOT NULL
);
```

## Migration to PostgreSQL/TimescaleDB

To migrate from SQLite to PostgreSQL:

1. Install PostgreSQL + TimescaleDB
2. Update connection string:
   ```bash
   --db-url "postgresql://user:pass@localhost/soltrace"
   ```
3. Update `soltrace-core/src/db.rs` to use `sqlx::postgres::PgPool`
4. Adjust SQL syntax for PostgreSQL
5. Create hypertable for time-series optimization:
   ```sql
   SELECT create_hypertable('events', 'timestamp');
   ```

See [TECHNICAL.md](TECHNICAL.md) for detailed migration guide.

## Development

### Running Tests

```bash
cargo test
```

### Building Documentation

```bash
cargo doc --open
```

### Code Style

This project uses standard Rust formatting:

```bash
cargo fmt
cargo clippy
```

## Troubleshooting

### WebSocket Connection Failed

- Ensure you're using a valid WebSocket URL (`wss://` for secure)
- Check firewall rules
- Try using a different RPC endpoint

### Events Not Decoding

- Verify IDL file matches the program version
- Check that the program ID in the IDL matches the actual program
- Look for discriminator calculation errors in logs

### RPC Rate Limiting

- Increase `--batch-delay` in backfill mode
- Reduce `--batch-size` to make smaller requests
- Consider using a dedicated RPC provider

## Roadmap

- [ ] Full type-aware event decoding
- [ ] GraphQL API for querying events
- [ ] Web UI for event browsing
- [ ] Event filters and subscriptions
- [ ] Reorg handling and rollback support
- [ ] Prometheus metrics
- [ ] Docker support
- [ ] Continuous aggregates for analytics

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Built with [Anchor](https://www.anchor-lang.com/)
- Uses [Solana Rust SDK](https://github.com/solana-labs/solana)
- Database powered by [SQLx](https://github.com/launchbadge/sqlx)

## Support

### Documentation

- 📖 [Technical Documentation](TECHNICAL.md) - Architecture and database details
- 🔄 [Live Refactor Guide](LIVE_REFACTOR.md) - WebSocket implementation and auto-reconnect
- 🚀 [PubsubClient Guide](PUBSUB_CLIENT.md) - Official PubsubClient API usage
- 📊 [Backfill Refactor Guide](BACKFILL_REFACTOR.md) - RPC-based historical indexing
- 🐳 [Docker Guide](DOCKER.md) - **Docker deployment and Docker Compose**
- 🐳 [Docker Migration](PUBSUB_MIGRATION.md) - PubsubClient migration summary
- 📝 [IDL Examples](IDL_EXAMPLE.md) - Example IDL format for testing

### Getting Help

- 🐛 [Issue Tracker](https://github.com/your-org/soltrace/issues)
- 💬 [Discord](https://discord.gg/your-discord)

---

Made with ❤️ for the Solana ecosystem
