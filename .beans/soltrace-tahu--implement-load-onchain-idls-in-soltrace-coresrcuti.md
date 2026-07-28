---
# soltrace-tahu
title: Implement load_onchain_idls in soltrace-core/src/utils.rs
status: todo
type: task
created_at: 2026-07-28T08:31:17Z
updated_at: 2026-07-28T08:31:17Z
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
