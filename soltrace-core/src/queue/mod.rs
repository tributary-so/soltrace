use async_trait::async_trait;
use serde::Serialize;

#[cfg(feature = "kafka")]
pub mod kafka;

#[derive(Debug, Clone, Serialize)]
pub struct QueueEvent {
    pub event_name: String,
    pub signature: String,
    pub program_id: String,
    pub data: serde_json::Value,
    pub timestamp: String,
}

impl QueueEvent {
    pub fn new(
        event_name: String,
        signature: String,
        program_id: String,
        data: serde_json::Value,
    ) -> Self {
        Self {
            event_name,
            signature,
            program_id,
            data,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[async_trait]
pub trait EventQueue: Send + Sync {
    async fn send(&self, event: &QueueEvent) -> anyhow::Result<()>;
    async fn flush(&self) -> anyhow::Result<()>;
}
