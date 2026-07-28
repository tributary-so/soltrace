---
# soltrace-5jvf
title: Review Epic 1
status: todo
type: task
created_at: 2026-07-28T08:31:17Z
updated_at: 2026-07-28T08:31:17Z
parent: soltrace-ipo0
blocked_by:
    - soltrace-as68
---

See MS soltrace-z9wq HANDOFF.

## Cut

Code review of Epic 1 deliverables. Verify:
- Ponytail compliance: no speculative abstractions, no factories for single impls, no config knobs without consumers. ponytail: comments on deliberate shortcuts.
- Public API matches HANDOFF contract (signatures, return types, None vs Err semantics).
- TDD: every public fn has tests, fixtures are not production code.
- No new compiler warnings (cargo build --workspace --all-features 2>&1 | grep -i warning).
- File IDL precedence enforced (Decision 1).
- Direct-only data_source enforced (Decision 5).
- Canonical-only PDA (Decision 4) — derive_canonical_idl_pda seeds match program-metadata/program/src/processor/initialize.rs:68-79 byte-for-byte.

Report findings as comments in this bean. Block on critical issues, approve on minor.
