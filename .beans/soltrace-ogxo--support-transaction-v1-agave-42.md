---
# soltrace-ogxo
title: Support transaction v1 (Agave 4.2)
status: completed
type: task
priority: normal
created_at: 2026-08-22T00:24:52Z
updated_at: 2026-08-22T00:36:36Z
---

Agave 4.2 activates transaction v1 (SIMD-0296/0385). getTransaction calls declaring maxSupportedTransactionVersion 0 will fail outright for v1 transactions once the feature gate activates, dropping ALL events for those signatures (fetch is hoisted above both emit!/emit_cpi! loops in live). Fix: add v1 JSON fixture regression test to utils.rs, flip maxSupportedTransactionVersion to 1 at 3 Rust sites + scripts/fix_slots.py, update the comments documenting version 0. Decode path verified tolerant: TransactionVersion is untagged Number(u8), serde ignores unknown fields.

## Summary of Changes

Analysis mapped all 8 items of the Agave 4.2 breaking-changes checklist against soltrace's touchpoints: only Transaction v1 applies (no gRPC/Yellowstone, no rewards parsing, no jsonParsed, no account-update correlation, no hardcoded slot duration).

- [x] Analysis: exposure map + SDK decode-path verification (TransactionVersion is untagged `Number(u8)`; serde ignores unknown fields — no SDK bump required)
- [x] Test: `test_extract_inner_instructions_accepts_transaction_v1` via new `build_tx_with_version` harness in `soltrace-core/src/utils.rs`
- [x] Flip `maxSupportedTransactionVersion` 0 → 1: `soltrace-backfill/src/main.rs:419`, `soltrace-live/src/main.rs:513` (gap backfill), `soltrace-live/src/main.rs:823` (per-notification), `scripts/fix_slots.py:50`
- [x] Comment updates: `types.rs`, `utils.rs`, `live main.rs`

Verification: `cargo test --workspace` 85/85 pass (incl. new v1 fixture). Pre-existing clippy warnings (`must_use` in db/queue `async_trait`, unused `file_prog` in utils tests) and rustfmt drift on this machine are untouched — outside this change's scope.
