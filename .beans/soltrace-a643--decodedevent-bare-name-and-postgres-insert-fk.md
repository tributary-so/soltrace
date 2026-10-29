---
# soltrace-a643
title: DecodedEvent bare name and postgres insert FK
status: in-progress
type: task
created_at: 2026-10-29T11:35:25Z
updated_at: 2026-10-29T11:35:25Z
parent: soltrace-ptv8
---

- [ ] DecodedEvent.event_name_bare with serde(default)
- [ ] event.rs sets both composed + bare
- [ ] postgres insert_event: bare name + program_id, id hash unchanged (composed input)
- [ ] address->id cache + lazy upsert CTE
- [ ] sqlite/mongo untouched behavior
- [ ] Tests: decode returns both names; unit coverage
