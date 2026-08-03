use crate::{
    error::{Result, SoltraceError},
    types::{DecodedEvent, RawEvent, Slot},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub fn generate_event_id(signature: &str, index: usize, event_type: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}_{}_{}", signature, index, event_type));
    let result = hasher.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&result);
    bytes
}

/// Event record stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub id: String,
    pub slot: i64,
    pub signature: String,
    pub event_name: String,
    pub data: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

/// Trait defining the database backend interface
#[async_trait]
pub trait DatabaseBackend: Send + Sync {
    /// Run database migrations/schema setup
    async fn run_migrations(&self) -> Result<()>;

    /// Store a decoded event
    async fn insert_event(&self, event: &DecodedEvent, raw: &RawEvent, index: usize) -> Result<String>;

    /// Get events by slot range
    async fn get_events_by_slot_range(
        &self,
        start_slot: Slot,
        end_slot: Slot,
    ) -> Result<Vec<EventRecord>>;

    /// Get events by event name
    async fn get_events_by_name(&self, event_name: &str) -> Result<Vec<EventRecord>>;

    /// Check if an event already exists (by signature)
    async fn event_exists(&self, signature: &str) -> Result<bool>;

    /// Get the most recent signature stored in the database
    /// Returns None if no events exist
    async fn get_latest_signature(&self) -> Result<Option<String>>;
}

/// Handle to a configured database backend. Cloneable (Arc) and cheap to share
/// across tasks.
pub type Database = Arc<dyn DatabaseBackend>;

/// Create a database backend based on the URL scheme.
pub async fn create_backend(database_url: &str) -> Result<Database> {
    if database_url.starts_with("sqlite:") {
        Ok(Arc::new(sqlite::SqliteBackend::new(database_url).await?))
    } else if database_url.starts_with("postgres://") || database_url.starts_with("postgresql://") {
        Ok(Arc::new(postgres::PostgresBackend::new(database_url).await?))
    } else if database_url.starts_with("mongodb://") || database_url.starts_with("mongodb+srv://") {
        Ok(Arc::new(mongodb::MongoDbBackend::new(database_url).await?))
    } else {
        Err(SoltraceError::Database(format!(
            "Unsupported database URL scheme. Expected sqlite:, postgres://, or mongodb://, got: {}",
            database_url
        )))
    }
}

pub mod mongodb;
pub mod postgres;
pub mod sqlite;
