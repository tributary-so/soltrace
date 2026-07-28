---
# soltrace-5b95
title: Remove Database wrapper struct
status: completed
type: task
priority: high
tags:
    - ponytail-audit
    - yagni
created_at: 2026-07-28T07:57:39Z
updated_at: 2026-07-28T08:25:25Z
parent: soltrace-ycdk
---

See milestone soltrace-kok1 HANDOFF.

## Cut

`soltrace-core/src/db/mod.rs:62-103` defines a `Database` struct that is a pure delegation layer over `Arc<dyn DatabaseBackend>`. Each of its 7 methods (`run_migrations`, `insert_event`, `get_events_by_slot_range`, `get_events_by_name`, `event_exists`, `get_latest_signature`, `new`) just forwards to `self.backend`.

The factory already returns `Arc<dyn DatabaseBackend>`. Callers currently wrap in `Arc<Database>` for no reason — change them to hold `Arc<dyn DatabaseBackend>` directly.

## Files

- `soltrace-core/src/db/mod.rs` (delete `Database` struct + impl block, keep `DatabaseBackend` trait, `EventRecord`, `generate_event_id`, `event_id_to_hex`)
- `soltrace-core/src/lib.rs` (re-export `DatabaseBackend` instead of `Database`)
- `soltrace-live/src/main.rs` (replace `Arc<Database>` → `Arc<dyn DatabaseBackend>` at all sites; `Database::new(url)` → `create_backend(url)`)
- `soltrace-backfill/src/main.rs` (same)
- `soltrace-core/src/utils.rs` (`process_transaction` takes `&dyn DatabaseBackend` or `&Arc<dyn DatabaseBackend>`)

## Verify

- `cargo test --workspace` green — utils.rs dedup/idempotency tests must still pass
- `rg '\bDatabase\b'` outside the deleted struct definition → only `DatabaseBackend` references
- ~40 lines removed

## Note

DO NOT collapse `DatabaseBackend` trait or the three backend impls — that is explicitly out of scope (see HANDOFF §7).
