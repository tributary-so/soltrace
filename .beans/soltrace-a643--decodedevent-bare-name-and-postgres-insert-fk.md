---
# soltrace-a643
title: DecodedEvent bare name and postgres insert FK
status: completed
type: task
priority: normal
created_at: 2026-10-29T11:35:25Z
updated_at: 2026-10-29T13:02:57Z
parent: soltrace-ptv8
---

- [x] DecodedEvent.event_name_bare with serde(default)
- [x] event.rs sets both composed + bare
- [x] postgres insert_event: bare name + program_id, id hash unchanged (composed input)
- [x] address->id cache + lazy upsert CTE
- [x] sqlite/mongo untouched behavior
- [x] Tests: decode returns both names; unit coverage

## Summary of Changes

Binaries snapshot explicit mappings before load_from_idls so "default" prefixes never seed programs.name; unmapped programs named by full base58 address.
