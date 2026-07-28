---
# soltrace-tahu
title: Implement load_onchain_idls in soltrace-core/src/utils.rs
status: completed
type: task
priority: normal
created_at: 2026-07-28T08:31:17Z
updated_at: 2026-07-28T10:26:27Z
parent: soltrace-ipo0
blocked_by:
  - soltrace-n63g
---

See MS soltrace-z9wq HANDOFF.

## Cut

In soltrace-core/src/utils.rs (sibling to existing load_idls at lines 19-51):

pub async fn load_onchain_idls(parser: &mut IdlParser, rpc: &RpcClient, programs: &[Pubkey]) {
for program in programs {
match fetch_canonical_idl(rpc, program) {
Ok(Some(idl)) => {
info!(program = %program, "fetched on-chain IDL");
parser.insert_or_replace(idl);
}
Ok(None) => warn!(program = %program, "no canonical Direct on-chain IDL"),
Err(e) => warn!(program = %program, error = %e, "failed to fetch on-chain IDL"),
}
}
}

Match the warn-and-continue pattern of existing load_idls (utils.rs:39-42).

## TDD

- Test with stubbed fetch_canonical_idl (inject via trait or feature-flag — or simply unit-test by extracting the body into a helper that takes a closure). Simplest: refactor the loop body into a helper that accepts a fetcher closure, test the helper with a fake fetcher.

## Verify

- cargo test -p soltrace-core green
- Function compiles, imported into lib.rs

## Summary of Changes

- Added `load_onchain_idls(parser, rpc, programs)` to `soltrace-core/src/utils.rs` —
  one-shot startup fetch that mirrors `load_idls`'s warn-and-continue per-program
  resilience (Ok(Some) insert, Ok(None) warn, Err warn, never propagates).
- Factored the loop body into `load_onchain_idls_with(parser, programs, fetch: F)` —
  a private helper taking a `FnMut(&Pubkey) -> Result<Option<ParsedIdl>>` closure so
  the warn-and-continue logic is unit-testable without an RPC.
- Sync, not async — `fetch_canonical_idl` is a blocking RPC with no `.await`;
  making `load_onchain_idls` async would trip `clippy::unused_async`.
- Re-exported `load_onchain_idls` from `soltrace-core/src/lib.rs`.
- 6 new unit tests in `utils::tests` (Ok(Some) insert, Ok(None) skip, Err skip,
  mixed results, empty programs no-op, same-address overwrite).
