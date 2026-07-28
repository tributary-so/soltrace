---
# soltrace-7r53
title: Delete validation.rs module
status: completed
type: task
priority: high
tags:
    - ponytail-audit
    - delete
created_at: 2026-07-28T07:56:59Z
updated_at: 2026-07-28T08:15:41Z
parent: soltrace-qwu9
---

See milestone soltrace-kok1 HANDOFF.

## Cut

Delete `soltrace-core/src/validation.rs` entirely (276 lines).

- `validate_program_id`, `validate_program_ids`, `validate_directory`, `validate_db_url`, `validate_rpc_url`, `validate_ws_url`, `validate_commitment` — zero callers in either binary (both parse clap args inline).
- `BackfillConfig`, `LiveConfig` structs — never constructed.
- Drop `pub mod validation;` and the `pub use validation::{...}` re-export from `lib.rs`.

## Files

- `soltrace-core/src/validation.rs` (delete)
- `soltrace-core/src/lib.rs` (edit)

## Verify

- `rg -t rust 'validate_program_id|validate_rpc_url|validate_ws_url|BackfillConfig|LiveConfig'` → empty outside `soltrace-core` (re-confirm)
- `cargo build --workspace && cargo test --workspace` green
- ~276 lines removed
