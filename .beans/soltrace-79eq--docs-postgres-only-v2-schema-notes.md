---
# soltrace-79eq
title: 'Docs: postgres-only v2 schema notes'
status: completed
type: task
priority: normal
created_at: 2026-10-29T11:35:25Z
updated_at: 2026-10-29T13:02:57Z
parent: soltrace-7o6s
---

- [x] AGENTS.md: postgres-only v2 note on three-backend rule
- [x] README: schema section (programs/event_rows/VIEW events), name immutability, VIEW deprecated
- [x] CONTEXT.md only if terms drift (event_rows aligns with Event Row — no change expected)

## Summary of Changes

README schema section also fixed pre-existing staleness (documented columns that never existed in code). CONTEXT.md untouched — event_rows matches "Event Row".
