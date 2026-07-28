---
# soltrace-nc4t
title: Extend IdlParser + EventDecoder for arc-swap hot-reload
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
