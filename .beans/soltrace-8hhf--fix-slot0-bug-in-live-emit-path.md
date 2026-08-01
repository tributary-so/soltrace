---
# soltrace-8hhf
title: Fix slot=0 bug in live emit! path
status: completed
type: bug
priority: high
created_at: 2026-08-01T20:32:38Z
updated_at: 2026-08-01T20:34:21Z
---

RpcLogsResponse carries no slot. process_logs_message hardcodes slot:0 for emit! events then fetches the tx later for emit_cpi!. Hoist the get_transaction fetch above the emit! loop so both paths use transaction.slot.

## Summary of Changes

Hoisted the get_transaction fetch in process_logs_message (soltrace-live/src/main.rs) above both the emit! and emit_cpi! decode loops. Both RawEvent constructions now use transaction.slot and the real block_time-derived timestamp; the hardcoded slot: 0 is gone. Compiles clean (cargo check -p soltrace-live).

Behavior change: if the tx fetch fails, no events are stored for that signature (previously emit! events were stored with slot=0). Acceptable — reprocess/backfill is idempotent on the dedup key and storing slot=0 was the bug.
