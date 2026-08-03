---
# soltrace-ft40
title: Delete KafkaConfig::from_env
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

Delete `KafkaConfig::from_env` in `soltrace-core/src/queue/kafka.rs:18-22`.

`soltrace-live/src/main.rs` reads `--kafka-brokers` via clap and calls `KafkaConfig::new(brokers.clone())` directly — `from_env` is never called.

## Files

- `soltrace-core/src/queue/kafka.rs` (edit)

## Verify

- `rg 'KafkaConfig::from_env'` → empty
- `cargo build --workspace` green
- ~5 lines removed
