---
# soltrace-nqfd
title: 'Add deps: flate2, arc-swap, spl-program-metadata-client'
status: completed
type: task
priority: normal
created_at: 2026-07-28T08:31:17Z
updated_at: 2026-07-28T09:43:52Z
parent: soltrace-ipo0
blocked_by:
    - soltrace-1eto
---

See MS soltrace-z9wq HANDOFF.

## Cut

Add to soltrace-core/Cargo.toml [dependencies]:

- flate2 = "1" # inflate Metadata.data when compression == Zlib/Gzip
- arc-swap = "1" # lock-free Arc<ArcSwap<IdlParser>> hot-swap
- spl-program-metadata-client = { path = "../../program-metadata/clients/rust", features = ["fetch"] }
- solana-rpc-client (only if not already direct/transitive — check first)

The path-dep is RELATIVE to soltrace-core/Cargo.toml. Verify the path resolves.

## Verify

- cargo build -p soltrace-core --all-features green
- No new warnings
- cargo tree -p soltrace-core | grep -E '(flate2|arc-swap|spl-program-metadata-client)' shows all three

## Blocker (environmental — cannot complete as specified)

The path-dep `../../program-metadata/clients/rust` (relative to
`soltrace-core/Cargo.toml`) **does not resolve** — the `program-metadata/`
source tree is absent from this worktree, its git history, every sibling
worktree (`soltrace-87oa/z9wq/sfg2`), and the cargo registry/git cache.

### Diagnosis

- **In-repo?** No. `git log --all` has no record of `program-metadata/`; no
  `.gitmodules`. The MS HANDOFF calls it "in-repo (read-only)" but it was
  never vendored into the soltrace repo.
- **Published crate?** No. `crates.io/crates/spl-program-metadata-client` → 404.
- **Upstream located:** `github.com/solana-program/program-metadata` (crate
  name matches: `spl-program-metadata-client`, `fetch` feature exists).
  `git ls-remote` over HTTPS works (HEAD `75ea1f14`).
- **Transitives:** `flate2` + `arc-swap` already resolve in `Cargo.lock` —
  those two adds are zero-risk. Only the metadata-client path-dep is stuck.

### Why I did NOT silently work around it

1. Bean explicitly says "Verify the path resolves." It doesn't — that's a
   finding to report, not paper over.
2. **Type-compat cliff:** upstream `clients/rust/Cargo.toml` pins
   `solana-sdk 2.3`, `solana-account 3.0`, `solana-instruction 3.2`,
   `solana-rpc-client 3.0`. This workspace pins `solana-sdk 4.0`. A git-URL
   fallback builds, but the client's types won't be type-compatible with
   soltrace's `solana_sdk`/`solana_rpc_client` types — the integration beans
   (soltrace-n63g/nc4t) would hit this immediately.
3. Vendoring a third-party repo into `../../program-metadata` (a shared parent
   across all sibling worktrees) is an irreversible environment change the
   operator should approve, not an agent call.

### Options for the operator (pick one)

- **A. Vendor** `solana-program/program-metadata` at
  `/home/xeroc/.herdr/worktrees/soltrace/program-metadata` (makes the path-dep
  work as written; shared across lanes; pick a ref compatible with
  `solana-sdk 4.0` or accept the type-compat work in n63g/nc4t).
- **B. Switch dep source** to `git = "https://github.com/solana-program/program-metadata",
subdirectory = "clients/rust", rev = "..."` (deviates from bean CUT; same
  type-compat cliff; re-pin in this bean's Cargo.toml line).
- **C. Fork/pin a compatible ref** of program-metadata that targets
  `solana-sdk 4.0` (cleanest for types; most setup).

### Recommendation

**A** with a rev pin, plus an explicit `// ponytail:` note in n63g that the
metadata client's `solana-sdk` types are a different major version and must be
marshalled at the boundary (not passed through). This is the only option that
honours the bean's literal `path =` spec.

### Effect on the epic

This bean is the deps-add prerequisite for soltrace-n63g, nc4t, tahu (all
todo). They are all blocked on this resolution. Draft prerequisite bean
created: see `soltrace-<id>` (blocking this one).

`hordr blocked` is not a registered command in this hordr build, so the lane
cannot be released programmatically — bean left `in-progress` with this note.


## Summary of Changes

**Blocker resolved (Option A — vendor).** See soltrace-1eto. The
`program-metadata` tree was cloned to the shared worktree parent; the path-dep
now resolves.

**Edits (soltrace-core/Cargo.toml [dependencies]):**
- `flate2 = "1"` — inflate `Metadata.data` when compression == Zlib/Gzip
- `arc-swap = "1"` — lock-free `Arc<ArcSwap<IdlParser>>` hot-swap
- `spl-program-metadata-client = { path = "../../program-metadata/clients/rust", features = ["fetch"] }`

**Held back (ponytail — no speculative deps):** `solana-rpc-client` is NOT
added here. It becomes transitive via the `fetch` feature, and soltrace-core
has no `use solana_rpc_client::` site yet. soltrace-n63g will add it direct
when it writes `fetch_canonical_idl(rpc: &RpcClient, ...)`.

**Verify:**
- `cargo build -p soltrace-core --all-features` — green (metadata client
  compiles from vendored path; a second `solana-sdk 2.x` major version
  coexists with the workspace's 4.0 — no conflict at the dep-graph level).
- `cargo tree -p soltrace-core` — all three appear as direct deps
  (flate2 v1.1.9, arc-swap v1.8.2, spl-program-metadata-client v0.0.0).
- Clippy: ZERO new warnings. Pre-existing `needless_borrows_for_generic_args`
  failures in `src/db/sqlite.rs` (`hex::encode(&id_bytes)`) are unrelated and
  present on the base commit — out of scope (surgical principle).

**Landmine for downstream (soltrace-n63g/nc4t):** the vendored client's public
types are `solana-sdk 2.x`-typed; soltrace is `solana-sdk 4.0`-typed. Marshal
at the boundary, do not pass-through.
