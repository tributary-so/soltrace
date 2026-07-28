---
# soltrace-as68
title: Test Epic 1 (core primitives integration)
status: todo
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
- cargo test -p soltrace-core --test '*' green for the new integration test
- cargo test --workspace green (no regressions)
