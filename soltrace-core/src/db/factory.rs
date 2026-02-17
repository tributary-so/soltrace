use crate::error::{Result, SoltraceError};
use std::sync::Arc;

use super::DatabaseBackend;

/// Create a database backend based on the URL scheme
pub async fn create_backend(database_url: &str) -> Result<Arc<dyn DatabaseBackend>> {
    if database_url.starts_with("sqlite:") {
        let backend = super::sqlite::SqliteBackend::new(database_url).await?;
        Ok(Arc::new(backend))
    } else if database_url.starts_with("postgres://") || database_url.starts_with("postgresql://") {
        let backend = super::postgres::PostgresBackend::new(database_url).await?;
        Ok(Arc::new(backend))
    } else if database_url.starts_with("mongodb://") || database_url.starts_with("mongodb+srv://") {
        let backend = super::mongodb::MongoDbBackend::new(database_url).await?;
        Ok(Arc::new(backend))
    } else {
        Err(SoltraceError::Database(format!(
            "Unsupported database URL scheme. Expected sqlite:, postgres://, or mongodb://, got: {}",
            database_url
        )))
    }
}
