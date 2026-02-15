# Soltrace Live - WebSocket Refactor

## Overview

The live indexer has been completely refactored to use WebSocket log subscriptions instead of polling slots. This provides real-time event processing with automatic reconnection handling.

## Key Changes

### Before Refactor
- Used RPC slot polling
- Manually checked blocks for program activity
- High latency and overhead
- No automatic reconnection

### After Refactor ✅
- WebSocket log subscription
- Real-time event notifications
- Low latency processing
- Automatic reconnection with backoff
- Shared utilities with backfiller

## WebSocket Log Subscription

### Subscription Format

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "logsSubscribe",
  "params": [
    {"mentions": ["ProgramId1", "ProgramId2"]},
    {"commitment": "confirmed"}
  ]
}
```

### Parameters

| Parameter | Description | Example |
|-----------|-------------|---------|
| `mentions` | Array of program IDs to monitor | `["TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"]` |
| `commitment` | Confirmation level | `"confirmed"` (recommended), `"finalized"`, `"processed"` |

### Notification Format

When an event occurs, you receive:

```json
{
  "jsonrpc": "2.0",
  "method": "logsNotification",
  "params": {
    "result": {
      "err": null,
      "logs": [
        "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [1]",
        "Program log: Instruction: Transfer",
        "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA consumed 1234 of 200000 compute units",
        "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success",
        "Program data: eyJldmVudCI6IlRyYW5zZmVyIn0="
      ],
      "signature": "5K7dyntZ3dJ1a1Vh...<full_signature>",
      "slot": 123456789,
      "blockTime": 1234567890
    },
    "subscription": 12345
  }
}
```

## Connection Management

### Auto-Reconnection Loop

```rust
let mut reconnect_count = 0;
let reconnect_delay = 5; // seconds

loop {
    match websocket_handler(ws_url, program_ids, ...).await {
        Ok(_) => {
            info!("Connection closed normally");
            break;
        }
        Err(e) => {
            error!("WebSocket error: {}", e);
            reconnect_count += 1;
            info!("Reconnecting in {} seconds...", reconnect_delay);
            sleep(Duration::from_secs(reconnect_delay)).await;
        }
    }
}
```

### Reconnection Strategy

1. **Immediate Reconnect**: If connection drops, attempt to reconnect after delay
2. **Exponential Backoff**: In production, could increase delay on repeated failures
3. **Subscription Renewal**: Automatically re-subscribe to all programs on reconnect
4. **Ping/Pong**: Respond to server pings to keep connection alive

## Shared Utilities

Code shared between `soltrace-live` and `soltrace-backfill`:

### `soltrace-core/src/utils.rs`

#### `load_idls(idl_parser, idl_dir)`
Loads all IDL JSON files from a directory.

**Used by:** Both live and backfiller

```rust
load_idls(&mut idl_parser, "./idls").await?;
```

#### `process_transaction(transaction, program_id, event_decoder, db)`
Processes a single transaction, extracts events, decodes, and stores in database.

**Used by:** Both live and backfiller

```rust
let processed = process_transaction(transaction, program_id, &event_decoder, &db)?;
```

**Returns:** List of signatures that were successfully processed

#### `extract_event_from_log(log, program_id)`
Extracts base64-encoded event data from log line.

**Used by:** Both live and backfiller

```rust
if let Some(data) = extract_event_from_log(log_line, program_id) {
    // Decode and process event
}
```

## Event Processing Flow

### Live Indexer

```
┌─────────────────────────────────────────────────────────────┐
│ 1. Connect to WebSocket                                   │
│    - Send logsSubscribe for each program ID                  │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│ 2. Receive Notification                                   │
│    - logsNotification with transaction data                     │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│ 3. Process Notification                                   │
│    - Extract logs                                          │
│    - For each log:                                         │
│      * Check if it's an event for our program               │
│      * Extract base64 data                                  │
│      * Decode via IDL                                      │
│      * Store in database (deduplicate)                       │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│ 4. Continue Listening                                    │
│    - Wait for next notification                            │
│    - Auto-reconnect on errors                              │
└─────────────────────────────────────────────────────────────┘
```

### Backfiller

```
┌─────────────────────────────────────────────────────────────┐
│ 1. Get Signatures                                      │
│    - get_signatures_for_address(limit)                     │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│ 2. Process Signatures in Batches                        │
│    - For each signature:                                   │
│      * Fetch transaction (get_transaction)                   │
│      * Use process_transaction() shared function            │
│      * Store in database                                  │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│ 3. Report Statistics                                    │
│    - Total signatures fetched                               │
│    - Total events processed                                │
└─────────────────────────────────────────────────────────────┘
```

## CLI Usage

### Initialize Database

```bash
cargo run --bin soltrace-live -- init
```

### Start Live Indexing

```bash
cargo run --bin soltrace-live -- run \
  --ws-url wss://api.mainnet-beta.solana.com \
  --rpc-url https://api.mainnet-beta.solana.com \
  --programs "ProgramId1,ProgramId2" \
  --commitment confirmed \
  --reconnect-delay 5
```

### Parameters

| Parameter | Default | Description |
|-----------|----------|-------------|
| `--ws-url` | `wss://api.mainnet-beta.solana.com` | WebSocket endpoint |
| `--rpc-url` | `https://api.mainnet-beta.solana.com` | HTTP RPC for validation |
| `--programs` | Required | Comma-separated program IDs |
| `--db-url` | `sqlite:./soltrace.db` | Database connection |
| `--idl-dir` | `./idls` | IDL files directory |
| `--commitment` | `confirmed` | Confirmation level |
| `--reconnect-delay` | `5` | Seconds before reconnect attempt |

## Error Handling

### Connection Errors

- **Connection Failed**: Auto-reconnect after delay
- **Subscription Failed**: Log error and attempt to reconnect
- **Message Parse Error**: Log and continue (non-critical)

### Transaction Processing Errors

- **Failed Transaction**: Skip (checked via `err` field)
- **Duplicate Event**: Skip silently (UNIQUE constraint)
- **Decode Error**: Log and continue (event may be for different program)

### Reconnection Scenarios

| Scenario | Behavior |
|----------|----------|
| Server disconnect | Reconnect after delay |
| Network timeout | Reconnect after delay |
| Subscription failure | Attempt to re-subscribe |
| Ping timeout | Send pong response |

## Monitoring

### Logs Output

```
Connecting to WebSocket (attempt 1)...
Subscribing to logs for program: TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA
Successfully subscribed to 1 program(s)
Waiting for events...
Stored event: Transfer from 5K7dyntZ3...
Processed 100 messages, 15 events
Stored event: Mint from 8H3jkwL2...
Processed 200 messages, 32 events
```

### Health Checks

The indexer maintains connection health by:

1. Responding to server pings
2. Monitoring message flow
3. Detecting connection drops
4. Automatic reconnection

## Performance

### Expected Metrics

| Metric | Value |
|--------|-------|
| Connection establishment | 1-2 seconds |
| Subscription latency | < 100ms |
| Event notification latency | < 500ms (network dependent) |
| Processing per event | < 50ms |
| Memory usage | ~50-100MB (depends on IDL size) |

### Optimization Tips

1. **Use `confirmed` commitment**: Faster than `finalized`, sufficient for most use cases
2. **Filter at subscription**: Use `mentions` to only get relevant logs
3. **Batch database writes**: Consider batching inserts for high-volume programs
4. **Monitor connection logs**: Set up alerts for frequent reconnections

## Comparison: Live vs Backfill

| Aspect | Live (WebSocket) | Backfill (RPC) |
|---------|-------------------|-----------------|
| Latency | Real-time (< 1s) | Bulk (hours) |
| Data source | Push-based (server notifies) | Pull-based (client fetches) |
| Connection | Persistent WebSocket | HTTP requests |
| Use case | Monitor new events | Historical data |
| Resource usage | Low (no polling) | Higher (many RPC calls) |

## Troubleshooting

### "WebSocket error: Connection closed"

- Check firewall settings
- Verify WebSocket URL is correct
- Check RPC provider status
- Increase reconnect delay

### "No IDLs loaded"

- Ensure `idls/` directory exists
- Verify IDL files are valid JSON
- Check file permissions

### "Failed to decode event"

- IDL may be outdated
- Event may be from different program version
- Check discriminator calculation

### Frequent Reconnections

- Network instability
- RPC provider issues
- Consider switching RPC provider

## Production Considerations

### High Availability

1. **Multiple RPC Endpoints**: Configure backup WebSocket URLs
2. **Load Balancing**: Distribute subscriptions across endpoints
3. **Heartbeat Monitoring**: External health checks
4. **Metrics Collection**: Track connection stability, event rates

### Scaling

1. **Horizontal Scaling**: Run multiple instances with program subsets
2. **Event Partitioning**: Split by program or time range
3. **Database Optimization**: Use PostgreSQL + TimescaleDB
4. **Caching**: Cache frequently accessed events

### Monitoring

1. **Prometheus Metrics**: Export connection status, event rates, errors
2. **Log Aggregation**: Centralized logging (ELK, Loki)
3. **Alerting**: Notify on connection failures, low event rates
4. **Dashboard**: Real-time visualization (Grafana)

## Future Enhancements

- [ ] Parallel event processing
- [ ] Event filtering (specific event types only)
- [ ] GraphQL API for real-time queries
- [ ] Webhook notifications
- [ ] Metrics endpoint
- [ ] Health check endpoint
- [ ] Config file support
- [ ] Program-specific settings (different commitments per program)
