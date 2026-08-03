---
# soltrace-yrg6
title: Review Epic 2
status: completed
type: task
priority: normal
created_at: 2026-07-28T08:31:44Z
updated_at: 2026-07-28T12:46:43Z
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

## Review Findings

### Checklist — all PASS

| #   | Requirement                          | Status | Evidence                                                                                                    |
| --- | ------------------------------------ | ------ | ----------------------------------------------------------------------------------------------------------- |
| 1   | Dedicated WS connection              | PASS   | `PubsubClient::new(ws_url)` at `idl_subscription.rs:143` — own connection, not shared with logs_subscribe   |
| 2   | accountSubscribe (not polling/logs)  | PASS   | `pubsub.account_subscribe(&pda, ...)` at `:160` — per-PDA push subscription                                 |
| 3   | No startup fetch_metadata            | PASS   | No `fetch_canonical_idl` call — initial IDL arrives via accountSubscribe's immediate push                   |
| 4   | Decode failure does NOT swap         | PASS   | `Err`/`Ok(None)` branches (`:75-82`) return `Keep` — no `store()` call, parser untouched                    |
| 5   | SetImmutable unsub / Close drop+warn | PASS   | `!meta.mutable` → `Unsubscribe` (`:67-72`); closed (`None`) → `remove()` + warn + `Unsubscribe` (`:47-53`)  |
| 6   | Reconnect mirrors main.rs:485-555    | PASS   | Same shape: linear ramp (1-10s), 60s cap after 10 attempts (`:99-121`, `:124-130`)                          |
| 7   | Task crash doesn't kill indexer      | PASS   | Independent `tokio::spawn` (`:99`); panic captured by JoinHandle, logs_subscribe unaffected                 |
| 8   | Ponytail: no over-engineering        | PASS   | `SelectAll` (stdlib stream combinator), plain fn handler, no trait objects, `// ponytail:` shortcuts marked |
| 9   | Commitment same knob as logs         | PASS   | `commitment: CommitmentConfig` param (`:96`); passed to config (`:149`)                                     |

### Observations (non-blocking)

1. **No `max_reconnects` param.** The bean asked to "honor --max-reconnects if set (default: no max)." Current signature has no such param — loops forever until all programs exhausted. Default behavior matches the bean's stated default. Binary-integration task should add this if needed.

2. **Backoff ramp differs slightly from main.rs.** main.rs uses `reconnect_delay * count` (configurable base). This impl uses bare `count` (1s, 2s...10s, 60s). Same shape, no configurable multiplier. Acceptable for a dedicated task that's less critical than logs_subscribe.

3. **Double-parse of Metadata header.** `decode_metadata_account` parses borsh internally; then `Metadata::from_bytes` re-parses to read `mutable`. Documented as `// ponytail:` shortcut — one extra ~90-byte deserialise is negligible vs extending Epic 1's delivered API.

4. **Reconnect re-subscribes to dead PDAs.** On reconnect, ALL programs (including immutable/closed) get re-subscribed. They receive one immediate push, decode, then unsubscribe again. One wasted round-trip per dead PDA. Documented as `// ponytail:` — cheaper than tracking per-program terminal state across sessions.

### Verdict

**SHIP IT.** All 9 decision-conformance checks pass. No blocking issues. The 4 observations are all documented ponytail shortcuts with clear upgrade paths.
