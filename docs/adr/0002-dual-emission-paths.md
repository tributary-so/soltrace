# Cover both Anchor emission paths through one decoder

Status: accepted

Soltrace decodes events from **both** Anchor macros — `emit!` (scraped from
`Program data: <base64>` log lines) and `emit_cpi!` (extracted from
self-CPIs to `__event_authority` in inner instructions, after stripping the
8-byte `emit_cpi!` wrapper discriminator) — and routes the unwrapped bytes
through a single `EventDecoder`. Indexing only one path was rejected: programs
choose their macro per event (and sometimes use both), so a single-path
indexer would silently lose events. The cost is a second extraction path and a
dedup scheme that keeps the two collision-free (ADR-0003).

## Consequences

- A single signature can yield Event Rows from both paths; the dedup index
  offsets the two so they never collide (ADR-0003).
- The `emit_cpi!` path forces Live mode to fetch the full transaction on every
  log notification (inner instructions are not in the log stream), whereas
  `emit!` events can be decoded from logs alone.
- The `emit_cpi!` wrapper discriminator (`0x1d9acb512ea545e4`) and the event's
  own discriminator (`sha256("event:<name>")[..8]`) are two distinct 8-byte
  values with similar names — confusing them silently breaks every CPI-path
  decode, so the wrapper is stripped _before_ the event discriminator is looked
  up.
