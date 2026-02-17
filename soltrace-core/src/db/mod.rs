use crate::{
    error::Result,
    types::{DecodedEvent, RawEvent, Slot},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Event record stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub id: String,
    pub slot: i64,
    pub signature: String,
    pub program_id: String,
    pub event_name: String,
    pub discriminator: String,
    pub data: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

/// Trait defining the database backend interface
#[async_trait]
pub trait DatabaseBackend: Send + Sync {
    /// Run database migrations/schema setup
    async fn run_migrations(&self) -> Result<()>;

    /// Store a decoded event
    async fn insert_event(&self, event: &DecodedEvent, raw: &RawEvent) -> Result<String>;

    /// Get events by slot range
    async fn get_events_by_slot_range(
        &self,
        start_slot: Slot,
        end_slot: Slot,
    ) -> Result<Vec<EventRecord>>;

    /// Get events by program
    async fn get_events_by_program(&self, program_id: &str) -> Result<Vec<EventRecord>>;

    /// Get events by event name
    async fn get_events_by_name(&self, event_name: &str) -> Result<Vec<EventRecord>>;

    /// Get the latest indexed slot for a program
    async fn get_latest_slot(&self, program_id: &str) -> Result<Option<Slot>>;

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

    pub async fn insert_event(&self, event: &DecodedEvent, raw: &RawEvent) -> Result<String> {
        self.backend.insert_event(event, raw).await
    }

    pub async fn get_events_by_slot_range(
        &self,
        start_slot: Slot,
        end_slot: Slot,
    ) -> Result<Vec<EventRecord>> {
        self.backend.get_events_by_slot_range(start_slot, end_slot).await
    }

    pub async fn get_events_by_program(&self, program_id: &str) -> Result<Vec<EventRecord>> {
        self.backend.get_events_by_program(program_id).await
    }

    pub async fn get_events_by_name(&self, event_name: &str) -> Result<Vec<EventRecord>> {
        self.backend.get_events_by_name(event_name).await
    }

    pub async fn get_latest_slot(&self, program_id: &str) -> Result<Option<Slot>> {
        self.backend.get_latest_slot(program_id).await
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
