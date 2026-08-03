---
# soltrace-n4g3
title: Support classic Anchor on-chain IDLs (auto-discovery)
status: completed
type: feature
priority: normal
created_at: 2026-08-01T22:28:55Z
updated_at: 2026-08-01T22:35:23Z
---

Close the on-chain IDL gap: programs whose IDL was published via the classic Anchor mechanism (anchor idl init, pre-program-metadata) are not fetched. Add classic Anchor IDL decode (PDA of the program itself, zlib-compressed) alongside the existing program-metadata path, and auto-probe on-chain for every --program-prefixes program lacking a file IDL — no extra flag required.

## Scope
- Classic Anchor IDL account: PDA = create_with_seed(PDA(&[]), "anchor:idl", program), owner = program. Layout: [disc(8)][authority(32)][data_len u32 LE][zlib IDL JSON]. Source: anchor v0.30.1 cli/src/lib.rs:fetch_idl.
- Unified fetch_onchain_idl: try program-metadata canonical PDA first, then classic Anchor; first Some wins.
- Auto-candidate set: derive on-chain candidates from --program-prefixes (no file IDL) in both binaries; keep --onchain-programs as explicit override + live hot-swap trigger.
- Pre-0.30 legacy IDL shape is OUT OF SCOPE (deferred).

## TDD checklist
- [x] derive_anchor_classic_idl_pda matches Anchor formula (test)
- [x] decode_anchor_classic_account zlib round-trip (test)
- [x] decode_anchor_classic_account too-short -> None (test)
- [x] decode_anchor_classic_account bad zlib -> Err (test)
- [x] fetch_onchain_idl returns None when both PDAs absent (test)
- [x] load_onchain_idls uses unified fetch
- [x] backfill: auto-candidate from PROGRAM_PREFIXES
- [x] live: auto-candidate from PROGRAM_PREFIXES (preserve hot-swap on explicit --onchain-programs)
- [x] tests green (84 pass); new code fmt+clippy clean (pre-existing repo drift flagged, not touched)

## Summary of Changes

Classic Anchor on-chain IDL decode added alongside the existing program-metadata path; both binaries auto-probe on-chain for every --program-prefixes program lacking a file IDL.

**soltrace-core/src/onchain_idl.rs**
- derive_anchor_classic_idl_pda: create_with_seed(PDA(&[]), "anchor:idl", program) — mirrors anchor-lang IdlAccount::address
- decode_anchor_classic_account: strips [disc(8)][authority(32)][data_len u32 LE], zlib-decompresses, serde_json parse. Returns None when too short / truncated; Err on zlib/JSON failure
- fetch_anchor_classic_idl: owner==program gate (classic accounts are PDAs of the program itself)
- fetch_onchain_idl: unified — program-metadata canonical PDA first, classic fallback, first Some wins
- 5 new tests mirroring build_metadata_account pattern

**soltrace-core/src/utils.rs** — load_onchain_idls now calls fetch_onchain_idl (tries both standards)

**soltrace-core/src/lib.rs** — export derive_anchor_classic_idl_pda, decode_anchor_classic_account, fetch_onchain_idl

**soltrace-{live,backfill}/src/main.rs** — candidate set = (program-prefixes ids without a file IDL) ∪ (explicit --onchain-programs); probed via the unified fetch. --onchain-programs retained as explicit override and remains the trigger for live's accountSubscribe hot-swap.

Pre-0.30 legacy IDL shape deferred (out of scope).
