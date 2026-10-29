use crate::{
    error::{Result, SoltraceError},
    types::{DecodedEvent, RawEvent},
};
use async_trait::async_trait;
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

/// Trait defining the database backend interface
// clippy's double_must_use misfires on the async-trait desugaring
// (Result + Box<Future> are both must_use).
#[allow(clippy::double_must_use)]
#[async_trait]
pub trait DatabaseBackend: Send + Sync {
    /// Run database migrations/schema setup
    async fn run_migrations(&self) -> Result<()>;

    /// Register operator program-name mappings and backfill legacy prefixed
    /// rows (postgres v2 schema). Default no-op: backends frozen on the v1
    /// schema have nothing to seed.
    async fn seed_programs(
        &self,
        _mappings: &std::collections::HashMap<String, String>,
    ) -> Result<()> {
        Ok(())
    }

    /// Store a decoded event
    async fn insert_event(
        &self,
        event: &DecodedEvent,
        raw: &RawEvent,
        index: usize,
    ) -> Result<String>;

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
        Ok(Arc::new(
            postgres::PostgresBackend::new(database_url).await?,
        ))
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
