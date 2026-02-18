use crate::{
    db::{DatabaseBackend, EventRecord},
    error::Result,
    types::{DecodedEvent, RawEvent, Slot},
};
use async_trait::async_trait;
use chrono::DateTime;
use sqlx::Row;

/// SQLite database backend
#[derive(Clone)]
pub struct SqliteBackend {
    pool: sqlx::sqlite::SqlitePool,
}

impl SqliteBackend {
    pub async fn new(database_url: &str) -> Result<Self> {
        let db_path = database_url.trim_start_matches("sqlite:");
        tracing::info!("Database path: {}", db_path);

        if let Some(parent) = std::path::Path::new(db_path).parent() {
            let parent_str = parent.display().to_string();
            if !parent_str.is_empty() {
                tracing::info!("Creating database directory: {}", parent_str);
                std::fs::create_dir_all(parent)?;
            }
        }

        tracing::info!("Connecting to database: {}", database_url);
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true);

        let pool = sqlx::sqlite::SqlitePool::connect_with(options).await?;

        let db = Self { pool };
        db.run_migrations().await?;

        Ok(db)
    }

    fn parse_timestamp(ts_str: &str) -> Result<chrono::DateTime<chrono::Utc>> {
        DateTime::parse_from_rfc3339(ts_str)
            .map(|dt| dt.into())
            .map_err(|e| crate::error::SoltraceError::Database(format!("Invalid timestamp: {}", e)))
    }
}

#[async_trait]
impl DatabaseBackend for SqliteBackend {
    async fn run_migrations(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                slot INTEGER NOT NULL,
                signature TEXT NOT NULL,
                event_name TEXT NOT NULL,
                discriminator TEXT NOT NULL,
                data TEXT NOT NULL,
                timestamp TEXT NOT NULL
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_signature_unique ON events(signature);

            CREATE INDEX IF NOT EXISTS idx_slot ON events(slot);
            CREATE INDEX IF NOT EXISTS idx_event_name ON events(event_name);
            CREATE INDEX IF NOT EXISTS idx_timestamp ON events(timestamp);
        "#,
        )
        .execute(&self.pool)
        .await?;

        tracing::info!("SQLite migrations completed");
        Ok(())
    }

    async fn insert_event(&self, event: &DecodedEvent, raw: &RawEvent) -> Result<String> {
        let discriminator_hex = hex::encode_upper(event.discriminator);

        let result = sqlx::query(
            r#"
            INSERT INTO events (slot, signature, event_name, discriminator, data, timestamp)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        )
        .bind(raw.slot as i64)
        .bind(&raw.signature)
        .bind(&event.event_name)
        .bind(discriminator_hex)
        .bind(serde_json::to_string(&event.data)?)
        .bind(raw.timestamp.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid().to_string())
    }

    async fn get_events_by_slot_range(
        &self,
        start_slot: Slot,
        end_slot: Slot,
    ) -> Result<Vec<EventRecord>> {
        let rows = sqlx::query(
            "SELECT id, slot, signature, event_name, discriminator, data, timestamp FROM events WHERE slot >= ?1 AND slot <= ?2 ORDER BY slot ASC",
        )
        .bind(start_slot as i64)
        .bind(end_slot as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut events = Vec::new();
        for row in rows {
            events.push(EventRecord {
                id: row.get::<i64, _>("id").to_string(),
                slot: row.get("slot"),
                signature: row.get("signature"),
                event_name: row.get("event_name"),
                discriminator: row.get("discriminator"),
                data: serde_json::from_str(row.get::<String, _>("data").as_str())?,
                timestamp: Self::parse_timestamp(row.get::<String, _>("timestamp").as_str())?,
            });
        }

        Ok(events)
    }

    async fn get_events_by_name(&self, event_name: &str) -> Result<Vec<EventRecord>> {
        let rows = sqlx::query(
            "SELECT id, slot, signature, event_name, discriminator, data, timestamp FROM events WHERE event_name = ?1 ORDER BY slot DESC",
        )
        .bind(event_name)
        .fetch_all(&self.pool)
        .await?;

        let mut events = Vec::new();
        for row in rows {
            events.push(EventRecord {
                id: row.get::<i64, _>("id").to_string(),
                slot: row.get("slot"),
                signature: row.get("signature"),
                event_name: row.get("event_name"),
                discriminator: row.get("discriminator"),
                data: serde_json::from_str(row.get::<String, _>("data").as_str())?,
                timestamp: Self::parse_timestamp(row.get::<String, _>("timestamp").as_str())?,
            });
        }

        Ok(events)
    }

    async fn event_exists(&self, signature: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE signature = ?1")
            .bind(signature)
            .fetch_one(&self.pool)
            .await?;

        Ok(count > 0)
    }
}
