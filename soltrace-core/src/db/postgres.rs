use crate::{
    db::{generate_event_id, DatabaseBackend},
    error::{Result, SoltraceError},
    types::{DecodedEvent, RawEvent},
};
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

/// PostgreSQL database backend with JSONB support (v2 schema: `programs`
/// dimension table + `event_rows` fact table, legacy `events` kept as a VIEW).
pub struct PostgresBackend {
    pool: sqlx::postgres::PgPool,
    /// address -> programs.id cache; misses upsert lazily (first-seen name
    /// wins, so a cached id is stable for the process lifetime).
    program_ids: RwLock<HashMap<String, i32>>,
}

impl PostgresBackend {
    pub async fn new(database_url: &str) -> Result<Self> {
        tracing::info!("Connecting to PostgreSQL database");

        let pool = sqlx::postgres::PgPool::connect(database_url).await?;

        let backend = Self {
            pool,
            program_ids: RwLock::new(HashMap::new()),
        };
        backend.run_migrations().await?;

        Ok(backend)
    }

    /// Resolve (or first-seen create) the programs row for `address`.
    /// First-seen name is immutable by design: renaming would change future
    /// dedup ids and silently duplicate rows on any re-walk.
    async fn ensure_program(&self, address: &str, name: &str) -> Result<i32> {
        if let Some(id) = self.program_ids.read().await.get(address) {
            return Ok(*id);
        }

        let id: Option<i32> = sqlx::query_scalar(
            r#"
            WITH ins AS (
                INSERT INTO programs (address, name) VALUES ($1, $2)
                ON CONFLICT (address) DO NOTHING
                RETURNING id
            )
            SELECT id FROM ins
            UNION ALL
            SELECT id FROM programs WHERE address = $1
            LIMIT 1
            "#,
        )
        .bind(address)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        let id = id.ok_or_else(|| {
            SoltraceError::Database(format!("programs row missing for {}", address))
        })?;
        self.program_ids
            .write()
            .await
            .insert(address.to_string(), id);
        Ok(id)
    }

    async fn try_enable_timescaledb(&self) -> Result<()> {
        match sqlx::query("CREATE EXTENSION IF NOT EXISTS timescaledb")
            .execute(&self.pool)
            .await
        {
            Ok(_) => {
                tracing::info!("TimescaleDB extension enabled");

                match sqlx::query(
                    "SELECT create_hypertable('event_rows', 'timestamp', chunk_time_interval => interval '1 day', if_not_exists => TRUE)",
                )
                .execute(&self.pool)
                .await
                {
                    Ok(_) => {
                        tracing::info!("TimescaleDB hypertable created successfully");
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "Failed to create TimescaleDB hypertable, continuing with regular PostgreSQL"
                        );
                    }
                }
            }
            Err(e) => {
                tracing::info!(
                    error = %e,
                    "TimescaleDB not available, using standard PostgreSQL"
                );
            }
        }

        Ok(())
    }
}

#[async_trait]
impl DatabaseBackend for PostgresBackend {
    async fn run_migrations(&self) -> Result<()> {
        sqlx::migrate!()
            .run(&self.pool)
            .await
            .map_err(|e| SoltraceError::Database(format!("migration failed: {}", e)))?;

        self.try_enable_timescaledb().await?;

        tracing::info!("PostgreSQL migrations completed");
        Ok(())
    }

    /// Register operator mappings (--program-prefixes) and rewrite legacy
    /// prefixed rows. Idempotent and self-extinguishing: once no
    /// `program_id IS NULL` row matches a known name, it rewrites nothing.
    async fn seed_programs(&self, mappings: &HashMap<String, String>) -> Result<()> {
        for (address, name) in mappings {
            self.ensure_program(address, name).await?;
        }

        // Exact left() prefix match — LIKE is unusable here because names are
        // operator freeform and '_'/'%' are wildcards.
        let result = sqlx::query(
            r#"
            WITH matched AS (
                SELECT e.id AS event_id,
                       p.id AS program_id,
                       substring(e.event_name from length(p.name) + 2) AS bare_name
                FROM event_rows e
                JOIN programs p
                  ON left(e.event_name, length(p.name) + 1) = p.name || '_'
                WHERE e.program_id IS NULL
            )
            UPDATE event_rows e
            SET program_id = m.program_id,
                event_name = m.bare_name
            FROM matched m
            WHERE e.id = m.event_id
            "#,
        )
        .execute(&self.pool)
        .await?;

        tracing::info!(
            rows = result.rows_affected(),
            "Backfilled legacy prefixed event rows"
        );
        Ok(())
    }

    async fn insert_event(
        &self,
        event: &DecodedEvent,
        raw: &RawEvent,
        index: usize,
    ) -> Result<String> {
        // Hash input stays the legacy composed name so ids are identical to
        // pre-migration rows for the same emission — re-walks dedup across
        // the v1→v2 cutover.
        let id_bytes = generate_event_id(&raw.signature, index, &event.event_name);
        let event_id = hex::encode(id_bytes);

        // Configured programs were seeded by seed_programs with their
        // operator name (first-seen wins there); anything else is named by
        // its full base58 address — base58 excludes '_', so the VIEW name
        // stays splittable.
        let address = raw.program_id.to_string();
        let program_id = self.ensure_program(&address, &address).await?;

        sqlx::query(
            r#"
            INSERT INTO event_rows (id, slot, signature, event_name, program_id, data, timestamp)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (id) DO NOTHING
        "#,
        )
        .bind(&id_bytes[..])
        .bind(raw.slot as i64)
        .bind(&raw.signature)
        .bind(&event.event_name_bare)
        .bind(program_id)
        .bind(&event.data)
        .bind(raw.timestamp)
        .execute(&self.pool)
        .await?;

        Ok(event_id)
    }

    async fn get_latest_signature(&self) -> Result<Option<String>> {
        let result: Option<String> = sqlx::query_scalar(
            "SELECT signature FROM event_rows ORDER BY slot DESC, timestamp DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ids must not change across the v1→v2 cutover: the hash input is the
    /// composed name, not the bare name now stored in event_rows.
    #[test]
    fn dedup_id_hashes_composed_name() {
        let id = generate_event_id("sig", 3, "tributary_PaymentCollected");
        let bare_id = generate_event_id("sig", 3, "PaymentCollected");
        assert_ne!(id, bare_id);
        assert_eq!(id.len(), 32);
    }

    /// Full v1→v2 cutover rehearsal against a scratch database. Gated on
    /// SOLTRACE_PG_TEST_URL (e.g. postgres://postgres:soltrace@localhost:55432/soltrace_test);
    /// silently skips otherwise so the suite stays hermetic.
    #[tokio::test]
    async fn migration_cutover_end_to_end() {
        let Ok(url) = std::env::var("SOLTRACE_PG_TEST_URL") else {
            return;
        };
        // Wipe the scratch schema; URL must point at a dedicated test db.
        let reset = sqlx::postgres::PgPool::connect(&url).await.unwrap();
        sqlx::query("DROP SCHEMA public CASCADE")
            .execute(&reset)
            .await
            .unwrap();
        sqlx::query("CREATE SCHEMA public")
            .execute(&reset)
            .await
            .unwrap();
        reset.close().await;

        // -- 1. Simulate a legacy v1 deployment with a prefixed row. Id is the
        // legacy hash so cutover dedup is exercised.
        let legacy_id = generate_event_id("legacysig", 0, "tributary_PaymentCollected");
        let pool = sqlx::postgres::PgPool::connect(&url).await.unwrap();
        sqlx::query(
            r#"
            CREATE TABLE events (
                id BYTEA PRIMARY KEY, slot BIGINT NOT NULL,
                signature VARCHAR(88) NOT NULL, event_name VARCHAR(64) NOT NULL,
                data JSONB NOT NULL, timestamp TIMESTAMPTZ NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO events VALUES ($1, 100, 'legacysig', 'tributary_PaymentCollected', '{}', now())")
            .bind(&legacy_id[..])
            .execute(&pool)
            .await
            .unwrap();
        // Unmapped legacy row — must stay prefixed + NULL forever.
        sqlx::query(
            "INSERT INTO events VALUES ($1, 100, 'legacysig', 'default_Unknown', '{}', now())",
        )
        .bind([7u8; 32])
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        // -- 2. create_backend runs 0001 (no-op) + 0002 (rename, FK, view).
        let backend = PostgresBackend::new(&url).await.unwrap();

        let table: String = sqlx::query_scalar(
            "SELECT table_type FROM information_schema.tables WHERE table_name = 'events'",
        )
        .fetch_one(&backend.pool)
        .await
        .unwrap();
        assert_eq!(table, "VIEW");

        // -- 3. Seed + backfill: configured prefix rewrites, default row left alone.
        let mut mappings = HashMap::new();
        mappings.insert(
            "TRibg8W8zmPHQqWtyAD1rEBRXEdyU13Mu6qX1Sg42tJ".to_string(),
            "tributary".to_string(),
        );
        backend.seed_programs(&mappings).await.unwrap();

        let (name, program): (String, Option<i32>) = sqlx::query_as(
            "SELECT event_name, program_id FROM event_rows WHERE signature = 'legacysig' AND event_name NOT LIKE 'default%'",
        )
        .fetch_one(&backend.pool)
        .await
        .unwrap();
        assert_eq!(name, "PaymentCollected");
        assert!(program.is_some());

        let (name, program): (String, Option<i32>) = sqlx::query_as(
            "SELECT event_name, program_id FROM event_rows WHERE event_name LIKE 'default%'",
        )
        .fetch_one(&backend.pool)
        .await
        .unwrap();
        assert_eq!(name, "default_Unknown");
        assert!(program.is_none());

        // -- 4. Re-insert the SAME emission via the API: legacy id conflicts,
        // no duplicate row (cutover dedup continuity).
        let event = DecodedEvent {
            event_name: "tributary_PaymentCollected".to_string(),
            event_name_bare: "PaymentCollected".to_string(),
            data: serde_json::json!({}),
            discriminator: [0u8; 8],
        };
        let raw = RawEvent {
            slot: 100,
            signature: "legacysig".to_string(),
            program_id: "TRibg8W8zmPHQqWtyAD1rEBRXEdyU13Mu6qX1Sg42tJ"
                .parse()
                .unwrap(),
            log: String::new(),
            timestamp: chrono::Utc::now(),
        };
        backend.insert_event(&event, &raw, 0).await.unwrap();
        let (count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM event_rows WHERE signature = 'legacysig'")
                .fetch_one(&backend.pool)
                .await
                .unwrap();
        assert_eq!(count, 2, "same emission must not duplicate across cutover");

        // -- 5. VIEW reconstructs the legacy shape for both row kinds.
        let names: Vec<String> = sqlx::query_scalar(
            "SELECT event_name FROM events WHERE signature = 'legacysig' ORDER BY event_name",
        )
        .fetch_all(&backend.pool)
        .await
        .unwrap();
        assert!(names.contains(&"tributary_PaymentCollected".to_string()));
        assert!(names.contains(&"default_Unknown".to_string()));

        // -- 6. New program with no mapping is named by its address.
        let raw_unmapped = RawEvent {
            program_id: "9FD4uty8Qu1zWj6sf9R3TakZ6wrB8bfMjDt2wrBeBm5v"
                .parse()
                .unwrap(),
            signature: "newsig".to_string(),
            ..raw
        };
        backend
            .insert_event(&event, &raw_unmapped, 0)
            .await
            .unwrap();
        let (name,): (String,) = sqlx::query_as(
            "SELECT p.name FROM event_rows e JOIN programs p ON p.id = e.program_id WHERE e.signature = 'newsig'",
        )
        .fetch_one(&backend.pool)
        .await
        .unwrap();
        assert_eq!(name, "9FD4uty8Qu1zWj6sf9R3TakZ6wrB8bfMjDt2wrBeBm5v");

        // -- 7. Resume query retargeted.
        assert!(backend.get_latest_signature().await.unwrap().is_some());
    }
}
