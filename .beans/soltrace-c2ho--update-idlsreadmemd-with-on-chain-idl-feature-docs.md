---
# soltrace-c2ho
title: Update idls/README.md with on-chain IDL feature docs
status: completed
type: task
priority: normal
created_at: 2026-07-28T08:32:14Z
updated_at: 2026-07-30T13:57:16Z
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

## Summary of Changes

Added an "On-chain IDLs (`--onchain-programs`)" section to idls/README.md, matching the existing operator-facing voice. Documents all six required points:

1. The --onchain-programs / ONCHAIN_PROGRAMS flag (CLI + env usage, CLI wins, invalid base58 hard-errors, empty/unset = no-op).
2. Priority model: file IDLs win (Decision 1) — on-chain fetch only applies to programs not installed as files.
3. On-chain model: canonical `"idl"`+zero-pad-16-seed PDA under the program-metadata program (ProgM6...), with a link to the program-metadata README.
4. Direct-only limitation (Decision 5): Url/External skipped with a warning.
5. Backfill semantics (Decision 7): point-in-time one-shot fetch, no live updates, same staleness risk as file IDLs.
6. Link to program-metadata README for operator reference.

Plus a live-vs-backfill subsection (hot-swap / SetImmutable unsubscribe / Close drop+warn / capped-backoff reconnect) and the logs_subscribe chicken-and-egg note, so an operator can run the feature from README + --help alone.
