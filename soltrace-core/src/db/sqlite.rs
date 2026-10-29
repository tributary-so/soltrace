use crate::{
    db::{generate_event_id, DatabaseBackend},
    error::Result,
    types::{DecodedEvent, RawEvent},
};
use async_trait::async_trait;

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
}
#[async_trait]
impl DatabaseBackend for SqliteBackend {
    async fn run_migrations(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS events (
                id BLOB PRIMARY KEY,
                slot INTEGER NOT NULL,
                signature TEXT NOT NULL,
                event_name TEXT NOT NULL,
                data TEXT NOT NULL,
                timestamp TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_slot ON events(slot);
            CREATE INDEX IF NOT EXISTS idx_event_name ON events(event_name);
            CREATE INDEX IF NOT EXISTS idx_timestamp ON events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_signature ON events(signature);
        "#,
        )
        .execute(&self.pool)
        .await?;

        tracing::info!("SQLite migrations completed");
        Ok(())
    }

    async fn insert_event(
        &self,
        event: &DecodedEvent,
        raw: &RawEvent,
        index: usize,
    ) -> Result<String> {
        let id_bytes = generate_event_id(&raw.signature, index, &event.event_name);
        let event_id = hex::encode(id_bytes);

        sqlx::query(
            r#"
            INSERT OR IGNORE INTO events (id, slot, signature, event_name, data, timestamp)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        )
        .bind(&id_bytes[..])
        .bind(raw.slot as i64)
        .bind(&raw.signature)
        .bind(&event.event_name)
        .bind(serde_json::to_string(&event.data)?)
        .bind(raw.timestamp.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(event_id)
    }

    async fn get_latest_signature(&self) -> Result<Option<String>> {
        let result: Option<String> = sqlx::query_scalar(
            "SELECT signature FROM events ORDER BY slot DESC, timestamp DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }
}
