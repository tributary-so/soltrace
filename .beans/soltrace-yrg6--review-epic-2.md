---
# soltrace-yrg6
title: Review Epic 2
status: todo
type: task
created_at: 2026-07-28T08:31:44Z
updated_at: 2026-07-28T08:31:44Z
parent: soltrace-sfg2
blocked_by:
    - soltrace-vcen
---

See MS soltrace-z9wq HANDOFF.

## Cut

Code review of Epic 2 deliverables. Verify:
- Dedicated WS connection (NOT shared with logs_subscribe) — Decision 6.
- accountSubscribe used (not account polling, not logsSubscribe on ProgM_ID).
- No separate startup fetch_metadata call — push delivers initial state.
- Decode failure does NOT swap IdlParser — Decision 11.
- SetImmutable unsubscribes; Close drops from map + warns — Decision 6.
- Reconnect loop is a mirror of main.rs:485-555, not a new invention.
- Subscription task crash does NOT take down indexer.
- Ponytail: no over-engineered WS abstraction, no speculative multiplexing.
- Commitment level is the same knob as logs_subscribe (Decision 8).

Report findings as comments in this bean.
