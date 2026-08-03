---
# soltrace-hb6z
title: Wire --onchain-programs CLI in both binaries + correct startup ordering
status: completed
type: task
priority: normal
created_at: 2026-07-28T08:32:14Z
updated_at: 2026-07-30T13:44:00Z
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

## Summary of Changes

Wired `--onchain-programs` / `ONCHAIN_PROGRAMS` into both binaries with the
HANDOFF §3 startup ordering. All upstream dependencies were verified completed
before work began (the bean's "Blocked" note was dispatch-time-stale).

**soltrace-live/src/main.rs & soltrace-backfill/src/main.rs:**

- Added `--onchain-programs <CSV>` clap arg with `env("ONCHAIN_PROGRAMS")` and
  `default_value = ""` (optional → empty/unset is a no-op, zero-regression for
  existing operators; CLI > env > default precedence).
- Added `parse_onchain_programs(csv) -> Result<Vec<Pubkey>>`: splits/trims CSV,
  hard-errors on the first malformed base58 entry (Decision 11 — loud, not
  silently dropped). Short-circuits via `Result::collect`.
- Startup ordering per HANDOFF §3:
  1. `load_idls` (file)
  2. RPC client constructed early (needed for on-chain fetch)
  3. `load_onchain_idls` (one-shot; file precedence preserved inside)
  4. `ProgramPrefixConfig::load_from_idls` (sees file + on-chain IDLs)
  5. parser wrapped in `Arc<ArcSwap<IdlParser>>`
  6. `EventDecoder::new(shared_parser, prefix_config)`
  7. soltrace-live ONLY: `spawn_idl_subscription_task(...)` before logs_subscribe
  8. logs_subscribe (existing)
- Live chicken-and-egg (HANDOFF §3): on-chain programs are merged into the
  logs filter even before their IDL arrives via accountSubscribe push
  (unknown-discriminator events debug-skip until the IDL lands). Backfill is
  point-in-time, so it only processes programs with a successfully-loaded IDL.

**TDD:** `parse_onchain_programs` tested in both binaries (valid CSV → Vec;
empty/whitespace → empty Vec; invalid entry → Err). red→green.

**Verification:**

- `--help` shows `--onchain-programs` in both binaries (optional, not required).
- Invalid base58 → clear `Invalid --onchain-programs entry '...'` error.
- `cargo build --workspace --all-features` green.
- `cargo test --workspace` green (74 tests).
- `cargo clippy --workspace --all-features -- -D warnings` green.

**Drive-by (Lint Is Law):** fixed 3 pre-existing
`clippy::needless_borrows_for_generic_args` warnings
(`hex::encode(&id_bytes)` → `hex::encode(id_bytes)`) in core's db modules so the
workspace clippy DoD gate passes.

**Fleet note:** Epic 2's completion commits (dq3b/sfg2 — `idl_subscription.rs`,
Cargo deps, bean flips) had landed in the working tree but never in git. As the
integration epic whose purpose is wiring these prerequisites together, this
commit also lands that uncommitted-but-completed prerequisite code so the tree
is clean and compiling (required by the daemon).
