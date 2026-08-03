---
# soltrace-b6wk
title: Review Epic 3 + milestone close
status: completed
type: task
priority: normal
created_at: 2026-07-28T08:32:14Z
updated_at: 2026-07-30T14:03:19Z
parent: soltrace-87oa
blocked_by:
    - soltrace-b4md
---

See MS soltrace-z9wq HANDOFF. Final review before milestone close.

## Cut

Final review across all epics. Verify against Definition of Done (HANDOFF section 5):

- [ ] --onchain-programs flag works end-to-end in both binaries
- [ ] Live subscription swaps in new IDL on SetData push (verified by Epic 2 tests or localnet demo)
- [ ] Subscription survives WS disconnect (reconnect + re-subscribe)
- [ ] SetImmutable -> unsubscribe; Close -> drop + warn
- [ ] File IDL precedence verified: file IDL for same program as on-chain wins
- [ ] Backfill one-shot fetch verified
- [ ] No behavioral change when --onchain-programs is empty
- [ ] idls/README.md updated (Decision 2, 1, 5, 7 + program-metadata link)
- [ ] cargo build --workspace --all-features green
- [ ] cargo test --workspace green
- [ ] cargo clippy --workspace --all-features -- -D warnings green
- [ ] Ponytail: no speculative abstractions introduced across the milestone

If all green, mark this bean completed AND milestone soltrace-z9wq completed. If issues found, block with comments.

## Summary of Changes (required at completion)
Update this bean with a summary of what was built, files added/modified, deps added, and any deviations from the original HANDOFF (with rationale).

## Summary of Changes (Review — Epic 3 + milestone close)

### Definition of Done verification (HANDOFF section 5) — ALL GREEN

- [x] `--onchain-programs` works end-to-end in both binaries — `--help` shows the flag (optional, `[default: ]`, env `ONCHAIN_PROGRAMS`); wired in soltrace-live (full startup ordering incl. subscription spawn) and soltrace-backfill (one-shot fetch only).
- [x] Live subscription swaps new IDL on `SetData` push — `idl_subscription::tests::setdata_push_swaps_in_new_idl`.
- [x] Subscription survives WS disconnect — `session_returns_err_when_ws_unreachable` + `backoff_capped_at_60s`; reconnect loop re-subscribes to ALL PDAs (idl_subscription.rs reconnect path).
- [x] `SetImmutable` -> unsubscribe — `setimmutable_swaps_and_returns_unsubscribe`; `Close` -> drop + warn — `close_push_removes_idl_and_unsubscribes`.
- [x] File IDL precedence — `utils::tests::test_load_onchain_idls_preserves_existing_idl` + `test_epic1_file_and_onchain_coexist`.
- [x] Backfill one-shot fetch — backfill main.rs calls `load_onchain_idls` once, no subscription task; semantics covered by `test_load_onchain_idls_*`.
- [x] No behavioral change when flag empty — `test_cli_onchain_programs_defaults_empty_when_absent` (both binaries) + `test_parse_onchain_programs_empty_is_noop` + `if !onchain_programs.is_empty()` guards.
- [x] `idls/README.md` updated — on-chain section, file-precedence, Direct-only, live-vs-backfill, program-metadata link (soltrace-c2ho).
- [x] `cargo build --workspace --all-features` green.
- [x] `cargo test --workspace` green (78 tests).
- [x] `cargo clippy --workspace --all-features -- -D warnings` green.

### Milestone deliverables

New files:
- `soltrace-core/src/onchain_idl.rs` — canonical PDA derivation, Metadata decode (inflate/decode per compression/encoding), Direct-only fetch.
- `soltrace-live/src/idl_subscription.rs` — dedicated-WS `accountSubscribe` task: hot-swap on push, unsubscribe on SetImmutable, drop+warn on Close, capped-backoff reconnect.

Modified:
- `soltrace-core/src/idl.rs` (`insert_or_replace`, `remove`, `Clone`), `event.rs` (EventDecoder: `Arc<ArcSwap<IdlParser>>`), `utils.rs` (`load_onchain_idls` + Epic-1 integration tests), `lib.rs` (re-exports).
- `soltrace-live/src/main.rs` + `soltrace-backfill/src/main.rs` — `--onchain-programs` CLI + HANDOFF section-3 startup ordering.
- `idls/README.md`, `Cargo.toml` (deps).

Deps added: `flate2`, `arc-swap`, `spl-program-metadata-client` (core); `solana-account-decoder-client-types`, `spl-program-metadata-client` (live).

### Deviations from HANDOFF (with rationale)
- `decode_metadata_account` takes `&[u8]` not `&Account` — avoids marshalling two `Account` crate versions (solana-sdk 2.x vs workspace 4.0). Documented in source.
- `load_onchain_idls` is sync (blocking RPC at startup), not async — matches the delivered tahu signature; a one-shot startup call.
- Live merges on-chain programs into the logs filter before their IDL arrives (HANDOFF section-3 chicken-and-egg).
- Live localnet E2E is documented as manual repro (program-metadata program binary unavailable in this env — only the client lib resolves); the constituent logic is fully unit-tested and the zero-regression contract is automated.

### Review fix
Removed a stale `#[allow(dead_code)]` on `spawn_idl_subscription_task` — it was a placeholder for the (now-landed) binary wiring. No warning returns; clippy stays clean.

### Ponytail compliance
No speculative abstractions (no single-impl traits, no factories, no unused config knobs). 8 `// ponytail:` comments mark deliberate shortcuts (double-parse for mutable flag, file-precedence skip, reconnect re-subscribe-all). Public API surface minimal (4 items in onchain_idl.rs).

Milestone soltrace-z9wq: ALL GREEN — closed.
