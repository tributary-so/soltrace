---
# soltrace-hb6z
title: Wire --onchain-programs CLI in both binaries + correct startup ordering
status: todo
type: task
priority: normal
created_at: 2026-07-28T08:32:14Z
updated_at: 2026-07-28T08:40:56Z
parent: soltrace-87oa
blocked_by:
    - soltrace-nqfd
    - soltrace-n63g
    - soltrace-nc4t
    - soltrace-tahu
    - soltrace-dq3b
---

See MS soltrace-z9wq HANDOFF (Decision 2 + ordering in section 3).

## Cut

Both soltrace-live/src/main.rs and soltrace-backfill/src/main.rs:

1. Add CLI arg --onchain-programs <CSV> and env ONCHAIN_PROGRAMS (CLI wins). Mirror existing --idl-dir / IDL_DIR pattern (main.rs:73-75).
2. Parse CSV to Vec<Pubkey> via Pubkey::from_str. HARD-ERROR on any malformed entry (do NOT silently drop).
3. Empty/unset -> empty Vec -> feature is a no-op (zero behavioral change for existing operators).
4. Update startup ordering (HANDOFF section 3):
   a. IdlParser::new()
   b. load_idls (file) — existing
   c. load_onchain_idls(parser, rpc, &programs) — NEW (uses RpcClient already constructed elsewhere)
   d. ProgramPrefixConfig::load_from_idls(&parser) — existing, but now sees both file + on-chain IDLs
   e. Wrap parser: Arc::new(ArcSwap::from(parser))
   f. EventDecoder::new(shared_parser, prefix_config) — signature updated per soltrace-nc4t
   g. soltrace-live ONLY: spawn_idl_subscription_task(programs, ws_url, ws_commitment, shared_parser.clone()) — per soltrace-dq3b
   h. Start logs_subscribe (existing)

soltrace-backfill: same but skip step (g). One-shot fetch only.

## TDD
- Test CLI parse: valid CSV -> Vec<Pubkey>; invalid entry -> Err with clear message; empty/unset -> empty Vec.
- Test startup ordering via a tracing-capture test if practical (or just code review against HANDOFF section 3).

## Verify
- cargo run -p soltrace-live -- --help shows --onchain-programs
- cargo run -p soltrace-backfill -- --help shows --onchain-programs
- Invalid base58 -> non-zero exit with clear error
- Existing operators (no flag) -> identical behavior to today
- cargo test --workspace green

## Blocked

Cannot proceed — all upstream dependencies are unmet:

- `soltrace-nqfd` (add deps flate2/arc-swap/spl-program-metadata-client): todo
- `soltrace-n63g` (`onchain_idl.rs` PDA+decode+fetch): todo — `load_onchain_idls` doesn't exist
- `soltrace-nc4t` (EventDecoder arc-swap signature change): todo — `EventDecoder` still takes `IdlParser` directly
- `soltrace-tahu` (`load_onchain_idls` in utils.rs): todo
- `soltrace-dq3b` (`spawn_idl_subscription_task` / `idl_subscription.rs`): todo

Steps (c), (e), (f), (g) in the bean body all reference code that hasn't been written yet. Re-dispatch when Epic 1 + Epic 2 land.
