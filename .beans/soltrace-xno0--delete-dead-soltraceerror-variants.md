---
# soltrace-xno0
title: Delete dead SoltraceError variants
status: completed
type: task
priority: low
tags:
    - ponytail-audit
    - delete
created_at: 2026-07-28T07:56:59Z
updated_at: 2026-07-28T08:18:17Z
parent: soltrace-qwu9
---

See milestone soltrace-kok1 HANDOFF.

## Cut

From `soltrace-core/src/error.rs`, delete three variants of `SoltraceError` that are declared but never constructed anywhere:

- `DiscriminatorMismatch`
- `EventNotFound(String)`
- `SolanaClient(String)`

Used variants (keep): `IdlParse`, `EventDecode`, `Database`, `Sqlx`, `Io`, `Serialization`, `InvalidIdl`.

## Files

- `soltrace-core/src/error.rs` (edit)

## Verify

- `rg 'DiscriminatorMismatch|EventNotFound|SolanaClient' soltrace-core/src` → only the deletion site
- `cargo build --workspace` green
- ~6 lines removed
