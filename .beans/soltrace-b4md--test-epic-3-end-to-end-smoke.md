---
# soltrace-b4md
title: Test Epic 3 (end-to-end smoke)
status: todo
type: task
created_at: 2026-07-28T08:32:14Z
updated_at: 2026-07-28T08:32:14Z
parent: soltrace-87oa
blocked_by:
    - soltrace-c2ho
---

See MS soltrace-z9wq HANDOFF.

## Cut

End-to-end smoke verification. If solana-test-validator is available in CI:
- Spin up a local validator
- Deploy program-metadata (or use the deployed IDL on devnet for a known program)
- Initialize a canonical 'idl' Metadata account for a test program with a small known IDL (Zlib+Utf8+Direct)
- Run soltrace-live --onchain-programs <test_program> --rpc <local> --ws <local>
- Verify log shows 'fetched on-chain IDL' for the test program
- Trigger a SetData on the IDL account -> verify log shows arc-swap
- Trigger a Close -> verify warn + drop

If localnet is too heavy for CI, mark #[ignore] and provide manual repro instructions in the bean.

Also verify regression:
- Run soltrace-live with no --onchain-programs -> behavior identical to pre-change (compare log output structure).

## Verify
- Either automated #[ignore] test or manual run with attached log output
- Regression: zero behavioral change when flag absent
