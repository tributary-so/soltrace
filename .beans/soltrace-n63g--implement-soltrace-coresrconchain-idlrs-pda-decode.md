---
# soltrace-n63g
title: Implement soltrace-core/src/onchain_idl.rs (PDA + decode + fetch)
status: todo
type: task
created_at: 2026-07-28T08:31:17Z
updated_at: 2026-07-28T08:31:17Z
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
   - if meta.program != *expected_program { return Ok(None) }
   - if !matches!(meta.data_source, DataSource::Direct) { return Ok(None) }  // v1: Direct only
   - inflate(meta.compression, &meta.data)?  // flate2 for Zlib/Gzip, passthrough for None
   - decode_bytes(meta.encoding, &bytes)?    // Utf8=from_utf8, Base58=bs58, Base64=base64, None=passthrough
   - serde_json::from_str::<ParsedIdl>(&text)?
   - Return Ok(Some(parsed))

3. fetch_canonical_idl(rpc: &RpcClient, program: &Pubkey) -> Result<Option<ParsedIdl>>
   - let pda = derive_canonical_idl_pda(program)
   - match rpc.get_account(&pda) { Ok(a) => decode_metadata_account(&a, program), Err(_) => Ok(None) }

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
