---
# soltrace-nc4t
title: Extend IdlParser + EventDecoder for arc-swap hot-reload
status: completed
type: task
priority: normal
created_at: 2026-07-28T08:31:17Z
updated_at: 2026-07-28T10:22:46Z
parent: soltrace-ipo0
blocked_by:
    - soltrace-nqfd
---

See MS soltrace-z9wq HANDOFF.

## Cut

In soltrace-core/src/idl.rs:

- Add IdlParser::insert_or_replace(&mut self, idl: ParsedIdl) -> inserts/replaces by idl.address
- Add IdlParser::remove(&mut self, program: &str)

In soltrace-core/src/event.rs:

- Change EventDecoder.idl_parser field type: IdlParser -> Arc<ArcSwap<IdlParser>>
- In decode_event: let parser = self.idl_parser.load(); then use parser.find_event_by_discriminator(...) (one load per call)
- Update EventDecoder::new signature to accept Arc<ArcSwap<IdlParser>>

Update existing call sites in soltrace-live/src/main.rs:246-260 and soltrace-backfill/src/main.rs:98-134 to wrap parser in Arc::new(ArcSwap::from(parser)) before EventDecoder::new. (Subscription spawn task comes later in Epic 2/3 — for now just wrap once.)

## TDD

- Test insert_or_replace overwrites prior entry with same address
- Test remove drops entry
- Test ArcSwap: spawn N reader tasks calling decode_event in a loop, single writer swaps new parser, never observe torn state (decode_event either sees old or new map, never panics)

## Verify

- cargo test -p soltrace-core green
- cargo build --workspace green
- Existing tests in idl.rs:140-232 + event.rs still pass unchanged (no semantic regression)


## Summary of Changes

### soltrace-core/src/idl.rs
- `IdlParser::insert_or_replace(&mut self, idl: ParsedIdl)` — upsert keyed by `idl.address` (on-chain hot-reload insert).
- `IdlParser::remove(&mut self, program: &str)` — drop by address (on-chain IDL account closed).

### soltrace-core/src/event.rs
- `EventDecoder.idl_parser` field: `IdlParser` -> `Arc<ArcSwap<IdlParser>>`.
- `EventDecoder::new` now takes `Arc<ArcSwap<IdlParser>>` (shared with the future subscription task).
- `decode_event`: ONE `self.idl_parser.load()` per call; the `Guard` pins a coherent
  revision so a concurrent `store()` (on-chain IDL swap) can never tear a mid-decode view.
  Both `find_event_by_discriminator` and the `types` lookup read from that single load.
  `// ponytail:` comment documents the snapshot semantics.

### soltrace-core/src/lib.rs
- Re-exported `pub use arc_swap::ArcSwap;` so binaries build the shared
  `Arc<ArcSwap<IdlParser>>` without a direct arc-swap dep.

### Binary call sites (soltrace-live/main.rs:260, soltrace-backfill/main.rs:134)
- Wrap `idl_parser` in `Arc::new(soltrace_core::ArcSwap::from_pointee(idl_parser))`
  before `EventDecoder::new`. Borrow of `idl_parser` ends (NLL) before the move,
  so no regression. (Subscription task spawn is Epic 2 — for now wrap once.)

### Construction note
Used `ArcSwap::from_pointee(owned)` (not `ArcSwap::from`): arc-swap only impls
`From<Arc<T>>`, not `From<T>` (the pointee). `from_pointee` is the documented
equivalent of `ArcSwap::new(Arc::new(val))`.

### TDD (3 new tests, all green)
- `test_insert_or_replace_overwrites_same_address`: same-address upsert keeps 1 entry, latest wins.
- `test_remove_drops_entry`: remove by address clears it.
- `arcswap_load_never_tears_under_concurrent_swap`: 4 reader threads hammer
  decode_event while a writer flips the parser between {with-event}/{empty}/{fresh};
  every result is Ok or a clean `No event found` Err — never a panic or corrupt decode.

### Verify
- `cargo build --workspace --all-features` green.
- `cargo test --workspace` green (45 core + 2 live).
- `cargo clippy --workspace --all-features` — ZERO new warnings (event.rs/idl.rs/call
  sites clean; 3 pre-existing `needless_borrows` in db/{sqlite,postgres}.rs remain, out of scope).
- Existing idl.rs + event.rs + utils.rs tests pass unchanged (semantic equivalence preserved).
