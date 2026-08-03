---
# soltrace-lx43
title: Delete metrics.rs module
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

Delete `soltrace-core/src/metrics.rs` entirely (388 lines).

- `Metrics`, `HealthCheck`, `HealthStatus`, `MetricsSnapshot`, `HealthCheckResult` — zero consumers in `soltrace-live` or `soltrace-backfill`; only self-tests reference them.
- Drop `pub mod metrics;` from `soltrace-core/src/lib.rs`.
- Drop the re-export line: `pub use metrics::{HealthCheck, HealthStatus, Metrics, MetricsSnapshot};`

## Files

- `soltrace-core/src/metrics.rs` (delete)
- `soltrace-core/src/lib.rs` (edit)

## Verify

- `rg -t rust 'Metrics|HealthCheck|HealthStatus|MetricsSnapshot' soltrace-live soltrace-backfill` → empty (re-confirm)
- `cargo build --workspace` green
- `cargo test --workspace` green
- ~388 lines removed
