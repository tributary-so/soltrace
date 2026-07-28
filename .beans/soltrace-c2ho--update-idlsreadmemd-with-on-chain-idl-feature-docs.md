---
# soltrace-c2ho
title: Update idls/README.md with on-chain IDL feature docs
status: todo
type: task
created_at: 2026-07-28T08:32:14Z
updated_at: 2026-07-28T08:32:14Z
parent: soltrace-87oa
blocked_by:
    - soltrace-hb6z
---

See MS soltrace-z9wq HANDOFF.

## Cut

Update idls/README.md to document:
1. The new --onchain-programs / ONCHAIN_PROGRAMS flag.
2. The priority model: file IDLs win over on-chain (Decision 1).
3. The on-chain model in brief: canonical 'idl'-seed PDA via program-metadata program.
4. The Direct-only limitation (Decision 5): non-Direct data_sources are skipped with a warning.
5. The backfill semantics: one-shot fetch at startup, no live updates (Decision 7), same staleness as file IDLs.
6. Link to program-metadata README for operator reference.

Match the existing voice/format of idls/README.md (operator-vendored, protocol-agnostic).

## Verify
- A new operator reading idls/README.md + the --help output can use the feature without reading source code.
