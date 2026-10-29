---
# soltrace-9em9
title: Migrations 0001+0002 and seed/backfill step
status: in-progress
type: task
created_at: 2026-10-29T11:35:25Z
updated_at: 2026-10-29T11:35:25Z
parent: soltrace-u89u
---

- [ ] Add sqlx 'migrate' feature to workspace Cargo.toml
- [ ] migrations/0001_baseline.sql reproduces current events schema
- [ ] migrations/0002_programs_fk.sql: programs table, guarded rename, FK column, partial index, VIEW events
- [ ] run_migrations -> sqlx::migrate!().run
- [ ] try_enable_timescaledb retargets event_rows
- [ ] seed_programs default no-op in trait; postgres impl: program upsert + backfill UPDATE (left() match)
- [ ] Test: fresh pg migrates, legacy row backfills, VIEW round-trips composed name
