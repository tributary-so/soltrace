---
# soltrace-jsoo
title: Replace EventQueue trait with cfg-gated KafkaProducer
status: scrapped
type: task
priority: normal
tags:
    - ponytail-audit
    - yagni
created_at: 2026-07-28T07:57:39Z
updated_at: 2026-07-28T08:29:52Z
parent: soltrace-ycdk
---

See milestone soltrace-kok1 HANDOFF.

## Decision: scrapped — audit was wrong about this one

The `EventQueue` trait has a single implementation (`KafkaProducer`), but that implementation lives behind the `kafka` cargo feature. The trait is **load-bearing**: it lets `soltrace-live` express `Option<Arc<dyn EventQueue>>` as a typed parameter that compiles whether or not the `kafka` feature is enabled.

Removing the trait would require cfg-gating the kafka plumbing across 5 function signatures in `soltrace-live/src/main.rs` (run_indexer, gap_backfill, run_websocket_loop, websocket_handler, process_logs_message) plus their ~10 call/pass sites. That's a refactor, explicitly **out of scope** per milestone HANDOFF §3 / §7.

The audit conflated "single impl" with "removable." Single-impl-behind-a-feature-flag is a legitimate type-erasure pattern; the trait earns its keep by keeping the non-kafka build typecheck-simple.

## Related real cut (not done here)

`async-trait` crate is now unnecessary (Rust 1.75+ has native async fn in traits; this repo is on nightly). Dropping `#[async_trait]` across `db/mod.rs`, the 3 backends, and `queue/{mod,kafka}.rs` would remove a dependency without touching the trait. That's a separate task if wanted — NOT this one.
