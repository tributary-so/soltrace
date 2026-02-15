use crate::{
    error::Result,
    types::{Slot, DecodedEvent, RawEvent},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;

/// Event record stored in the database
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EventRecord {
    pub id: i64,
    pub slot: Slot,
    pub signature: String,
    pub program_id: String,
    pub event_name: String,
    pub discriminator: String, // Hex-encoded
    pub data: String,          // JSON-encoded
    pub timestamp: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

pub struct Database {
    pool: sqlx::sqlite::SqlitePool,
}

impl Database {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = sqlx::sqlite::SqlitePool::connect(database_url).await?;

        let db = Self { pool };
        db.run_migrations().await?;

        Ok(db)
    }

    pub async fn run_migrations(&self) -> Result<()> {
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                slot INTEGER NOT NULL,
                signature TEXT NOT NULL,
                program_id TEXT NOT NULL,
                event_name TEXT NOT NULL,
                discriminator TEXT NOT NULL,
                data TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('utc'))
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_signature_unique ON events(signature);

            CREATE INDEX IF NOT EXISTS idx_slot ON events(slot);
            CREATE INDEX IF NOT EXISTS idx_program_id ON events(program_id);
            CREATE INDEX IF NOT EXISTS idx_event_name ON events(event_name);
            CREATE INDEX IF NOT EXISTS idx_timestamp ON events(timestamp);
        "#)
        .execute(&self.pool)
        .await?;

        tracing::info!("Database migrations completed");
        Ok(())
    }

    /// Store a decoded event
    pub async fn insert_event(&self, event: &DecodedEvent, raw: &RawEvent) -> Result<i64> {
        let discriminator_hex = hex::encode_upper(&event.data);

        let result = sqlx::query(r#"
            INSERT INTO events (slot, signature, program_id, event_name, discriminator, data, timestamp)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#)
        .bind(raw.slot)
        .bind(&raw.signature)
        .bind(raw.program_id.to_string())
        .bind(&event.event_name)
        .bind(discriminator_hex)
        .bind(serde_json::to_string(&event.data)?)
        .bind(raw.timestamp.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid() as i64)
    }

    /// Get events by slot range
    pub async fn get_events_by_slot_range(
        &self,
        start_slot: Slot,
        end_slot: Slot,
    ) -> Result<Vec<EventRecord>> {
        let events = sqlx::query_as::<_, EventRecord>(
            "SELECT * FROM events WHERE slot >= ?1 AND slot <= ?2 ORDER BY slot ASC"
        )
        .bind(start_slot)
        .bind(end_slot)
        .fetch_all(&self.pool)
        .await?;

        Ok(events)
    }

    /// Get events by program
    pub async fn get_events_by_program(&self, program_id: &str) -> Result<Vec<EventRecord>> {
        let events = sqlx::query_as::<_, EventRecord>(
            "SELECT * FROM events WHERE program_id = ?1 ORDER BY slot DESC"
        )
        .bind(program_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(events)
    }

    /// Get events by event name
    pub async fn get_events_by_name(&self, event_name: &str) -> Result<Vec<EventRecord>> {
        let events = sqlx::query_as::<_, EventRecord>(
            "SELECT * FROM events WHERE event_name = ?1 ORDER BY slot DESC"
        )
        .bind(event_name)
        .fetch_all(&self.pool)
        .await?;

        Ok(events)
    }

    /// Get the latest indexed slot for a program
    pub async fn get_latest_slot(&self, program_id: &str) -> Result<Option<Slot>> {
        let row = sqlx::query(
            "SELECT MAX(slot) as max_slot FROM events WHERE program_id = ?1"
        )
        .bind(program_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.and_then(|r| r.get("max_slot")))
    }

    /// Check if an event already exists (by signature)
    pub async fn event_exists(&self, signature: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE signature = ?1")
            .bind(signature)
            .fetch_one(&self.pool)
            .await?;

        Ok(count > 0)
    }
}
