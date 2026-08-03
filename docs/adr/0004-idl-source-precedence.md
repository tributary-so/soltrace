# IDL source precedence: the file IDL always wins

Status: accepted

When a program has an IDL from more than one source, soltrace resolves the
conflict with a fixed precedence: **file IDL > program-metadata IDL > classic
Anchor IDL**, and file wins absolutely — if a file IDL exists for a program in
`IDLS_DIR`, no on-chain IDL is fetched for it at all.

The rejected alternative was "newest wins" (prefer the on-chain account when
it is more recent than the operator's file). It was rejected because the
operator's local file is the authoritative source of truth they signed off on:
silently overriding it with whatever happens to be on-chain would make
decoding non-deterministic from the operator's point of view and could mask a
malicious or buggy IDL re-publish. Predictable operator control beats fresh.

## Consequences

- To force an on-chain IDL for a program, remove (or don't create) its file in
  `IDLS_DIR`; there is no "prefer on-chain" flag.
- Within the on-chain tier, program-metadata (modern Anchor ≥0.31) is tried
  before classic Anchor (≤0.30), so the common modern case costs one fewer RPC
  call; classic is the fallback for older publications.
- A program named in `--program-prefixes` with no IDL from any source is
  skipped (not indexed), since its events cannot decode — auto-discovery probes
  both on-chain standards so the operator need not say which one a program
  uses.
