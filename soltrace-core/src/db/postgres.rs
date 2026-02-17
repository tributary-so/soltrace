use crate::{
    db::{DatabaseBackend, EventRecord},
    error::Result,
    types::{DecodedEvent, RawEvent, Slot},
};
use async_trait::async_trait;
use sqlx::Row;

/// PostgreSQL database backend with JSONB support
#[derive(Clone)]
pub struct PostgresBackend {
    pool: sqlx::postgres::PgPool,
}

impl PostgresBackend {
    pub async fn new(database_url: &str) -> Result<Self> {
        tracing::info!("Connecting to PostgreSQL database");
        
        let pool = sqlx::postgres::PgPool::connect(database_url).await?;
        
        let backend = Self { pool };
        backend.run_migrations().await?;
        
        Ok(backend)
    }

    fn row_to_event_record(&self, row: sqlx::postgres::PgRow) -> Result<EventRecord> {
        Ok(EventRecord {
            id: row.get::<i64, _>("id").to_string(),
            slot: row.get("slot"),
            signature: row.get("signature"),
            program_id: row.get("program_id"),
            event_name: row.get("event_name"),
            discriminator: row.get("discriminator"),
            data: row.get::<serde_json::Value, _>("data"),
            timestamp: row.get("timestamp"),
        })
    }
}

#[async_trait]
impl DatabaseBackend for PostgresBackend {
    async fn run_migrations(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS events (
                id BIGSERIAL PRIMARY KEY,
                slot BIGINT NOT NULL,
                signature TEXT NOT NULL UNIQUE,
                program_id TEXT NOT NULL,
                event_name TEXT NOT NULL,
                discriminator TEXT NOT NULL,
                data JSONB NOT NULL,
                timestamp TIMESTAMPTZ NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_slot ON events(slot);
            CREATE INDEX IF NOT EXISTS idx_program_id ON events(program_id);
            CREATE INDEX IF NOT EXISTS idx_event_name ON events(event_name);
            CREATE INDEX IF NOT EXISTS idx_timestamp ON events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_data_gin ON events USING GIN (data);
        "#,
        )
        .execute(&self.pool)
        .await?;

        tracing::info!("PostgreSQL migrations completed");
        Ok(())
    }

    async fn insert_event(&self, event: &DecodedEvent, raw: &RawEvent) -> Result<String> {
        let discriminator_hex = hex::encode_upper(event.discriminator);

        let result = sqlx::query(r#"
            INSERT INTO events (slot, signature, program_id, event_name, discriminator, data, timestamp)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
        "#)
        .bind(raw.slot as i64)
        .bind(&raw.signature)
        .bind(raw.program_id.to_string())
        .bind(&event.event_name)
        .bind(discriminator_hex)
        .bind(&event.data)
        .bind(raw.timestamp)
        .fetch_one(&self.pool)
        .await?;

        let id: i64 = result.get("id");
        Ok(id.to_string())
    }

    async fn get_events_by_slot_range(
        &self,
        start_slot: Slot,
        end_slot: Slot,
    ) -> Result<Vec<EventRecord>> {
        let rows = sqlx::query(
            "SELECT id, slot, signature, program_id, event_name, discriminator, data, timestamp FROM events WHERE slot >= $1 AND slot <= $2 ORDER BY slot ASC"
        )
        .bind(start_slot as i64)
        .bind(end_slot as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut events = Vec::new();
        for row in rows {
            events.push(self.row_to_event_record(row)?);
        }

        Ok(events)
    }

    async fn get_events_by_program(&self, program_id: &str) -> Result<Vec<EventRecord>> {
        let rows = sqlx::query(
            "SELECT id, slot, signature, program_id, event_name, discriminator, data, timestamp FROM events WHERE program_id = $1 ORDER BY slot DESC"
        )
        .bind(program_id)
        .fetch_all(&self.pool)
        .await?;

        let mut events = Vec::new();
        for row in rows {
            events.push(self.row_to_event_record(row)?);
        }

        Ok(events)
    }

    async fn get_events_by_name(&self, event_name: &str) -> Result<Vec<EventRecord>> {
        let rows = sqlx::query(
            "SELECT id, slot, signature, program_id, event_name, discriminator, data, timestamp FROM events WHERE event_name = $1 ORDER BY slot DESC"
        )
        .bind(event_name)
        .fetch_all(&self.pool)
        .await?;

        let mut events = Vec::new();
        for row in rows {
            events.push(self.row_to_event_record(row)?);
        }

        Ok(events)
    }

    async fn get_latest_slot(&self, program_id: &str) -> Result<Option<Slot>> {
        let row = sqlx::query("SELECT MAX(slot) as max_slot FROM events WHERE program_id = $1")
            .bind(program_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row
            .and_then(|r| r.get::<Option<i64>, _>("max_slot"))
            .map(|s| s as Slot))
    }

    async fn event_exists(&self, signature: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE signature = $1")
            .bind(signature)
            .fetch_one(&self.pool)
            .await?;

        Ok(count > 0)
    }
}
