---
# soltrace-z9wq
title: On-chain IDL fetch & live updates
status: completed
type: milestone
priority: normal
created_at: 2026-07-28T08:30:16Z
updated_at: 2026-07-30T14:03:26Z
---

# On-chain IDL fetch & live updates

Add the ability to fetch Anchor IDLs directly from Solana via the
`program-metadata` on-chain program (canonical `"idl"`-seed Metadata PDAs),
with live `accountSubscribe` updates. File-based IDLs in `./idls/` remain the
primary source; on-chain is purely additive for programs not present as files.

## Decision log (locked)

| #   | Decision      | Choice                                                                                                                                                            |
| --- | ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Priority      | **File primary**; on-chain auto-fetch only for programs not in `./idls/`                                                                                          |
| 2   | Discovery     | Explicit `--onchain-programs` / `ONCHAIN_PROGRAMS` list (base58 CSV)                                                                                              |
| 3   | Hot-reload    | `Arc<ArcSwap<IdlParser>>`, full swap on every update                                                                                                              |
| 4   | Trust         | **Canonical PDA only** (`[program, "idl"+zero-pad-16]`)                                                                                                           |
| 5   | `data_source` | **Direct only**; non-Direct → warn + skip                                                                                                                         |
| 6   | Subscription  | Dedicated WS connection, `accountSubscribe` (push = fetch), unsub on `SetImmutable`, drop+warn on `Close`, capped-backoff reconnect mirroring logs_subscribe loop |
| 7   | Backfill      | One-shot `load_onchain_idls` at startup, no subscription task                                                                                                     |
| 8   | Commitment    | Reuse `--ws-commitment` (default confirmed)                                                                                                                       |
| 9   | Inflate       | `flate2` crate                                                                                                                                                    |
| 10  | Modules       | core: new `onchain_idl.rs` + extend `idl.rs`; live: new `idl_subscription.rs`; backfill: one-shot call                                                            |
| 11  | Failures      | Per-program startup failure: warn-and-continue; subscription decode failure: keep old IdlParser; CLI: hard-error on malformed pubkey                              |

## HANDOFF

### 1. Happy Path (per task)

1. Pick a `todo` task under one of the three epics.
2. Read this HANDOFF in full + the task body for specifics.
3. TDD: write failing test → implement → green → refactor. No exceptions.
4. `cargo build --workspace --all-features` + `cargo test --workspace` green.
5. `beans update <id> -s completed` only when Definition of Done (below) is met.

### 2. Data Contract

**Existing types/behavior preserved (do NOT break):**

- `soltrace-core/src/idl.rs` — `IdlParser { idls: HashMap<String, ParsedIdl> }` keyed by IDL `address`. Existing API: `new`, `load_from_file`, `load_from_str`, `get_idls`, `get_events`, `calculate_discriminator`, `find_event_by_discriminator`. **All stay.**
- `soltrace-core/src/event.rs` — `EventDecoder { idl_parser, prefix_config }`. **Field type changes** from `IdlParser` to `Arc<ArcSwap<IdlParser>>`. Update the ONE read site in `decode_event` to `.load()` once per call.
- `soltrace-core/src/utils.rs:19-51` — `load_idls(&mut IdlParser, idl_dir)`. Unchanged. Add a sibling `load_onchain_idls` next to it.
- `soltrace-live/src/main.rs:485-555` — capped-backoff WS reconnect loop. **Mirror this** for the new dedicated subscription WS.
- `soltrace-live/src/main.rs:586-594` — existing `logs_subscribe` setup. Unchanged.
- `soltrace-backfill/src/main.rs:98-134` — load IDLs → derive program IDs → EventDecoder. Add a one-shot `load_onchain_idls` call before `EventDecoder::new`.

**New contract:**

- Canonical IDL PDA derivation: `Pubkey::find_program_address(&[program.as_ref(), &seed_padded_16], &PROGRAM_METADATA_ID)` where `seed_padded_16` is `b"idl"` right-zero-padded to 16 bytes. Program-metadata program ID: `ProgM6JCCvbYkfKqJYHePx4xxSUSqJp7rh8Lyv7nk7S`.
- `Metadata` account (from `spl-program-metadata-client`): discriminator=2, has `compression` (None/Gzip/Zlib), `encoding` (None/Utf8/Base58/Base64), `data_source` (None/Direct/Url/External), `data: TrailingVec<u8>`. Default CLI writes `Zlib + Utf8 + Direct`.
- Direct data path: inflate (per compression) → utf8-decode (per encoding) → `serde_json::from_str` → `ParsedIdl`. Non-Direct → return `None`, log warn.

**External context (read-only, in-repo):**

- `program-metadata/clients/rust/` — Codama-generated client. Use `pub use generated::*` re-exports. `PROGRAM_METADATA_ID` constant + `Metadata` struct + `fetch_metadata(rpc, &addr)` (feature-gated `fetch`). **No PDA helper in Rust** — derive manually. See `program-metadata/clients/rust/src/generated/accounts/metadata.rs:54` and `program-metadata/idl.json:10-93`.
- `program-metadata/program/src/processor/{initialize,set_data}.rs` — authoritative PDA seed layout (cross-check).

### 3. Edge Cases & Constraints

- **Chicken-and-egg with `logs_subscribe` filter.** The logs filter is built from program IDs known upfront. For on-chain-IDL programs, we know the addresses from `--onchain-programs` (CLI), so subscribe to logs immediately even before IDLs arrive. Events whose IDL hasn't landed yet hit the existing unknown-discriminator path (`debug!` skip). Once the IDL arrives via accountSubscribe push, decoding starts. Brief startup gap is acceptable.
- **Subscription push semantics.** `accountSubscribe` pushes the **current** account state immediately on subscribe (this is how we get the initial IDL — no separate startup fetch needed), then deltas. On disconnect, all subscriptions are lost — re-subscribe on reconnect.
- **`SetImmutable` (mutable=false).** The IDL is frozen forever. Unsubscribe to free the WS slot. Log once at info.
- **`Close`.** Account closed → push with empty account. Drop program from IdlParser (swap fresh map without it). Unsubscribe. Log at **warn**.
- **Subscription decode failure on a push** (garbage data, malicious account, decompression fail). **Do NOT swap.** Keep the last-known-good IdlParser. Log warn with the decode error.
- **Subscription task crash / reconnect exhaustion.** Do not take down the indexer. Keep running with the last-known IdlParser (file IDLs + previously-fetched on-chain IDLs still work). Log error.
- **All on-chain programs fail AND no file IDLs.** Existing `program_ids.is_empty()` hard-exit kicks in (`main.rs:247`, backfill `main.rs:122`). No new code path.
- **Backfill is point-in-time.** On-chain IDLs have no version history (`SetData` overwrites in place). If the IDL changed between the historical tx and backfill time, decoding may produce garbage. **Same staleness semantics as file IDLs** — out of scope to fix.
- **Prefix config interaction.** On-chain IDLs must be inserted into `IdlParser` BEFORE `ProgramPrefixConfig::load_from_idls` runs (so default prefix logic applies uniformly). `--program-prefixes` works identically for on-chain programs. Ordering at startup:
  1. `load_idls` (file)
  2. `load_onchain_idls` (one-shot RPC)
  3. `ProgramPrefixConfig::load_from_idls`
  4. Wrap parser in `Arc<ArcSwap<IdlParser>>`
  5. Spawn subscription task (live only)
  6. Start `logs_subscribe`
- **CLI validation.** Parse each entry as `Pubkey::from_str`. Hard-error on malformed (operator typo should be loud, not silently dropped).
- **Ponytail.** Ponytail mode is active in this repo. No speculative abstractions. No config knobs that don't have a current consumer. If a function can be one line, make it one line. Mark deliberate shortcuts with `// ponytail:` comments.

### 4. Business Logic (pseudo-code, target language: Rust)

```rust
// onchain_idl.rs (new)
pub fn derive_canonical_idl_pda(program: &Pubkey) -> Pubkey {
    let mut seed = [0u8; 16];
    seed[..3].copy_from_slice(b"idl");
    let (pda, _) = Pubkey::find_program_address(&[program.as_ref(), &seed], &PROGRAM_METADATA_ID);
    pda
}

pub fn decode_metadata_account(account: &Account, expected_program: &Pubkey) -> Result<Option<ParsedIdl>> {
    let meta = Metadata::from_bytes(&account.data)?;
    if meta.program != *expected_program { return Ok(None); }   // wrong account
    if !matches!(meta.data_source, DataSource::Direct) { return Ok(None); }   // v1: Direct only
    let bytes = inflate(meta.compression, &meta.data)?;
    let text  = decode_bytes(meta.encoding, &bytes)?;
    let parsed = serde_json::from_str::<ParsedIdl>(&text)?;
    Ok(Some(parsed))
}

pub fn fetch_canonical_idl(rpc: &RpcClient, program: &Pubkey) -> Result<Option<ParsedIdl>> {
    let pda = derive_canonical_idl_pda(program);
    let account = match rpc.get_account(&pda) { Ok(a) => a, Err(_) => return Ok(None) };
    decode_metadata_account(&account, program)
}

// idl.rs (extend)
impl IdlParser {
    pub fn insert_or_replace(&mut self, idl: ParsedIdl) { self.idls.insert(idl.address.clone(), idl); }
    pub fn remove(&mut self, program: &str) { self.idls.remove(program); }
}

// EventDecoder: idl_parser field becomes Arc<ArcSwap<IdlParser>>
// In decode_event: let parser = self.idl_parser.load();  parser.find_event_by_discriminator(...)

// utils.rs (extend)
pub async fn load_onchain_idls(parser: &mut IdlParser, rpc: &RpcClient, programs: &[Pubkey]) {
    for program in programs {
        match fetch_canonical_idl(rpc, program).await {
            Ok(Some(idl)) => { parser.insert_or_replace(idl); info!(...); }
            Ok(None)      => warn!(...),       // no canonical IDL / non-Direct
            Err(e)        => warn!(...),       // RPC / decode failure
        }
    }
}

// idl_subscription.rs (new, live only)
pub fn spawn_idl_subscription_task(
    programs: Vec<Pubkey>,
    ws_url: String,
    commitment: CommitmentConfig,
    shared_parser: Arc<ArcSwap<IdlParser>>,
) -> JoinHandle<()> { /* dedicated WS, reconnect loop, per-PDA accountSubscribe */ }
```

### 5. Definition of Done (per task AND milestone close)

**Per task:**

- [ ] TDD: red → green → refactor. Test committed with implementation.
- [ ] `cargo build --workspace --all-features` green
- [ ] `cargo test --workspace` green
- [ ] No new clippy warnings (`cargo clippy --workspace --all-features -- -D warnings`)
- [ ] Ponytail compliance: no speculative abstractions, `// ponytail:` comments where appropriate

**Milestone close:**

- [ ] `--onchain-programs` flag works end-to-end in both binaries
- [ ] Live subscription swaps in new IDL on `SetData` push, verified by integration test or localnet demo
- [ ] Subscription survives WS disconnect (reconnect + re-subscribe)
- [ ] `SetImmutable` → unsubscribe; `Close` → drop from map + warn
- [ ] File IDL precedence verified: file IDL for same program as on-chain wins
- [ ] Backfill one-shot fetch verified
- [ ] No behavioral change when `--onchain-programs` is empty (zero-regression for existing operators)
- [ ] Docs: `idls/README.md` updated with the new flag and the priority model

### 6. Test Matrix (Given / When / Then)

**Core primitives (`onchain_idl.rs`):**

- Given the program-metadata canonical PDA derivation vectors from `program-metadata/program/tests/setup/initialize.rs:14-19`, When `derive_canonical_idl_pda(program)` is called, Then output matches the reference JS/TS helper output byte-for-byte.
- Given a fixture zlib+utf8 IDL `Metadata` account blob (synthesized from a known IDL JSON), When `decode_metadata_account` runs, Then output equals the source `ParsedIdl`.
- Given a `Metadata` with `data_source = Url`, When `decode_metadata_account` runs, Then returns `Ok(None)`.
- Given a `Metadata` whose `program` field doesn't match the expected program, When `decode_metadata_account` runs, Then returns `Ok(None)`.
- Given a non-existent PDA, When `fetch_canonical_idl` runs against a stub RPC, Then returns `Ok(None)` (no error propagation).

**IdlParser hot-swap (`idl.rs`):**

- Given an `Arc<ArcSwap<IdlParser>>` with N entries, When a writer swaps in a new parser with N+1 entries, Then a concurrent reader calling `.load().find_event_by_discriminator(...)` sees either the old or new map, never a torn state.

**Subscription loop (`idl_subscription.rs`):**

- Given a mock WS that pushes a `SetData` notification, When the subscription task processes it, Then the shared parser is swapped to include the new IDL within 1 tick.
- Given a mock WS that pushes a `Close` notification (empty account), When the subscription task processes it, Then the program is removed from the shared parser.
- Given a mock WS that pushes garbage that fails `decode_metadata_account`, When the subscription task processes it, Then the shared parser is **not** swapped (old IDL retained), warn logged.
- Given a mock WS that disconnects, When the reconnect loop fires, Then all PDAs are re-subscribed on reconnect.

**Binary integration:**

- Given `--onchain-programs TRib...`, When soltrace-live starts, Then: file IDLs loaded → on-chain IDL fetched → prefix config built → subscription task spawned → logs_subscribe starts (in that order).
- Given an invalid `--onchain-programs NOTABASE58`, When CLI parses, Then hard-error with a clear message.
- Given `--onchain-programs` is empty/unset, When soltrace starts, Then behavior is byte-identical to today.

### 7. Open Questions

- **Http fetcher for `Url` data_source?** Deferred — out of scope for v1. If we hit a `Url` IDL in production, log and skip; revisit if it becomes common.
- **`External` data_source?** Deferred — out of scope for v1. Same skip+log behavior.
- **Auto-discovery via `ProgM_ID` logs_subscribe?** Out of scope — that's a different feature ("index new programs automatically"), not "fetch known program's IDL from chain".
- **Per-program `:canonical|any` config flag?** Rejected — non-canonical IDLs are an attacker surface for garbage decode; canonical-only matches Solana Explorer.
- **Snapshot historical IDLs for accurate backfill?** Out of scope — would require an off-chain IDL archive (a separate service). Current behavior matches file-IDL staleness.
- **Shared WS connection with logs_subscribe?** Rejected — coupled failure modes; dedicated connection is simpler.

## Out of scope (explicit YAGNI)

- Non-canonical IDLs, `Url` / `External` data sources, version history, historical IDL reconstruction for backfill, auto-discovery via `ProgM_ID` logs, per-program source config.

## External references

- `program-metadata/README.md` — operator-facing docs for the on-chain program
- `program-metadata/clients/rust/src/generated/accounts/metadata.rs:54` — `fetch_metadata` signature
- `program-metadata/program/src/processor/{initialize,set_data}.rs` — PDA seed derivation source-of-truth
- `program-metadata/idl.json:10-93` — `Metadata` account layout
- `soltrace-core/src/idl.rs:60-120` — existing `find_event_by_discriminator` (must keep working)
- `soltrace-live/src/main.rs:485-555` — capped-backoff reconnect loop template
- `soltrace-live/src/main.rs:586-594` — existing `logs_subscribe` setup
