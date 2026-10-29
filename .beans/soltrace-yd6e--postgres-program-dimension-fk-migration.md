---
# soltrace-yd6e
title: 'Postgres program dimension: FK migration'
status: completed
type: milestone
priority: normal
created_at: 2026-10-29T11:35:10Z
updated_at: 2026-10-29T13:02:57Z
---

Replace prefix-in-event_name denormalization with a programs dimension table on postgres. Decisions from grilling 2026-10-29: (1) programs(id SERIAL PK, address UNIQUE, name) one row per on-chain address; name from --program-prefixes at first sight, fallback = full base58 address, IMMUTABLE after assignment (id-hash continuity). (2) events renamed to event_rows, bare event_name, nullable program_id FK; legacy rows backfilled config-driven (left()-prefix match, no LIKE wildcards), unmatched keep prefixed name + NULL. (3) dedup id stays sha256(sig, index, "{name}_{bare}") = legacy composed name — ids identical to legacy where backfilled, cutover-safe re-walks. (4) VIEW events = LEFT JOIN programs + COALESCE reproduces today's exact shape; kept indefinitely, documented deprecated. (5) POSTGRES ONLY — sqlite/mongo frozen on old write path (user decision; examples/tributary stays old schema, docs note it). (6) sqlx::migrate! files replace hand-rolled run_migrations. (7) Dead read API (get_events_by_name, get_events_by_slot_range, event_exists, EventRecord) deleted; get_latest_signature stays retargeted.

## Summary of Changes

Shipped in commit `feat(core): 🗃️ postgres v2 schema: programs FK replaces event-name prefixes` (develop). All four tasks + three epics closed. Key implementation facts beyond the plan: binaries snapshot explicit `--program-prefixes` mappings BEFORE `load_from_idls` (which fills "default" and would have re-created the collapse); dead read API deletion also orphaned sqlite's parse_timestamp and mongo's EventDocument conversion; clippy gate repaired (HEAD was failing: double_must_use on async-trait + unused test var). Verified by `migration_cutover_end_to_end` against postgres:16 + soltrace-live binary smoke + frozen sqlite init.
