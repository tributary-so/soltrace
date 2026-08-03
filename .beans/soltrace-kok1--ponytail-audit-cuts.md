---
# soltrace-kok1
title: Ponytail audit cuts
status: todo
type: milestone
created_at: 2026-07-28T07:56:23Z
updated_at: 2026-07-28T07:56:23Z
---

# Ponytail audit cuts

Repo-wide over-engineering audit (ponytail-audit skill) identified ~910 removable lines and 1 removable dependency (`url`) across `soltrace-core`. Correctness, security, and performance are explicitly out of scope — this milestone is **cuts only**. Each task is one finding from the audit, ranked biggest-cut-first.

Scope guardrails:
- Do NOT refactor working code beyond the named cut.
- Do NOT touch `program-metadata/` — audit covered the `soltrace-*` workspace only.
- Each cut ships independently; tasks are NOT serialized unless a task body says so.

## HANDOFF

### 1. Happy Path

1. Pick a `todo` task under one of the two epics.
2. Apply the cut described in the task body (delete file / drop fn / inline wrapper).
3. `cargo build --workspace` + `cargo test --workspace` green.
4. `beans update <id> -s completed` (or hand to reviewer per hordr role flow).

### 2. Data Contract

- Audit source: conversation transcript in this session (ponytail-audit skill output).
- Public surface removed must be cross-checked with `rg` before deletion — audit already verified zero non-test consumers, but re-verify on the day of the cut.
- `lib.rs` re-exports must be dropped alongside their modules.

### 3. Edge Cases & Constraints

- `sqlx` feature-split (sqlite vs postgres behind cargo features) is **out of scope** — refactor, not a cut.
- Do not collapse the three DB backends into one; only the `Database` wrapper struct / `factory.rs` indirection is in scope.
- `EventQueue` trait removal touches `soltrace-live` signatures (`Option<Arc<dyn EventQueue>>` → `Option<Arc<KafkaProducer>>` cfg-gated) — update all 4 sites in main.rs.
- Tests referencing deleted code (e.g. `metrics::tests`, `validation::tests`, retry dead-fn tests) get deleted with their subjects.

### 4. Business Logic (pseudo-code, target language)

```
// per task:
verify_no_consumers(symbol) -> delete symbol -> cargo build && cargo test
```

### 5. Definition of Done

- [ ] `cargo build --workspace --all-features` green
- [ ] `cargo test --workspace` green
- [ ] No new warnings introduced by the cut
- [ ] `rg` confirms zero dangling references to the removed symbol
- [ ] Milestone closes when `wc -l` of `soltrace-core/src` drops by the claimed amount (~910 lines)

### 6. Test Matrix (Given / When / Then)

- Given metrics.rs is deleted, When `cargo build -p soltrace-live`, Then build succeeds (proves no consumer).
- Given `event_id_to_hex` is inlined, When `rg event_id_to_hex`, Then zero matches.
- Given `url` dep is dropped, When `cargo tree -p soltrace-core | grep url`, Then empty.
- Given `Database` wrapper is removed, When callers hold `Arc<dyn DatabaseBackend>`, Then `cargo test` still passes the dedup/idempotency tests in utils.rs.

### 7. Open Questions

- Should `Database` wrapper removal also drop `DatabaseBackend` trait and use a concrete enum? **No** — out of scope, three backends still need the trait. Only the wrapper struct goes.
- Feature-gate sqlx backends? **Deferred** — separate milestone if compile time becomes a pain.
- Replace `eprintln!` at `idl.rs:101` with `tracing::warn!`? **Out of scope** — that's a correctness/logging fix, not a cut.
