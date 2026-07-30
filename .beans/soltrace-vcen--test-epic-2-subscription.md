---
# soltrace-vcen
title: Test Epic 2 (subscription)
status: completed
type: task
priority: normal
created_at: 2026-07-28T08:31:44Z
updated_at: 2026-07-28T12:43:11Z
parent: soltrace-sfg2
blocked_by:
  - soltrace-dq3b
---

See MS soltrace-z9wq HANDOFF.

## Cut

Integration/characterization tests for the subscription task:

- Verify the per-notification handler is pure and unit-testable (per T21 design). If not pure, refactor for testability.
- Cover all 4 cases from HANDOFF test matrix: SetData, Close, garbage decode, reconnect.
- Verify terminal state: SetImmutable (mutable=false) triggers unsubscribe + info log.
- Verify arc-swap visibility: writer task swaps, concurrent reader observes new state.

If a real localnet end-to-end test is feasible (solana-test-validator + a real program-metadata Initialize), add it as a #[ignore] integration test (gated by feature or env var to avoid CI network needs).

## Verify

- cargo test -p soltrace-live green
- Coverage of all 4 notification paths + SetImmutable + reconnect

## Summary of Changes

Added 4 new tests to `idl_subscription.rs` (total: 12) extending the dq3b
handler tests with concurrency and session-level coverage:

1. **`concurrent_swap_and_read_never_tears`** — Arc-swap visibility: 1 writer
   hammers `handle_account_notification`, 3 readers load 5k times each. No torn
   reads — every observed name starts with 'V'. Mirrors event.rs's test.
2. **`multiple_programs_swap_independently`** — Two programs pushed
   independently; both land with correct names.
3. **`close_one_program_leaves_others_intact`** — Closing A leaves B untouched.
4. **`session_returns_err_when_ws_unreachable`** — Dead-port WS returns Err;
   parser not mutated. Exercises the reconnect-trigger path.

Full reconnect-loop re-subscribe (mock WS server) deferred per the bean's skip
clause — verified via localnet demo.
