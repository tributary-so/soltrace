---
# soltrace-1eto
title: 'Resolve program-metadata source: vendor path-dep vs git-URL vs compatible fork (blocks soltrace-nqfd + epic)'
status: draft
type: task
priority: critical
created_at: 2026-07-28T08:51:52Z
updated_at: 2026-07-28T08:52:24Z
parent: soltrace-ipo0
---

(parent: soltrace-ipo0 epic; blocks: soltrace-nqfd and transitively n63g/nc4t/tahu)

## Decision needed (operator)

`spl-program-metadata-client` must become available to `soltrace-core`. The MS
HANDOFF assumes `program-metadata/` is vendored in-repo; it is not.

### Options
- **A. Vendor** `github.com/solana-program/program-metadata` at
  `<worktree-parent>/program-metadata` so the bean's literal
  `path = "../../program-metadata/clients/rust"` resolves.
- **B. Git-URL dep** in `soltrace-core/Cargo.toml`:
  `git = "https://github.com/solana-program/program-metadata", subdirectory = "clients/rust", rev = "<pin>"`.
- **C. Fork** a ref retargeted to `solana-sdk 4.0`.

### Known landmine (applies to A and B)
Upstream `clients/rust/Cargo.toml` pins `solana-sdk 2.3` / `solana-account 3.0`
/ `solana-instruction 3.2` / `solana-rpc-client 3.0`. This workspace uses
`solana-sdk 4.0`. Cargo will resolve two major versions; the metadata client's
public types (`Pubkey`, `Account`, `RpcClient`) will NOT be type-compatible
with soltrace's. Integration beans must marshal at the boundary.

### When resolved
Flip this to `completed`, then re-dispatch `soltrace-nqfd` — it will add all
three deps (flate2, arc-swap, spl-program-metadata-client) and verify in one
commit.
