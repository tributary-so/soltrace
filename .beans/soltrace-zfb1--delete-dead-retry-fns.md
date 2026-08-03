---
# soltrace-zfb1
title: Delete dead retry fns
status: completed
type: task
priority: normal
tags:
    - ponytail-audit
    - delete
created_at: 2026-07-28T07:56:59Z
updated_at: 2026-07-28T08:16:14Z
parent: soltrace-qwu9
---

See milestone soltrace-kok1 HANDOFF.

## Cut

From `soltrace-core/src/retry.rs`, delete three functions and their tests:

- `retry_with_backoff` (~40 lines) + tests `test_retry_with_backoff_success`, `test_retry_with_backoff_failure`
- `concurrent_process` (~22 lines) + test `test_concurrent_process`
- `process_batches` (~28 lines)

Only `retry_with_rate_limit` has consumers (live + backfill). Keep that one.

Drop the matching re-exports from `lib.rs`:
`pub use retry::{concurrent_process, process_batches, retry_with_backoff, retry_with_rate_limit};`
→ `pub use retry::retry_with_rate_limit;`

## Files

- `soltrace-core/src/retry.rs` (edit)
- `soltrace-core/src/lib.rs` (edit)

## Verify

- `cargo build --workspace && cargo test --workspace` green
- ~140 lines removed
