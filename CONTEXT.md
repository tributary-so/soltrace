# Soltrace

A protocol-agnostic Solana event indexer. Soltrace reads Anchor IDLs supplied by
the operator, decodes `emit!` and `emit_cpi!` events from those programs, and
stores each decoded occurrence as a row keyed by where in the transaction it
came from. Soltrace itself knows nothing about any specific Solana program.

## Language

### The indexed thing

**EventEmission**:
An act of an Anchor program emitting an event on-chain. One EventEmission
becomes at most one Event Row. Distinguish from the bytes, the decoded value,
and the row.
_Avoid_: event, anchor event, log entry

**EventPayload**:
The bytes of one EventEmission after any wrapper has been stripped:
`<event-disc(8)><borsh-encoded fields>`. The unit `IdlEventDecoder` consumes.
_Avoid_: event data, raw bytes, payload

**DecodedEvent**:
The post-borsh JSON value of an EventPayload, plus its discriminator and
prefixed name. The unit `EventDecoder` returns.
_Avoid_: parsed event, decoded data, event object

**RawEvent**:
The capture of an EventEmission's carrier before decode: slot, signature,
program id, source log line (empty for CPI events), and timestamp.
_Avoid_: log entry, raw event data

**Event Row**:
One stored occurrence in the events table. The dedup unit. A single signature
can carry many Event Rows (log path + CPI path; same name, different sources).
_Avoid_: event, record, entry

### Emission paths

**emit! path**:
Anchor's `emit!` macro. Events surface as `Program data: <base64>` log lines
in `meta.log_messages`. Extracted by log-scraping.

**emit_cpi! path**:
Anchor's `emit_cpi!` macro. Events surface as self-CPIs to the program's
`__event_authority` PDA inside `meta.innerInstructions`. Extracted by
inner-instruction inspection. Bytes are wrapped: `[event_cpi disc(8)][EventPayload]`.

**event_cpi wrapper discriminator**:
The fixed 8-byte prefix `0x1d9acb512ea545e4` on every `emit_cpi!` instruction.
Distinct from the inner event's own discriminator (sha256("event:<name>")[..8]).

### Solana primitives (as used here)

**Inner instruction**:
Any instruction nested inside an outer instruction of a transaction, as listed
in `meta.innerInstructions`. Solana-runtime concept.
_Avoid_: CPI (an inner instruction is not necessarily a CPI)

**CPI**:
Cross-Program Invocation — one Solana program invoking another. Always appears
as an inner instruction; not every inner instruction is a CPI.

**Self-CPI**:
A CPI where the calling program invokes a PDA of itself. `emit_cpi!` is a
self-CPI to `["__event_authority"]`.

**Signature**:
A Solana transaction's base58 signature. Used as the transaction-identity
component of the dedup key. A signature can carry many Event Rows.

**Slot**:
Solana slot number in which the transaction was confirmed.

**Commitment**:
Solana commitment level (processed / confirmed / finalized) at which the
indexer subscribes and fetches. Affects reorg exposure and finality.

### Identity & naming

**Discriminator**:
The 8-byte Anchor event discriminator `sha256("event:<name>")[..8]`. Used to
look up an EventPayload's IDL definition. (See also: event_cpi wrapper
discriminator — a different 8-byte value with a similar name.)

**Prefix**:
The per-program string prepended to event names when storing
(`<prefix>_<EventName>`). Configured by the operator via `PROGRAM_PREFIXES`;
defaults to `default` for any program not mapped.

### Ingestion modes

**Live**:
The real-time mode. Subscribes to program logs via WebSocket; on each
notification, fetches the full transaction to decode `emit_cpi!` events from
inner instructions.

**Backfill**:
The historical mode (`soltrace-backfill`). Paginates `get_signatures_for_address`
backwards from the chain tip, fetches each transaction, processes it.
_Avoid_: catch-up, reprocess

**Catch-up**:
The startup phase of Live that fills the gap between the last stored signature
and the current chain tip, using the Backfill mechanism.
_Avoid_: gap backfill (overloaded with Backfill)

**Reprocess**:
Running a signature through the decode-and-store pipeline again. Always
idempotent — duplicate Event Rows are silently dropped on the dedup key.

### The actor

**Operator**:
The person who runs soltrace. Owns the IDLs, the RPC endpoints, and the
program-to-prefix mapping. Soltrace never ships IDLs; the operator installs
them. Distinguish from any downstream consumer of the stored Event Rows.

### Artifacts

**IDL**:
An Anchor Interface Definition Language JSON file describing a program's events
and types. Loaded from the IDL Directory. Filename is free-form; soltrace keys
by the IDL's top-level `address` field.

**IDL Directory**:
The folder (default `./idls`, override via `IDL_DIR`) from which every `*.json`
IDL is loaded. Loaded exactly once at process startup; changes during a run
are ignored until restart.
