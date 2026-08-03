---
# soltrace-as68
title: Test Epic 1 (core primitives integration)
status: completed
type: task
created_at: 2026-07-28T08:31:17Z
updated_at: 2026-07-28T08:31:17Z
parent: soltrace-ipo0
blocked_by:
  - soltrace-nc4t
  - soltrace-tahu
---

See MS soltrace-z9wq HANDOFF.

## Cut

End-to-end integration test of the core path:

- Build a fixture IdlParser with one file IDL.
- Run load_onchain_idls with a stub RPC that returns a fixture zlib+utf8 Metadata account for one program.
- Verify both IDLs are in the parser.
- Wrap parser in Arc<ArcSwap<>>, spawn reader task, swap fresh parser, verify reader observes new state without panic.
- Verify file IDL precedence: load file IDL for program X, then call load_onchain_idls with program X — assert file IDL is preserved (NOT overwritten by on-chain). [Implementer: this precedence check may need to live in load_onchain_idls itself — "skip if address already in parser". Confirm against Decision 1 and add the guard if missing.]

## Verify

- cargo test -p soltrace-core --test '\*' green for the new integration test
- cargo test --workspace green (no regressions)

## Summary of Changes

- **Added file-IDL precedence guard** to `load_onchain_idls_with`: if a program
  is already in the parser (file IDL loaded first), the on-chain fetch is
  skipped entirely (no wasted RPC). Confirmed against Decision 1 ("File
  primary") — this guard was missing from soltrace-tahu.
- **Updated `test_load_onchain_idls_preserves_existing_idl`** — verifies the
  fetcher closure is never called for a program with an existing IDL.
- **Added 2 Epic 1 integration tests** in `utils::tests`:
  - `test_epic1_file_and_onchain_coexist`: file IDL (program A) + on-chain
    Metadata blob decoded via `decode_metadata_account` (program B) coexist.
  - `test_epic1_arcswap_reader_sees_swapped_idl`: empty parser → decode fails →
    ArcSwap store parser with IDL → decode succeeds (hot-reload reader path).
- Metadata blob fixture uses actual enum discriminants, not hardcoded guesses.
