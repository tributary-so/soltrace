---
# soltrace-nqfd
title: 'Add deps: flate2, arc-swap, spl-program-metadata-client'
status: todo
type: task
created_at: 2026-07-28T08:31:17Z
updated_at: 2026-07-28T08:31:17Z
parent: soltrace-ipo0
---

See MS soltrace-z9wq HANDOFF.

## Cut

Add to soltrace-core/Cargo.toml [dependencies]:
- flate2 = "1"           # inflate Metadata.data when compression == Zlib/Gzip
- arc-swap = "1"         # lock-free Arc<ArcSwap<IdlParser>> hot-swap
- spl-program-metadata-client = { path = "../../program-metadata/clients/rust", features = ["fetch"] }
- solana-rpc-client (only if not already direct/transitive — check first)

The path-dep is RELATIVE to soltrace-core/Cargo.toml. Verify the path resolves.

## Verify
- cargo build -p soltrace-core --all-features green
- No new warnings
- cargo tree -p soltrace-core | grep -E '(flate2|arc-swap|spl-program-metadata-client)' shows all three
