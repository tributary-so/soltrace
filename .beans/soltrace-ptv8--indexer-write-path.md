---
# soltrace-ptv8
title: Indexer write path
status: completed
type: epic
priority: normal
created_at: 2026-10-29T11:35:17Z
updated_at: 2026-10-29T11:54:33Z
parent: soltrace-yd6e
---

DecodedEvent gains event_name_bare (serde default); postgres insert stores bare + program FK with in-memory address->id cache and lazy upsert (UNION ALL CTE, no write on conflict); id hash keeps composed name input. sqlite/mongo write path unchanged.
