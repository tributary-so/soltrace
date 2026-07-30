# IDL directory

Soltrace is protocol-agnostic. It decodes any Anchor program's `emit_cpi!`
events from inner instructions, routing the unwrapped bytes through whatever
Anchor IDLs the operator drops into this directory.

**No IDLs are vendored.** Operators install the IDLs for the programs they
want to index. The binaries load every `*.json` in `IDL_DIR` (default `./idls`,
override with `--idl-dir` / `IDL_DIR`).

## Installing an IDL

Fetch from mainnet with Anchor CLI (the on-chain IDL account is the deployed
artifact):

```bash
anchor idl fetch <PROGRAM_ADDRESS> --provider.cluster https://api.mainnet-beta.solana.com \
  > idls/<name>.json
```

Filename is free-form — soltrace keys by the IDL's top-level `address` field,
not by filename. The file must be valid JSON with at least:

```jsonc
{
  "address": "<base58 program id>", // required — used as the lookup key
  "events": [
    /* Anchor event defs */
  ],
  "types": [
    /* referenced structs/enums */
  ],
}
```

## On-chain IDLs (`--onchain-programs`)

As an alternative to dropping IDL files into this directory, soltrace can fetch
IDLs directly from Solana via the
[program-metadata](../../program-metadata/README.md) on-chain program
(`ProgM6JCCvbYkfKqJYHePx4xxSUSqJp7rh8Lyv7nk7S`). Each program's IDL lives at a
canonical PDA derived from seeds `[program, "idl"+zero-pad-16]` under that
program. Pass the programs you want indexed as a comma-separated base58 list:

```bash
# CLI (CLI wins over env)
soltrace-live run --onchain-programs <PROG_A>,<PROG_B> ...

# env
ONCHAIN_PROGRAMS=<PROG_A>,<PROG_B> soltrace-live run ...
```

Invalid base58 entries hard-error at startup (a typo is loud, never silently
dropped). Empty/unset is a no-op — existing operators see no change.

### Priority: file IDLs win

File IDLs in `IDL_DIR` are the primary source. For any program present in both,
the **file IDL wins** and the on-chain account is never even fetched. On-chain
fetch only applies to programs you list that are _not_ installed as files.
`--program-prefixes` applies to on-chain programs identically.

### Direct-only

Only `data_source = Direct` IDLs are read (the default the program-metadata CLI
writes). `Url` / `External` data sources are skipped with a warning — fetch
those and install them as files instead.

### Live vs backfill

- **`soltrace-live`** fetches each listed program's IDL once at startup, then
  keeps a dedicated WebSocket `accountSubscribe` open. When the IDL account is
  updated (`SetData`) the parser is hot-swapped live; when it is frozen
  (`SetImmutable`) soltrace unsubscribes; when it is closed the program is
  dropped from the parser and a warning is logged. On disconnect all
  subscriptions are re-established automatically (capped-backoff reconnect).
- **`soltrace-backfill`** is point-in-time: it does one fetch per listed program
  at startup and never subscribes. An on-chain IDL has no version history
  (`SetData` overwrites in place), so if the IDL changed between a historical
  transaction and backfill time, decoding may produce stale output — the same
  staleness risk you already accept for file IDLs.

On-chain-IDL programs are subscribed to `logs_subscribe` immediately, even
before their IDL arrives via the startup fetch / subscription push. Events for
an as-yet-unfetched IDL hit the unknown-discriminator debug-skip path and start
decoding once the IDL lands.

## Program → prefix mapping

Event rows are named `<prefix>_<EventName>`. The prefix comes from
`PROGRAM_PREFIXES` (env or `--program-prefixes`), format
`<program_id>:<prefix>,<program_id>:<prefix>,...`. Programs absent from the
mapping fall back to the `default` prefix.

## Runtime behaviour with no IDLs

`load_idls` logs a warning and continues. Unknown-discriminator `emit_cpi!`
events are skipped at debug level (never crash). Log-path events
(`Program data: <base64>`) still work for programs whose events are emitted
via `emit!` rather than `emit_cpi!`.
