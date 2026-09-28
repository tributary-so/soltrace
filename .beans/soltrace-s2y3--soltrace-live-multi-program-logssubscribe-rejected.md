---
# soltrace-s2y3
title: 'soltrace-live: multi-program logsSubscribe rejected by RPC'
status: completed
type: bug
priority: normal
created_at: 2026-09-28T12:47:12Z
updated_at: 2026-09-28T12:51:44Z
---

Single logsSubscribe with Mentions([N pubkeys]) fails on providers that enforce one address per subscription (rpcpool/Triton: -32602 'Only 1 address supported'). Fix: one subscription per program on the same PubsubClient connection, merged via select_all; per-signature live dedup so a tx mentioning multiple monitored programs is fetched once.


## Summary of Changes

- `soltrace-live/src/main.rs` `websocket_handler`: replaced the single multi-address `logsSubscribe` (`Mentions([N pubkeys])`) with one subscription per program on the same `PubsubClient` connection, merged via `futures::stream::select_all`. All `UnsubscribeFn` handles collected and awaited on cleanup. Fixes `-32602 Invalid Request: Only 1 address supported` on providers (rpcpool/Triton) that enforce one address per subscription; the JSON-RPC pubsub spec itself only honors the first entry of a multi-address mentions array.
- Added `SeenSignatures` (in-session, capped 4096, ring-evict) in the processor task: a tx mentioning N monitored programs now fires N identical notifications; duplicates are dropped before the `get_transaction` fetch. DB `ON CONFLICT DO NOTHING` remains the persistent dedup.
- Tests: `test_seen_signatures_dedups_within_window`, `test_seen_signatures_evicts_oldest_at_cap`.

Verification: `cargo fmt --all -- --check` clean; 0 clippy findings in soltrace-live; 88 workspace tests pass (21 soltrace-live); 30s live smoke run against mainnet rpcpool endpoint with 6 programs logs `Successfully subscribed to logs for 6 program(s)` and no errors.
