# Event Row dedup key and idempotent reprocessing

Status: accepted

Every Event Row is identified by the tuple `(signature, index, event_name)`,
and inserts use `ON CONFLICT DO NOTHING`. Reprocessing a signature is therefore
idempotent by design — `UNIQUE constraint failed` is the normal, expected
outcome of overlapping Live catch-up, Backfill, and re-runs, not a bug to
suppress or retry around.

The non-obvious part is `index`. It must distinguish every event a signature
carries, including events from both emission paths (ADR-0002), while remaining
a single integer column:

- **log-path** `index` = a sequential counter starting at 0;
- **CPI-path** `index` = `1_000_000_000 + outer_ix * 100_000 + inner_ix`.

The 1,000,000,000 offset places the two paths in disjoint ranges so they can
never collide; the 100,000 stride between outer instructions is far above any
real transaction's inner-instruction count (capped by Solana's ~1232-byte
transaction limit). A composite `(path, outer_ix, inner_ix)` column was
rejected because a single integer keeps the uniqueness constraint and the
log/CPI code paths symmetric and trivially comparable.

## Consequences

- Live catch-up, Backfill, and re-indexing can run concurrently and overlap
  freely; duplicate rows are silently dropped.
- The offset constants are load-bearing: changing them would orphan every
  existing CPI-path row and break idempotency until a full re-index.
- Operators must not interpret `index` as a per-transaction ordinal without
  accounting for the CPI offset.
