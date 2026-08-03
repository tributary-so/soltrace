# Soltrace

A protocol-agnostic Solana event indexer. The operator installs Anchor IDLs;
soltrace decodes `emit!` and `emit_cpi!` events from those programs and stores
each decoded occurrence as a row keyed by where in the transaction it came
from. Soltrace itself knows nothing about any specific Solana program.

## Language

### The indexed thing

**EventEmission**:
An act of an Anchor program emitting an event on-chain. One EventEmission
becomes at most one Event Row.
_Avoid_: event, anchor event, log entry

**EventPayload**:
The bytes of one EventEmission after any wrapper has been stripped. The unit
`IdlEventDecoder` consumes.
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
in `meta.logMessages`. Extracted by log-scraping.

**emit_cpi! path**:
Anchor's `emit_cpi!` macro. Events surface as self-CPIs to the program's
`__event_authority` PDA inside `meta.innerInstructions`. Extracted by
inner-instruction inspection.

**event_cpi wrapper discriminator**:
A fixed 8-byte prefix on every `emit_cpi!` instruction, stripped before the
event's own discriminator is looked up. A different value from an event's own
discriminator despite the similar name.

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
A Solana transaction's base58 signature. The transaction-identity component of
the dedup key. One signature can carry many Event Rows.

**Slot**:
Solana slot number in which the transaction was confirmed.

**Commitment**:
Solana commitment level (processed / confirmed / finalized) at which the
indexer subscribes and fetches. Affects reorg exposure and finality.

### Identity & naming

**Discriminator**:
Anchor's 8-byte event type tag. Used to look up an EventPayload's IDL
definition. (See also: event_cpi wrapper discriminator — a different 8-byte
value with a similar name.)

**Prefix**:
The per-program string prepended to event names when storing
(`<prefix>_<EventName>`). Configured by the operator; defaults to `default`
for any program not mapped.

### IDL sources

**IDL**:
An Anchor Interface Definition Language JSON document describing a program's
events and types. Soltrace keys an IDL by its top-level `address` field
(filename is free-form).

**IDL Directory**:
The folder (default `./idls`) from which every `*.json` IDL is read. Loaded
exactly once at process startup; changes during a run are ignored until
restart.

**File IDL**:
An IDL the operator places in the IDL Directory. Always wins over any
on-chain IDL for the same program.
_Avoid_: local IDL, disk IDL

**On-chain IDL**:
An IDL fetched live from Solana rather than read from disk. Either a
program-metadata IDL or a classic Anchor IDL — the two publication standards
soltrace supports.
_Avoid_: remote IDL

**program-metadata IDL**:
An IDL published via the Solana program-metadata program — the modern Anchor
(≥0.31) standard. Stored in a Metadata account whose data source is Direct;
the only on-chain form that supports IDL hot-swap.
_Avoid_: new IDL, SPL IDL

**classic Anchor IDL**:
An IDL published by Anchor's own `anchor idl init` mechanism
(pre-program-metadata). Stored compressed in an account owned by the indexed
program itself. Not to be confused with a legacy IDL.
_Avoid_: legacy IDL (reserved for the pre-0.30 shape), old IDL

**legacy IDL**:
The pre-0.30 IDL _JSON shape_ — a structurally different document from the
current spec. Distinct from a classic Anchor IDL (a publication mechanism, not
a shape). Decoding legacy-shape IDLs is deferred and out of scope.

**IDL hot-swap**:
The live-indexer mechanism that replaces the in-memory parser when a
program-metadata IDL account is updated on-chain, without a restart. Applies
to program-metadata IDLs only.
_Avoid_: reload, refresh

### Ingestion modes

**Live**:
The real-time mode. Subscribes to program logs via WebSocket; on each
notification, fetches the full transaction to decode `emit_cpi!` events from
inner instructions.

**Backfill**:
The historical mode (`soltrace-backfill`). Paginates
`get_signatures_for_address` backwards from the chain tip, fetches each
transaction, processes it.
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
