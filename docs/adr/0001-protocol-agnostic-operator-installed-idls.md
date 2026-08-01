# Protocol-agnostic by default: no IDLs are vendored

Status: accepted

Soltrace ships with an empty `idls/` directory and decodes any Anchor program's
events from IDLs the **operator** installs — it deliberately knows nothing
about any specific Solana program. The alternative, a purpose-built indexer
that vendors the IDL(s) for one protocol, was rejected: it would couple every
release to upstream program changes and re-derive the same decode/extract
plumbing per protocol. Protocol-agnosticity puts the burden on the operator
(fetch the IDL, map program→prefix) in exchange for a single binary that indexes
any Anchor program.

## Consequences

- Adding a new program is purely operational (drop an IDL in `IDLS_DIR` or list
  it for on-chain fetch); no code change, no release.
- Soltrace never ships IDL JSON in the repo — `idls/` is operator-installed and
  read exactly once at startup. Changes mid-run require a restart.
- Decode failures (unknown discriminator, borsh mismatch) are surfaced, not
  special-cased per program — see ADR-0004 for how IDL source conflicts resolve.
