---
# soltrace-u89u
title: Schema and migrations
status: completed
type: epic
priority: normal
created_at: 2026-10-29T11:35:17Z
updated_at: 2026-10-29T11:54:33Z
parent: soltrace-yd6e
---

sqlx migrate files: 0001 baseline (current events schema), 0002 cutover (programs table, guarded rename events->event_rows, program_id FK + partial unmigrated index, idx_program_id, compat VIEW events). Startup seed_programs step: idempotent program upsert from --program-prefixes + self-extinguishing backfill UPDATE via left() prefix match.
