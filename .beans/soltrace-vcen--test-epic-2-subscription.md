---
# soltrace-vcen
title: Test Epic 2 (subscription)
status: todo
type: task
created_at: 2026-07-28T08:31:44Z
updated_at: 2026-07-28T08:31:44Z
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
