---
# soltrace-a0vt
title: Delete dead read API and retarget resume query
status: completed
type: task
priority: normal
created_at: 2026-10-29T11:35:25Z
updated_at: 2026-10-29T13:02:57Z
parent: soltrace-7o6s
---

- [x] Remove get_events_by_name/get_events_by_slot_range/event_exists/EventRecord from trait + all backends + lib.rs
- [x] get_latest_signature -> event_rows
- [x] cargo fmt/clippy/test clean

## Summary of Changes

Also removed orphans the deletion created: sqlite parse_timestamp, mongo EventDocument→EventRecord conversion, unused imports. Zero callers existed (verified by grep before deletion).
