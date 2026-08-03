---
# soltrace-nrgn
title: Delete dead ParsedIdl fields + IdlMetadata
status: completed
type: task
priority: normal
tags:
    - ponytail-audit
    - delete
created_at: 2026-07-28T07:56:59Z
updated_at: 2026-07-28T08:18:17Z
parent: soltrace-qwu9
---

See milestone soltrace-kok1 HANDOFF.

## Cut

In `soltrace-core/src/types.rs`, the following fields on `ParsedIdl` are deserialized but never read anywhere in the workspace:

- `instructions: Option<serde_json::Value>`
- `accounts: Option<serde_json::Value>`
- `errors: Option<serde_json::Value>`
- `metadata: Option<IdlMetadata>`

And the `IdlMetadata` struct (only used by the `metadata` field being deleted) — fields `version`, `name`, `spec`, `description`, all unread.

Keep `version`, `name`, `address`, `events`, `types` — those ARE read (idl.rs uses types and events).

## Files

- `soltrace-core/src/types.rs` (edit)

## Verify

- `rg '\.instructions|\.accounts\b|\.errors\b|IdlMetadata' soltrace-core/src` → empty (excluding InnerInstructionInfo.accounts which is a different struct's field)
- `cargo test --workspace` green (idl parsing tests still pass)
- ~25 lines removed
