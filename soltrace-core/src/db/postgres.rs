use crate::{
    db::{event_id_to_hex, generate_event_id, DatabaseBackend, EventRecord},
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
        let id_bytes: Vec<u8> = row.get("id");
        Ok(EventRecord {
            id: hex::encode(&id_bytes),
            slot: row.get("slot"),
            signature: row.get("signature"),
            event_name: row.get("event_name"),
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
                id BYTEA PRIMARY KEY,
                slot BIGINT NOT NULL,
                signature TEXT NOT NULL,
                event_name TEXT NOT NULL,
                data JSONB NOT NULL,
                timestamp TIMESTAMPTZ NOT NULL
            )
        "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_slot ON events(slot)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_event_name ON events(event_name)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_timestamp ON events(timestamp)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_data_gin ON events USING GIN (data)")
            .execute(&self.pool)
            .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_signature ON events(signature)")
            .execute(&self.pool)
            .await?;

        tracing::info!("PostgreSQL migrations completed");
        Ok(())
    }

    async fn insert_event(&self, event: &DecodedEvent, raw: &RawEvent, index: usize) -> Result<String> {
        let id_bytes = generate_event_id(&raw.signature, index, &event.event_name);
        let event_id = event_id_to_hex(&id_bytes);

        sqlx::query(
            r#"
            INSERT INTO events (id, slot, signature, event_name, data, timestamp)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (id) DO NOTHING
        "#,
        )
        .bind(&id_bytes[..])
        .bind(raw.slot as i64)
        .bind(&raw.signature)
        .bind(&event.event_name)
        .bind(&event.data)
        .bind(raw.timestamp)
        .execute(&self.pool)
        .await?;

        Ok(event_id)
    }

    async fn get_events_by_slot_range(
        &self,
        start_slot: Slot,
        end_slot: Slot,
    ) -> Result<Vec<EventRecord>> {
        let rows = sqlx::query(
            "SELECT id, slot, signature, event_name, data, timestamp FROM events WHERE slot >= $1 AND slot <= $2 ORDER BY slot ASC"
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

    async fn get_events_by_name(&self, event_name: &str) -> Result<Vec<EventRecord>> {
        let rows = sqlx::query(
            "SELECT id, slot, signature, event_name, data, timestamp FROM events WHERE event_name = $1 ORDER BY slot DESC"
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

    async fn event_exists(&self, signature: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE signature = $1")
            .bind(signature)
            .fetch_one(&self.pool)
            .await?;

        Ok(count > 0)
    }
}
