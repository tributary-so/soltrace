---
# soltrace-5jvf
title: Review Epic 1
status: completed
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

## Summary of Changes

**Verdict: APPROVE.** No critical issues. Epic 1 is production-ready.

### Checklist results

1. **Ponytail compliance** — PASS

   - No speculative abstractions. `load_onchain_idls_with` split is for testability (genuine consumer).
   - `ponytail:` comments present on deliberate shortcuts (`decode_metadata_account` taking `&[u8]` not `&Account`; precedence guard).
   - No factories for single impls, no config knobs without consumers.

2. **Public API vs HANDOFF** — PASS (2 justified deviations)

   - `decode_metadata_account(data: &[u8], ...)` vs HANDOFF `&Account` — justified by `ponytail:` comment: `spl-program-metadata-client` pins solana-sdk 2.x, workspace pins 4.0; `.data` is all we read.
   - `load_onchain_idls` sync vs HANDOFF `async` — justified: `fetch_canonical_idl` is blocking, `async` with no `.await` trips `clippy::unused_async`.
   - All other signatures match: `derive_canonical_idl_pda`, `fetch_canonical_idl`, `insert_or_replace`, `remove`.
   - None vs Err semantics correct: RPC failure → `Ok(None)`; decode failure → `Err`; non-Direct → `Ok(None)`.

3. **TDD** — PASS

   - Every public fn has tests: `derive_canonical_idl_pda` (2 tests), `decode_metadata_account` (7 tests), `fetch_canonical_idl` (1 test), `insert_or_replace` (1 test), `remove` (1 test), `load_onchain_idls_with` (7 tests), ArcSwap hot-reload (1 test + concurrent test in event.rs).
   - All fixtures in `#[cfg(test)]` modules.

4. **Compiler warnings** — PASS

   - `cargo build --workspace --all-features` produces zero warnings.
   - 3 pre-existing clippy `needless_borrows_for_generic_args` in `db/sqlite.rs` (not Epic 1 scope).

5. **File IDL precedence (Decision 1)** — PASS

   - Guard in `load_onchain_idls_with`: `parser.get_idls().contains_key(...)` → skip + debug log. Fetcher never called for existing IDLs. Tested by `test_load_onchain_idls_preserves_existing_idl`.

6. **Direct-only data_source (Decision 5)** — PASS

   - `decode_metadata_account`: `if !matches!(meta.data_source, DataSource::Direct) { return Ok(None); }`. Tested by `decode_non_direct_data_source_returns_none`.

7. **Canonical-only PDA (Decision 4)** — PASS
   - `derive_canonical_idl_pda`: seeds `[program.as_ref(), "idl"+pad16]` under `PROGRAM_METADATA_ID`.
   - Cross-checked against `program-metadata/program/src/processor/initialize.rs:68-69`: `derive_program_address(&[program.address().as_array(), args.seed.as_ref()], &ID)` — identical seed layout.
   - Tests verify byte-for-byte match + authority-scoped empty-form equivalence.

### Minor observations (non-blocking)

- Pre-existing production `unwrap()` in `event.rs:36` (`data[..8].try_into().unwrap()`) and `utils.rs:34` (`path.to_str().unwrap()`) — both pre-Epic-1, both guarded by context (length check / OS path semantics). Out of scope.
- `PROGRAM_METADATA_ID` constant matches HANDOFF: `ProgM6JCCvbYkfKqJYHePx4xxSUSqJp7rh8Lyv7nk7S`.
