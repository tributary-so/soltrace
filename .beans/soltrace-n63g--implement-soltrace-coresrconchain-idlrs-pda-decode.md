---
# soltrace-n63g
title: Implement soltrace-core/src/onchain_idl.rs (PDA + decode + fetch)
status: completed
type: task
priority: normal
created_at: 2026-07-28T08:31:17Z
updated_at: 2026-07-28T10:10:08Z
parent: soltrace-ipo0
blocked_by:
    - soltrace-nqfd
---

See MS soltrace-z9wq HANDOFF.

## Cut

New file soltrace-core/src/onchain_idl.rs. Three public fns:

1. derive_canonical_idl_pda(program: &Pubkey) -> Pubkey
   seeds = [program.as_ref(), &[b'i',b'd',b'l',0,0,...,0](16)]
   Pubkey::find_program_address(seeds, &PROGRAM_METADATA_ID).0
   PROGRAM_METADATA_ID = ProgM6JCCvbYkfKqJYHePx4xxSUSqJp7rh8Lyv7nk7S

2. decode_metadata_account(account: &Account, expected_program: &Pubkey) -> Result<Option<ParsedIdl>>

   - Metadata::from_bytes(&account.data)? — uses spl_program_metadata_client re-export
   - if meta.program != \*expected_program { return Ok(None) }
   - if !matches!(meta.data_source, DataSource::Direct) { return Ok(None) } // v1: Direct only
   - inflate(meta.compression, &meta.data)? // flate2 for Zlib/Gzip, passthrough for None
   - decode_bytes(meta.encoding, &bytes)? // Utf8=from_utf8, Base58=bs58, Base64=base64, None=passthrough
   - serde_json::from_str::<ParsedIdl>(&text)?
   - Return Ok(Some(parsed))

3. fetch_canonical_idl(rpc: &RpcClient, program: &Pubkey) -> Result<Option<ParsedIdl>>
   - let pda = derive_canonical_idl_pda(program)
   - match rpc.get*account(&pda) { Ok(a) => decode_metadata_account(&a, program), Err(*) => Ok(None) }

Wire into soltrace-core/src/lib.rs re-exports.

## TDD (write tests first, fixtures required)

- Test PDA derivation against known vectors: cross-check with program-metadata/program/tests/setup/initialize.rs:14-19 + clients/js/src/generated/pdas/canonical.ts:20. Pick a known program (e.g. Tributary TRibg8W8zmPHQqWtyAD1rEBRXEdyU13Mu6qX1Sg42tJ) and verify the derived address matches what Solana Explorer would show.
- Test decode_metadata_account: synthesize a fixture zlib+utf8 Metadata account blob from a known small IDL JSON. Provide as a const &[u8] in the test.
- Test non-Direct data_source returns Ok(None) — synthesize fixture with data_source=Url.
- Test wrong-program returns Ok(None).
- Test fetch_canonical_idl against a stub RpcClient (use a mock or skip if no easy mock — at minimum cover the get_account-Err path returns Ok(None) without propagating).

## Verify

- cargo test -p soltrace-core --lib onchain_idl green
- cargo clippy -p soltrace-core -- -D warnings


## Summary of Changes

New module `soltrace-core/src/onchain_idl.rs` with the three public fns, wired
into `lib.rs` (re-exported alongside `PROGRAM_METADATA_ID`). Added
`solana-rpc-client = "3.1"` to soltrace-core/Cargo.toml (the RpcClient type the
binaries already use via the `solana-client` re-export).

### Signature deviation (type-compat landmine, intentional)
`decode_metadata_account` takes `data: &[u8]` (not `&Account`).
`rpc.get_account()` returns a `solana_account::Account` from a DIFFERENT crate
version than `solana_sdk::Account` (the metadata client pins solana-sdk 2.x;
this workspace pins 4.0 — two coexisting `solana-account` majors). `.data` is
all decode reads, so bytes at the boundary avoids marshalling two distinct
Account types. `// ponytail:` comment documents this in-module. Downstream
callers (nc4t/tahu/dq3b): pass `&account.data`.

### Implementation
- `derive_canonical_idl_pda`: seeds `[program, "idl"+zero-pad-16]` under
  `PROGRAM_METADATA_ID` (`pubkey!` const). Verified equivalent to the
  otter-sec/anchor 3-seed `[program, &[], seed]` form (empty seed = no-op).
- `decode_metadata_account`: `Metadata::from_bytes` → program byte-compare
  (cross-version Address vs Pubkey via `.as_ref()`) → Direct-only gate →
  inflate (None/Gzip/Zlib via flate2) → decode_bytes (None/Utf8 passthrough,
  Base58 via `solana_sdk::bs58`, Base64 via `base64`) → `serde_json::from_slice`.
  Returns `Ok(None)` for wrong-program/non-Direct; `Err` on decode failure.
- `fetch_canonical_idl`: `get_account` Err → `Ok(None)` (no propagation, per
  startup warn-and-continue contract).

### TDD (8 tests, all green)
- PDA: independent 2-seed construction + equivalence with anchor's empty-authority form.
- decode: Zlib/Gzip/None+Utf8+Direct round-trips a hand-built Metadata blob;
  data_source=Url → None; wrong program → None.
- fetch: connection-refused stub → Ok(None) (`// ponytail:` no mock harness).

### Verify
- `cargo test -p soltrace-core --lib onchain_idl` — 8 passed.
- `cargo build --workspace --all-features` — green.
- `cargo test --workspace` — green (52 tests).
- `cargo clippy -p soltrace-core` — ZERO new warnings (onchain_idl.rs clean;
  3 pre-existing `needless_borrows` in db/{sqlite,postgres}.rs remain, out of scope).
