# AGENTS.md

> Context and instructions for AI coding agents working on soltrace.
> Humans should read [README.md](./README.md) first; this file complements it
> with the technical detail an agent needs to contribute without hand-holding.

## Project Overview

Soltrace is a **protocol-agnostic Solana event indexer** written in Rust. It
reads Anchor IDLs supplied by the operator, decodes both `emit!` (log-scraped)
and `emit_cpi!` (inner-instruction-extracted) events, and stores each decoded
occurrence as a row keyed by `(signature, index, event_name)`. Soltrace itself
knows nothing about any specific Solana program — **no IDLs are vendored**.

### Architecture

Cargo workspace, three crates:

```
soltrace-core/      Core library — IDL parsing, borsh decode, DB backends,
                    CPI/log extraction, retry, Kafka queue. Everything reusable.
soltrace-live/      Real-time binary. WebSocket `logsSubscribe` + startup
                    catch-up + on-chain IDL hot-swap subscription.
soltrace-backfill/  Historical binary. Paginated `get_signatures_for_address`
                    walk with concurrent tx fetch.
```

Both binaries are thin CLIs over `soltrace-core`. **Put shared logic in core,
not in the binaries.** A change that only touches a binary's `main.rs` is
usually a smell — the logic probably belongs in core.

### Key data-flow facts (read before touching decode/extract code)

- Two emission paths feed the **same** `EventDecoder`:
  - **log path** (`emit!`): scrape `Program data: <base64>` from `meta.logMessages`
  - **CPI path** (`emit_cpi!`): self-CPI to `["__event_authority"]` in
    `meta.innerInstructions`, with an 8-byte wrapper discriminator
    `0x1d9acb512ea545e4` that must be stripped before decode.
- **Dedup key** is `(signature, index, event_name)`:
  - log-path `index` = sequential counter from 0
  - CPI-path `index` = `1_000_000_000 + outer_ix * 100_000 + inner_ix`
    (offset so the two paths never collide)
- Reprocessing is **idempotent by design** (`ON CONFLICT DO NOTHING`). A
  `UNIQUE constraint failed` error is normal, not a bug.
- `u64`/`u128`/`i64`/`i128` decode to JSON **strings** (lossless); smaller ints
  decode to JSON numbers. Don't "fix" this asymmetry — it's intentional.

### Domain language

[CONTEXT.md](./CONTEXT.md) is the **authoritative ubiquitous-language
glossary**. Use its terms in code, comments, commit messages, and PRs. Key
distinctions an agent routinely gets wrong:

- **EventEmission** (the on-chain act) ≠ **EventPayload** (the bytes) ≠
  **DecodedEvent** (the JSON value) ≠ **Event Row** (the stored row). Don't
  call any of these "event" or "log entry".
- **Inner instruction** ≠ **CPI** (an inner instruction is not necessarily a
  CPI). Don't use "CPI" when you mean "inner instruction".
- **emit_cpi! wrapper discriminator** (`0x1d9acb512ea545e4`) ≠ an event's own
  **discriminator** (`sha256("event:<name>")[..8]`). Two different 8-byte
  values with similar names.
- **Operator** (the person running soltrace) ≠ a downstream consumer of the
  rows. Soltrace never ships IDLs; the operator installs them.

## Setup Commands

Requires **Rust 1.70+** (Dockerfile pins 1.88; edition 2021). No
`rust-toolchain.toml` — uses whatever the toolchain default is.

```bash
# Build everything (release binaries land in target/release/)
cargo build --release

# Build with the optional Kafka fan-out feature
cargo build --release --features kafka

# Build a single crate
cargo build --package soltrace-core
```

### IDLs — do not vendor

The `idls/` directory ships **empty** (only `README.md`). Do **not** add IDL
JSON files to the repo — they are operator-installed, loaded once at startup,
keyed by the IDL's top-level `address` field (filename is free-form). See
[idls/README.md](./idls/README.md) for the load contract and on-chain-IDL
alternative (`--onchain-programs`).

### Running locally

```bash
# 1. Initialize the DB (sqlite default)
./target/release/soltrace-live init --db-url sqlite:./soltrace.db

# 2. Run the real-time indexer (drop an IDL in ./idls first)
./target/release/soltrace-live run \
  --program-prefixes "<program_id>:<prefix>" \
  --idl-dir ./idls --db-url sqlite:./soltrace.db \
  --rpc-url https://api.mainnet-beta.solana.com \
  --ws-url  wss://api.mainnet-beta.solana.com

# 3. (Optional) Historical backfill (one-shot)
./target/release/soltrace-backfill \
  --program-prefixes "<program_id>:<prefix>" \
  --idl-dir ./idls --db-url sqlite:./soltrace.db \
  --rpc-url https://api.mainnet-beta.solana.com
```

Full env-var reference in [README.md → Environment Variables](./README.md#environment-variables).
A runnable reference deployment lives in [examples/tributary/](./examples/tributary/).

## Development Workflow

No dev-server / hot-reload. The loop is: edit → `cargo build` / `cargo test`
→ run the binary against a local sqlite DB. For iteration on decode logic,
prefer adding a unit test (see below) over running the full indexer.

```bash
cargo check --workspace                 # fast type-check
cargo build --workspace                 # debug build
cargo run --package soltrace-live -- run --help   # run a binary in dev
```

Env vars can be sourced from a file (the binaries load `.env` via `dotenv`);
see [examples/tributary/env](./examples/tributary/env) for the shape.

## Testing Instructions

Tests are **inline** `#[cfg(test)] mod tests { ... }` blocks at the bottom of
each `.rs` file (no `tests/` directory, no external harness). Coverage
concentrates in `soltrace-core`: `idl.rs`, `idl_event.rs`, `event.rs`,
`utils.rs`, `onchain_idl.rs`.

```bash
# All tests across the workspace
cargo test --workspace

# One crate
cargo test --package soltrace-core

# One test by name substring
cargo test --package soltrace-core -- cpi_dedup_index

# With println output visible
cargo test --workspace -- --nocapture

# Kafka feature tests (gated behind the feature flag)
cargo test --workspace --features kafka
```

**When you change decode/extraction logic, add or update a test in the same
file.** The decoder handles a long list of borsh types
(`bool`, `u8`–`u128`, `i8`–`i128`, `string`, `publicKey`, `bytes`, `option<T>`,
`vec<T>`, `[T; N]`, enums, nested structs) — each has a test case; mirror the
existing pattern when adding support for a new type.

## Code Style

Rust, edition 2021. Standard Rust formatting; no custom style doc.

```bash
cargo fmt --all                          # format
cargo fmt --all -- --check               # check only (CI gate)
cargo clippy --workspace --all-targets -- -D warnings   # lint, warnings = errors
```

Conventions:

- **Error handling**: `anyhow` for binaries, `thiserror` (`SoltraceError`) in
  `soltrace-core`. Don't bubble `anyhow::Error` out of core APIs.
- **Async**: Tokio. DB and Kafka code is async; the decode path
  (`IdlParser`/`IdlEventDecoder`/`EventDecoder`) is sync and cheap — keep it
  off the async boundary where it already is.
- **Logging**: `tracing` (`info!`/`warn!`/`debug!`/`error!`), **never**
  `println!`/`eprintln!`. Unknown-discriminator events are skipped at
  `debug!` level — do not promote them to warnings.
- **Public surface**: `soltrace-core/src/lib.rs` re-exports the public API.
  Add new types/functions there if operators or the binaries need them.
- **Comments**: per repo convention, do not add comments unless asked.
- **No comments that just restate code.** Comments explaining _why_
  (a non-obvious Solana runtime detail, a dedup-offset ceiling) are welcome.

### Database backends

`soltrace-core/src/db/` has a `Database` trait with three backends
auto-selected by URL scheme: `sqlite:` (JSON text), `postgres:` (JSONB + GIN),
`mongodb:` (nested docs). When adding a column or changing the schema, update
**all three** `init`/`insert` implementations plus the
`CREATE TABLE` in sqlite.

## Build and Deployment

```bash
# Release binaries
cargo build --release
# → target/release/soltrace-live, target/release/soltrace-backfill

# Docker (multi-stage; builds with --features kafka)
docker build -t soltrace .
docker compose build && docker compose up -d   # live + backfill + kafka + kafka-ui
```

`docker-compose.yml` runs `soltrace-live` (long-running) and
`soltrace-backfill` (one-shot, `restart: "no"`) sharing a named DB volume and
a read-only `idls/` mount, plus Kafka + Kafka UI. There is **no CI pipeline
yet** (`.github/workflows/` is empty) — the pre-commit hooks below are the
only automated gate.

## Pull Request Guidelines

**Before committing**, run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### Commit messages — Conventional Commits + Gitmoji (enforced)

Pre-commit runs [commitizen](https://github.com/commitizen-tools/commitizen)
with [cz-conventional-gitmoji](https://github.com/ljnsn/cz-conventional-gitmoji).
Format:

```
<type>(<scope>): <emoji> <subject>
```

Examples: `feat(core): ✨ add u128 array decode`, `fix(utils): 🐛 strip
wrapper disc before lookup`. Use the conventional-commit types
(`feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `perf`, ...) and the
matching gitmoji. Hook setup (first checkout only):

```bash
pip install pre-commit && pre-commit install --install-hooks -t commit-msg
```

### PR style

ASCII diagrams, data-flow drawings, and a test summary are welcome and on-brand
for this repo. End with a "ship it" flourish.

## Issue Tracking

This repo uses **[beans](https://github.com/nicholasgriffintn/beans)** (not
todo lists). Bean IDs are prefixed `soltrace-` (see
[.beans.yml](./.beans.yml)). Before starting work, check for an existing bean;
create one if none exists, and keep its checklist current as you go.

```bash
beans list --json --ready                 # beans ready to start
beans show --json <id>                    # full detail
beans create "Title" -t task -d "..."     # create (type: milestone|epic|feature|bug|task)
```

## Debugging and Troubleshooting

- **Events decode as `{"hex": "..."}`** — `IdlEventDecoder` failed
  borsh decode. Causes: IDL version mismatch (re-fetch), wrong IDL keyed
  (the `address` field must equal the on-chain program id), or an
  undecodable borsh shape (the `decode_error` field names the cause).
- **`No event found with discriminator`** — no loaded IDL defines a matching
  event; skipped at debug level. Not a crash.
- **`UNIQUE constraint failed`** — dedup working as designed; reprocessing is
  idempotent. Do not "fix" by catching/retrying.
- **`database is locked` (SQLite)** — concurrency pressure; lower
  `--concurrency` or move to Postgres/Mongo.
- **RPC `429`** — raise `--batch-delay`, lower `--batch-size`/`--concurrency`,
  or use a paid RPC. `retry_with_rate_limit` already backs off.
- **WebSocket reconnect loop** — confirm `wss://` scheme; public endpoints
  throttle, use a paid RPC.

See [README.md → Troubleshooting](./README.md#troubleshooting) for the full
list.

## Things That Will Bite You

- **Don't add IDLs to the repo.** `idls/` is operator-installed and loaded
  once at startup; changes mid-run are ignored until restart.
- **Don't conflate the two discriminators.** The `emit_cpi!` wrapper disc
  (`0x1d9acb512ea545e4`) is stripped _before_ the event's own disc
  (`sha256("event:<name>")[..8]`) is looked up. Confusing them silently
  breaks every CPI-path decode.
- **Don't put logic in `main.rs`.** Both binaries are thin; reusable code
  goes in `soltrace-core`.
- **Don't promote unknown-discriminator skips to warnings.** They're debug by
  design — a program may emit events before its on-chain IDL arrives.
- **Mixed Solana SDK versions** (`solana-sdk` 4.0, most others 3.1). If a
  type mismatch appears across crates, check the workspace dep versions in
  the root [Cargo.toml](./Cargo.toml) before "fixing" with a cast.

## Beans

**IMPORTANT**: before you do anything else, run the

- `beans prime`
- `hordr prime`

commands and heed the output.
When making a commit, include the relevant bean IDs in the commit message.

### Rules for the agent producing the milestone

- **Read the change first.** Trace what the feature actually touches
  (program? SDK only? an app? docs?) before creating any bean.
- **Omit what doesn't apply.** A pure-SDK change needs no program-contract
  feature. A doc-only milestone has no implementation epic. Don't pad.
- **One feature per `apps/` deliverable group**; one task per touched
  `apps/<dir>`. Don't fan out tasks for untouched apps.
- **New ADR ⇒ documentation feature** (or its own epic if the decision is
  the milestone). Code is authority on state; ADR is authority on rationale.
- **Program changes ⇒ Update of tributary.qedspec**; update the spec and
  recreate the entire formal_verification directory accordingly!
- **Status flows up.** A milestone is `completed` only when all its leaf
  tasks are `completed`. Epics close when all their features close.

### Bean hygiene

1. **Check before creating.** Run `beans list --json` and scan for existing
   beans covering the same scope. Duplicates waste context. If a new bean
   subsumes an old one, scrap the old with a `## Reasons for Scrapping`
   section — don't leave both.
2. **Restructuring.** When a grilling or design session changes scope,
   rewrite bean bodies by appending a `## REWRITTEN SCOPE (date —
supersedes content above)` section. Update titles via GraphQL (CLI has
   no `--title` flag — see cheat sheet). Don't scrap and recreate beans
   that have accumulated context; rewrite in place.
3. **Design decisions → milestone body.** When a grilling resolves
   architectural questions, capture them in the milestone body as a
   "Design decisions" section with struct layouts, flow diagrams, and
   rationale. Individual tasks under features carry the acceptance criteria
   (TDD checklist).
4. **Investigation beans.** Research that may never become actionable goes
   as a `draft` epic with `low` priority. Don't create task children until
   the investigation concludes and the scope is concrete.
5. **Active milestones may supersede code state.** The AGENTS.md and ADRs
   describe the _current deployed_ architecture. An active milestone's body
   may contain design decisions that will change the code but haven't
   landed yet. Always check `beans list --json --ready` for in-flight work
   before assuming the docs reflect reality.
