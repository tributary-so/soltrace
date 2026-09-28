---
# soltrace-gr49
title: Fix on-chain IDL decode for padded program-metadata accounts (anchor 1.x)
status: completed
type: bug
priority: normal
created_at: 2026-09-28T10:31:07Z
updated_at: 2026-09-28T10:39:26Z
---

On devnet, program can5ZhfgQpi7jymkxE7uEv4ZVm3X2f51KThTUtdWrFs fails with `IO error: corrupt deflate stream`.

## Root cause (verified against the live account)

The canonical program-metadata layout (per the repo's own codama idl.json) pads 5 zero bytes after the u32 `dataLength` (postOffsetTypeNode offset:5, padded). Anchor >= 1.x writes this layout. The generated Rust client (spl-program-metadata-client @ 75ea1f14, unchanged at upstream main) does NOT render that pad: its `data: TrailingVec<u8>` starts right after the u32, so `meta.data` = 5 NUL bytes + zlib stream. ZlibDecoder chokes on the leading zeros -> corrupt deflate stream.

Devnet evidence: account 5wvLVHdhpECNKiohJZ3qHdWAa9vjxeQmgYuvn7sRnnwa is 15,665B; u32 len @87..91 = 15,569; 5 NULs @91..96; zlib payload @96..15,665 (exactly 15,569B) inflates to the IDL JSON.

## Fix

`decode_metadata_account`: use `data_length` as authoritative and slice the payload from the TAIL of `meta.data` (works for padded and unpadded writers).

- [x] Reproduce / root-cause against live devnet account
- [x] Tail-slice fix in decode_metadata_account
- [x] Regression unit test for the padded (anchor 1.x) layout
- [x] cargo fmt + test green (clippy has 11 pre-existing errors at HEAD in db/queue/utils — unrelated files, left alone)
- [x] End-to-end verified: fetch_canonical_idl against live devnet decodes OK (10 events)

## Summary of Changes

- soltrace-core/src/onchain_idl.rs: decode_metadata_account now slices the payload from the tail of meta.data using the authoritative data_length, tolerating the 5-byte codama post-offset pad that anchor >= 1.x writes and the pinned spl-program-metadata-client (75ea1f14, also unchanged at upstream main) drops.
- New regression test decode_canonical_padded_layout_round_trips.
- Verified live: fetch_canonical_idl on devnet program can5Zhfg... returns the IDL (10 events).
