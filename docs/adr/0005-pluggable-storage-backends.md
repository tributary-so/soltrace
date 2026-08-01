# Pluggable storage backends over a JSON-denominator schema

Status: accepted

Soltrace supports three storage backends — SQLite, PostgreSQL, and MongoDB —
auto-selected by the URL scheme of `DB_URL`, behind a single `Database` trait.
The stored Event Row carries the decoded event as a JSON value; each backend
maps that JSON to its native type (SQLite: JSON text; PostgreSQL: `JSONB` +
GIN index; MongoDB: a nested document).

The rejected alternative was committing to one backend (or to a fixed
relational schema). It was rejected because deployment targets differ — a
single-process operator is fine with embedded SQLite, while a high-throughput
consumer wants Postgres' JSONB queries or Mongo's document model. The cost is
keeping three `init`/`insert` implementations in sync; the payoff is no
operator is forced onto a backend that doesn't fit, and the JSON denominator
keeps the decoded payload queryable everywhere.

## Consequences

- Adding a column or changing the schema means updating **all three** backends
  plus the SQLite `CREATE TABLE`; they must stay in lockstep.
- The decoded `data` field is JSON everywhere, so u64/u128/i64/i128 serialize
  as JSON **strings** (lossless) while smaller ints serialize as JSON numbers.
  This asymmetry is deliberate and must not be "fixed" per-backend.
- SQLite's database-level locking can surface as `database is locked` under
  concurrency; the escape hatch is Postgres or Mongo, not app-level locking.
