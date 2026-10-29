---
# soltrace-9em9
title: Migrations 0001+0002 and seed/backfill step
status: completed
type: task
priority: normal
created_at: 2026-10-29T11:35:25Z
updated_at: 2026-10-29T13:02:57Z
parent: soltrace-u89u
---

- [x] Add sqlx 'migrate' feature to workspace Cargo.toml
- [x] migrations/0001_baseline.sql reproduces current events schema
- [x] migrations/0002_programs_fk.sql: programs table, guarded rename, FK column, partial index, VIEW events
- [x] run_migrations -> sqlx::migrate!().run
- [x] try_enable_timescaledb retargets event_rows
- [x] seed_programs default no-op in trait; postgres impl: program upsert + backfill UPDATE (left() match)
- [x] Test: fresh pg migrates, legacy row backfills, VIEW round-trips composed name

## Summary of Changes

0002 renames via guarded DO block (works on fresh + legacy DBs); verified end-to-end by `migration_cutover_end_to_end` (env-gated on SOLTRACE_PG_TEST_URL) against postgres:16.
