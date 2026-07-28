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
