use crate::{
    error::Result,
    types::{DecodedEvent, RawEvent, Slot},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub fn generate_event_id(signature: &str, index: usize, event_type: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}_{}_{}", signature, index, event_type));
    let result = hasher.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&result);
    bytes
}

pub fn event_id_to_hex(id: &[u8; 32]) -> String {
    hex::encode(id)
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
}

/// Database wrapper that holds a dynamic backend
#[derive(Clone)]
pub struct Database {
    backend: std::sync::Arc<dyn DatabaseBackend>,
}

impl Database {
    /// Create a new database instance by parsing the URL scheme
    pub async fn new(database_url: &str) -> Result<Self> {
        let backend = crate::db::factory::create_backend(database_url).await?;
        Ok(Self { backend })
    }

    pub async fn run_migrations(&self) -> Result<()> {
        self.backend.run_migrations().await
    }

    pub async fn insert_event(&self, event: &DecodedEvent, raw: &RawEvent, index: usize) -> Result<String> {
        self.backend.insert_event(event, raw, index).await
    }

    pub async fn get_events_by_slot_range(
        &self,
        start_slot: Slot,
        end_slot: Slot,
    ) -> Result<Vec<EventRecord>> {
        self.backend
            .get_events_by_slot_range(start_slot, end_slot)
            .await
    }

    pub async fn get_events_by_name(&self, event_name: &str) -> Result<Vec<EventRecord>> {
        self.backend.get_events_by_name(event_name).await
    }

    pub async fn event_exists(&self, signature: &str) -> Result<bool> {
        self.backend.event_exists(signature).await
    }
}

pub mod factory;
pub mod mongodb;
pub mod postgres;
pub mod sqlite;

pub use factory::create_backend;
