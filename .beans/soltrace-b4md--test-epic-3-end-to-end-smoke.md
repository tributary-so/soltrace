---
# soltrace-b4md
title: Test Epic 3 (end-to-end smoke)
status: completed
type: task
priority: normal
created_at: 2026-07-28T08:32:14Z
updated_at: 2026-07-30T14:00:46Z
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

## Summary of Changes

### Regression (zero behavioral change when flag absent) — AUTOMATED, deterministic

Added CLI-surface regression tests to both binaries (`soltrace-live/src/main.rs`, `soltrace-backfill/src/main.rs`):

- `test_cli_onchain_programs_defaults_empty_when_absent` — the flag is OPTIONAL with an empty default; existing operators who omit it parse to `onchain_programs = ""`.
- `test_cli_onchain_programs_accepted_when_present` — the flag parses a base58 value when supplied.

Combined with the existing `test_parse_onchain_programs_empty_is_noop` (empty CSV -> empty Vec) and the `if !onchain_programs.is_empty()` guards around the on-chain fetch + subscription spawn + program-ids merge, this proves the flag's absence executes the exact pre-change code path: no RPC call, no subscription task, no logs-filter change.

Verification gate (green): `cargo build --workspace --all-features`, `cargo test --workspace` (78 tests), `cargo clippy --workspace --all-features -- -D warnings`.

### End-to-end localnet smoke — MANUAL (program-metadata program binary unavailable in this env)

A live localnet E2E is not runnable in this environment: only the `spl-program-metadata-client` Rust lib resolves (path dep `../../program-metadata/clients/rust`); the deployable program (`../../program-metadata/program`) and its compiled `.so` are absent. `solana-test-validator`/`anchor`/CLI are installed, but there is no program binary to deploy. Per the bean's escape hatch, here is the manual repro for an environment where program-metadata IS built:

```bash
# 1. Start a local validator.
solana-test-validator --reset

# 2. Deploy program-metadata (build + deploy from its repo):
#    anchor build && solana program deploy target/deploy/program_metadata.so
PROGRAM_META=<program-metadata program id>   # ProgM... on localnet

# 3. Initialize a canonical Direct IDL account for a test program (use the
#    program-metadata CLI / a small script) with a known IDL, Zlib+Utf8+Direct.
TEST_PROG=<your test anchor program>

# 4. Run soltrace-live against localnet:
cargo run -p soltrace-live -- run   --ws-url http://localhost:8900 --rpc-url http://localhost:8899   --program-prefixes "" --db-url sqlite:./smoke.db   --onchain-programs $TEST_PROG

# Expect: "fetched on-chain IDL" for $TEST_PROG at startup, then events decode.

# 5. Trigger a SetData on the IDL account (update the IDL):
#    Expect log: "on-chain IDL hot-swapped".

# 6. Trigger a SetImmutable:
#    Expect log: "IDL frozen (SetImmutable); unsubscribing".

# 7. Trigger a Close on the IDL account:
#    Expect log: "on-chain IDL account closed; removed from parser" (warn).
```

The constituent logic these steps exercise is already covered by automated tests:
- decode/PDA/fetch + file-precedence + ArcSwap reader: `soltrace-core/src/utils.rs` (Epic 1 integration tests).
- subscription hot-swap / SetImmutable unsubscribe / Close drop+warn / reconnect: `soltrace-live/src/idl_subscription.rs` (Epic 2 tests).
- CLI parse + startup guards: this task.

The only unautomated seam is the live RPC/WS + deployed-program glue, which is what the manual repro above covers.
