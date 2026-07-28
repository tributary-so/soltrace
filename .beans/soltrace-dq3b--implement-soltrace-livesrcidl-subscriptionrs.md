---
# soltrace-dq3b
title: Implement soltrace-live/src/idl_subscription.rs
status: todo
type: task
priority: normal
created_at: 2026-07-28T08:31:44Z
updated_at: 2026-07-28T10:05:03Z
parent: soltrace-sfg2
---

See MS soltrace-z9wq HANDOFF (Decisions 6, 8, 9). Blocked by Epic 1 (uses core primitives).

## Cut

New file soltrace-live/src/idl_subscription.rs. Exposes:

pub fn spawn_idl_subscription_task(
    programs: Vec<Pubkey>,
    ws_url: String,
    commitment: CommitmentConfig,
    shared_parser: Arc<ArcSwap<IdlParser>>,
) -> JoinHandle<()>

Internal responsibilities:
1. Open a dedicated PubsubClient (separate from logs_subscribe's). Do NOT share connection.
2. For each program: derive_canonical_idl_pda, then account_subscribe(pda, commitment, encoding=base64+zstd optional). accountSubscribe pushes initial state immediately, so no separate startup fetch needed.
3. On each notification: decode account data via decode_metadata_account(pda_account, program). On Ok(Some(new_idl)): build a fresh IdlParser from current load() + insert_or_replace(new_idl), then shared_parser.store(new). On Ok(None) or Err: warn, DO NOT swap (keep last-known-good).
4. On account closed notification (empty account, lamports=0): drop program from a fresh IdlParser, swap, unsubscribe, warn.
5. After first successful decode per account, inspect meta.mutable. If false (SetImmutable): unsubscribe + log info once. (mutable comes from the Metadata header — read on each push, act on transition true→false.)
6. Reconnect loop mirroring main.rs:485-555: capped exponential backoff (>10 attempts -> 60s). On reconnect: re-subscribe to ALL active PDAs (subscriptions don't persist). Honor --max-reconnects if set (same flag as logs or separate? — confirm during impl, default: no max).
7. Task panic / exhaustion: log error and return. Do NOT take down indexer (logs task continues with last-known IdlParser).

Use existing patterns from main.rs: PubsubClient::new(ws_url).await, mpsc channel decoupling if helpful.

## TDD (mock WS or test against a localnet validator)
- Test: mock WS pushes a SetData notification -> shared_parser.load() reflects new IDL
- Test: mock WS pushes Close (empty account) -> program removed from parser
- Test: mock WS pushes garbage decode -> shared_parser NOT swapped, warn emitted
- Test: mock WS disconnects -> reconnect attempts fire, on reconnect all PDAs re-subscribed

If mocking PubsubClient is too heavy, structure the code so the per-notification handler is a pure fn(decoder_input, &shared_parser) that's unit-testable, and test the WS plumbing separately or skip.

## Verify
- cargo test -p soltrace-live green
- cargo clippy -p soltrace-live -- -D warnings
- Manual: run against a localnet with a program that has a program-metadata IDL account, trigger a SetData, verify log shows the swap

## Blocker (verified 2026-07-28T08:40:19Z)

Cannot implement. Unmet cross-epic dependency on `soltrace-ipo0` (Core on-chain IDL fetch+decode primitives), which is `todo` and only just starting (currentTask: `soltrace-nqfd` = add deps).

Verified missing in worktree:
- `soltrace-core/src/onchain_idl.rs` does not exist (no `derive_canonical_idl_pda`, no `decode_metadata_account`)
- `IdlParser` has no `insert_or_replace` / `remove` (idl.rs)
- `EventDecoder.idl_parser` is still `IdlParser`, not `Arc<ArcSwap<IdlParser>>` (event.rs:10)

This task consumes all three as its public API. Building them here would duplicate Epic 1's lane (`soltrace-ipo0` branch) and create merge conflicts.

`hordr blocked` is not a registered command in this hordr version (0.1.0); no programmatic lane-release available. Bean left at `todo`. Re-dispatch once `soltrace-ipo0` lands its primitives.
