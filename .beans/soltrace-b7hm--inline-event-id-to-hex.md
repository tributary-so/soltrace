---
# soltrace-b7hm
title: Inline event_id_to_hex
status: completed
type: task
priority: low
tags:
    - ponytail-audit
    - stdlib
created_at: 2026-07-28T07:57:39Z
updated_at: 2026-07-28T08:25:25Z
parent: soltrace-ycdk
---

See milestone soltrace-kok1 HANDOFF.

## Cut

`soltrace-core/src/db/mod.rs:19-21`:

```rust
pub fn event_id_to_hex(id: &[u8; 32]) -> String {
    hex::encode(id)
}
```

A one-line wrapper around `hex::encode`. Inline at the 3 call sites and delete the function.

Call sites (all in db backends):
- `soltrace-core/src/db/sqlite.rs:78` — `let event_id = event_id_to_hex(&id_bytes);` → `let event_id = hex::encode(&id_bytes);`
- `soltrace-core/src/db/postgres.rs:122` — same
- `soltrace-core/src/db/mongodb.rs:105` — same

Drop `event_id_to_hex` from the import list at the top of each backend file.

## Files

- `soltrace-core/src/db/mod.rs` (delete fn)
- `soltrace-core/src/db/sqlite.rs` (edit)
- `soltrace-core/src/db/postgres.rs` (edit)
- `soltrace-core/src/db/mongodb.rs` (edit)

## Verify

- `rg 'event_id_to_hex'` → empty
- `cargo test --workspace` green
- ~5 lines removed
