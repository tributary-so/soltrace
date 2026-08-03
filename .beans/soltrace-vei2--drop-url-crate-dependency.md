---
# soltrace-vei2
title: Drop url crate dependency
status: completed
type: task
priority: normal
tags:
    - ponytail-audit
    - native
created_at: 2026-07-28T07:57:39Z
updated_at: 2026-07-28T08:29:52Z
parent: soltrace-ycdk
---

See milestone soltrace-kok1 HANDOFF.

## Cut

`url` crate (workspace Cargo.toml:31) is used in exactly one place: `soltrace-core/src/db/mongodb.rs:47-54` to parse the db name off the Mongo connection string path. The same logic is a one-liner string split:

```rust
let db_name = database_url
    .split('?').next().unwrap_or(database_url)   // strip query
    .trim_end_matches('/')
    .rsplit('/').next()
    .filter(|s| !s.is_empty())
    .unwrap_or("soltrace");
```

(Or simpler: `database_url.rsplit_once('/').map(|(_, last)| last).filter(|s| !s.is_empty()).unwrap_or("soltrace")` after stripping `?`-query if present.)

Drop `url = "2"` from `Cargo.toml` `[workspace.dependencies]` AND from `soltrace-core/Cargo.toml` `[dependencies]`.

## Files

- `Cargo.toml` (workspace deps)
- `soltrace-core/Cargo.toml`
- `soltrace-core/src/db/mongodb.rs` (edit)

## Verify

- `cargo tree -p soltrace-core | grep -w url` → empty
- `cargo test --workspace` green
- Mongo db-name extraction still produces `soltrace` for `mongodb://host:27017/soltrace?retryWrites=true` (add a unit test if no coverage exists)
- 1 dependency removed
