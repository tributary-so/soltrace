# Soltrace Backfiller - Refactored Approach

## Architecture Changes

The backfiller has been refactored to use a more efficient approach for historical event retrieval.

### Previous Approach
- Iterate through blocks in slot range
- Fetch all transactions from each block
- Filter transactions by program involvement
- High overhead, many unnecessary requests

### New Approach ✅
- Use `get_signatures_for_address` RPC method
- Get N recent signatures that call specific program
- Fetch transactions only for relevant signatures
- Much more targeted and efficient

## RPC Methods Used

### 1. `get_signatures_for_address`

Returns signatures of transactions that involve a specific address (program ID).

```rust
let config = RpcSignaturesForAddressConfig {
    before: None,
    until: None,
    limit: Some(1000),  // Get last 1000 signatures
    commitment: Some(CommitmentConfig::confirmed()),
};

let signatures = rpc_client.get_signatures_for_address_with_config(
    &program_id,
    config
)?;
```

**Parameters:**
- `address`: Program ID to search for
- `limit`: Number of signatures to return (max 1000 for this MVP)
- `before`: Optional signature to paginate backwards
- `until`: Optional signature to paginate forwards
- `commitment`: Confirmation level (processed/confirmed/finalized)

**Returns:**
```rust
Vec<RpcConfirmedTransactionStatusWithSignature>
```

Each contains:
- `signature`: Transaction signature
- `slot`: Slot number
- `err`: Optional error (null if successful)
- `confirmation_status`: Confirmation status
- `memo`: Optional memo field

### 2. `get_transaction`

Fetches full transaction details including logs.

```rust
let transaction = rpc_client.get_transaction_with_config(
    &signature,
    RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::Json),
        commitment: Some(CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(0),
    },
)?;
```

**Parameters:**
- `signature`: Transaction signature
- `encoding`: Transaction encoding (Json, Base64, etc.)
- `commitment`: Confirmation level

**Returns:**
- Full transaction with metadata including logs
- Logs contain Anchor program event data

## Event Extraction Flow

### Log Format
Anchor programs emit event data in transaction logs:

```
Program <program_id> invoke [1]
Program log: Instruction: Transfer
Program <program_id> consumed 1234 of 200000 compute units
Program <program_id> success
Program data: eyJldmVudCI6IlRyYW5zZmVyIiwiZGF0YSI6eyJmcm9tIjoiLi4uIiwidG8iOiIuLi4iLCJhbW91bnQiOjEwMH19
```

### Event Data Extraction
The `Program data:` line contains base64-encoded event data:

```rust
fn extract_event_from_log(log: &str, program_id_str: &str) -> Option<Vec<u8>> {
    if log.starts_with("Program data:") {
        let data_str = log.strip_prefix("Program data: ")?.trim();
        if let Ok(data) = base64::decode(data_str) {
            if log.contains(program_id_str) {
                return Some(data);
            }
        }
    }
    None
}
```

### Decoding Pipeline

1. **Extract raw bytes** from log
2. **Parse discriminator** (first 8 bytes)
3. **Lookup event** in IDL by discriminator
4. **Decode event data** using borsh
5. **Store in database**

## CLI Usage

### Backfill Latest 1000 Events

```bash
cargo run --bin soltrace-backfill \
  --programs "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA" \
  --limit 1000 \
  --batch-size 100 \
  --batch-delay 100
```

### Backfill Multiple Programs

```bash
cargo run --bin soltrace-backfill \
  --programs "Prog1...,Prog2...,Prog3..." \
  --limit 500 \
  --batch-size 50
```

### Parameters

| Parameter | Default | Description |
|-----------|----------|-------------|
| `--rpc-url` | `https://api.mainnet-beta.solana.com` | Solana RPC endpoint |
| `--programs` | Required | Comma-separated program IDs |
| `--db-url` | `sqlite:./soltrace.db` | Database connection string |
| `--idl-dir` | `./idls` | IDL files directory |
| `--limit` | `1000` | Number of signatures to fetch per program |
| `--batch-size` | `100` | Transactions per batch |
| `--batch-delay` | `100` | Delay between batches (ms) |

## Processing Flow

```
┌─────────────────────────────────────────────────────────────┐
│ 1. Parse Program IDs                                    │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│ 2. For Each Program:                                     │
│    - Validate program ID                                   │
│    - Call get_signatures_for_address(limit)                  │
│    - Get list of signatures                               │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│ 3. Process Signatures in Batches:                        │
│    - Batch of 100 signatures                              │
│    - For each signature:                                  │
│      * Skip if already processed                           │
│      * Fetch transaction (get_transaction)                  │
│      * Extract logs                                       │
│      * Look for "Program data:" lines                     │
│      * Decode event via IDL                               │
│      * Store in database (deduplicate)                     │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│ 4. Report Statistics                                     │
│    - Total signatures fetched                               │
│    - Total events processed                                │
│    - Per-program breakdown                                 │
└─────────────────────────────────────────────────────────────┘
```

## Optimization Opportunities

### Current Limitations

1. **Individual Transaction Fetching**: Currently fetches transactions one-by-one
   - Solana doesn't have a bulk `get_transactions` method
   - Each request adds latency

2. **No Pagination**: Fixed limit of 1000 signatures
   - For programs with >1000 historical transactions, need pagination
   - Would use `before` parameter for pagination

3. **Duplicate Detection**: Checks database for each event
   - Could cache processed signatures in memory for faster checks

### Future Improvements

#### 1. Parallel Transaction Fetching
```rust
use futures::stream::{self, StreamExt};

let transaction_futures = signatures.iter()
    .map(|sig| fetch_transaction(rpc_client, sig))
    .collect::<Vec<_>>();

let transactions = stream::iter(transaction_futures)
    .buffer_unordered(10)  // Fetch 10 in parallel
    .collect::<Vec<_>>()
    .await;
```

#### 2. Pagination for Historical Backfill
```rust
let mut last_signature: Option<Signature> = None;
let mut total_fetched = 0;
let target = 10000;  // Backfill 10k events

while total_fetched < target {
    let config = RpcSignaturesForAddressConfig {
        before: last_signature,
        limit: Some(1000),
        commitment: Some(CommitmentConfig::confirmed()),
    };

    let signatures = rpc_client.get_signatures_for_address_with_config(
        &program_id,
        config
    )?;

    last_signature = signatures.last().map(|s| s.signature.parse().unwrap());
    total_fetched += signatures.len();
}
```

#### 3. Bulk Insert to Database
```rust
async fn insert_events_batch(db: &Database, events: Vec<EventRecord>) -> Result<()> {
    let mut tx = db.begin().await?;

    for event in events {
        sqlx::query("INSERT INTO events ...")
            .bind(...)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(())
}
```

## Error Handling

### Common Errors and Solutions

| Error | Cause | Solution |
|-------|-------|----------|
| `Invalid program ID` | Malformed address | Verify base58 encoding |
| `Failed to fetch account` | Program doesn't exist | Check program ID |
| `Rate limited` | Too many RPC requests | Increase `--batch-delay` |
| `Transaction not found` | Old transaction pruned | Use confirmed commitment |

## Performance Metrics

Expected performance (depending on RPC provider):

| Metric | Value |
|--------|-------|
| Signatures per RPC call | Up to 1000 |
| Transactions per batch | 100 (configurable) |
| Average transaction fetch time | 100-300ms |
| Events per signature | 0-10 (varies) |
| Total time for 1000 events | ~5-15 minutes |

## Database Schema

Events are stored with the following structure:

```sql
CREATE TABLE events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    slot INTEGER NOT NULL,
    signature TEXT NOT NULL,
    program_id TEXT NOT NULL,
    event_name TEXT NOT NULL,
    discriminator TEXT NOT NULL,  -- Hex-encoded 8 bytes
    data TEXT NOT NULL,            -- JSON-encoded event data
    timestamp TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('utc'))
);

-- Indexes for fast queries
CREATE INDEX idx_signature ON events(signature);
CREATE INDEX idx_program_id ON events(program_id);
CREATE INDEX idx_slot ON events(slot);
```

## Example Output

```
Starting Soltrace Backfill
RPC URL: https://api.mainnet-beta.solana.com
Fetching latest 1000 signatures per program
Batch size: 100
Indexing 1 program(s):
  - TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA
Database connected: sqlite:./soltrace.db
Loaded 1 IDL(s) from ./idls
  - TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA: 3 events

Processing program: TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA
Fetching signatures for program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA...
Found 1000 signatures
Processing batch 1 to 100 of 1000 signatures
  Processed 15 events
Processing batch 101 to 200 of 1000 signatures
  Processed 12 events
...
Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA complete: 145 events processed

Backfill complete!
Total signatures fetched: 1000
Total events processed: 145
Unique signatures processed: 145
```

## Migration Notes

If you were using the old slot-range based backfiller, here's how to migrate:

### Old Command
```bash
soltrace-backfill --start-slot 123456 --end-slot 123500 --programs "Prog..."
```

### New Command
```bash
soltrace-backfill --limit 1000 --programs "Prog..."
```

The new approach is simpler and more efficient for most use cases.
