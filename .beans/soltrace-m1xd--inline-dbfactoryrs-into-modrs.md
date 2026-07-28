---
# soltrace-m1xd
title: Inline db/factory.rs into mod.rs
status: completed
type: task
priority: normal
tags:
    - ponytail-audit
    - shrink
created_at: 2026-07-28T07:57:39Z
updated_at: 2026-07-28T08:25:25Z
parent: soltrace-ycdk
blocked_by:
    - soltrace-5b95
---

See milestone soltrace-kok1 HANDOFF.

## Cut

`soltrace-core/src/db/factory.rs` is a 23-line file with one function (`create_backend`). Inline that function into `soltrace-core/src/db/mod.rs`, drop the `factory` submodule + the `pub mod factory;` declaration + the `pub use factory::create_backend;` re-export.

Best sequenced AFTER the `Database` wrapper task (soltrace-<id from above>) since the wrapper removal will already touch `mod.rs`. Either way works — they touch overlapping lines.

## Files

- `soltrace-core/src/db/factory.rs` (delete)
- `soltrace-core/src/db/mod.rs` (absorb `create_backend`)

## Verify

- `cargo build --workspace` green
- `ls soltrace-core/src/db/` shows no `factory.rs`
- ~15 lines removed (net — the fn body moves, the file boilerplate goes)
