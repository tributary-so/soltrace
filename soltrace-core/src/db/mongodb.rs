use crate::{
    db::{DatabaseBackend, EventRecord},
    error::{Result, SoltraceError},
    types::{DecodedEvent, RawEvent, Slot},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use mongodb::{bson, bson::doc, options::IndexOptions, Client, Collection, IndexModel};
use serde::{Deserialize, Serialize};

/// MongoDB document structure for events
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EventDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    id: Option<bson::oid::ObjectId>,
    slot: i64,
    signature: String,
    event_name: String,
    discriminator: String,
    /// Nested data structure - not JSON string but actual document
    data: bson::Document,
    timestamp: DateTime<Utc>,
}

impl From<EventDocument> for EventRecord {
    fn from(doc: EventDocument) -> Self {
        EventRecord {
            id: doc.id.map(|oid| oid.to_string()).unwrap_or_default(),
            slot: doc.slot,
            signature: doc.signature,
            event_name: doc.event_name,
            discriminator: doc.discriminator,
            data: bson::Bson::Document(doc.data).into(),
            timestamp: doc.timestamp,
        }
    }
}

/// MongoDB database backend
#[derive(Clone)]
pub struct MongoDbBackend {
    collection: Collection<EventDocument>,
}

impl MongoDbBackend {
    pub async fn new(database_url: &str) -> Result<Self> {
        tracing::info!("Connecting to MongoDB database");

        // Parse URL to extract database name
        let parsed = url::Url::parse(database_url)
            .map_err(|e| SoltraceError::Database(format!("Invalid MongoDB URL: {}", e)))?;

        let db_name = parsed
            .path_segments()
            .and_then(|mut s| s.next())
            .filter(|s| !s.is_empty())
            .unwrap_or("soltrace");

        let client = Client::with_uri_str(database_url)
            .await
            .map_err(|e| SoltraceError::Database(format!("Failed to connect to MongoDB: {}", e)))?;

        let db = client.database(db_name);
        let collection = db.collection::<EventDocument>("events");

        let backend = Self { collection };
        backend.run_migrations().await?;

        Ok(backend)
    }

    async fn create_indexes(&self) -> Result<()> {
        // Signature unique index
        let signature_index = IndexModel::builder()
            .keys(doc! { "signature": 1 })
            .options(IndexOptions::builder().unique(true).build())
            .build();

        // Slot index
        let slot_index = IndexModel::builder().keys(doc! { "slot": 1 }).build();

        // Event name index
        let event_name_index = IndexModel::builder().keys(doc! { "event_name": 1 }).build();

        // Timestamp index
        let timestamp_index = IndexModel::builder().keys(doc! { "timestamp": 1 }).build();

        self.collection
            .create_indexes(vec![
                signature_index,
                slot_index,
                event_name_index,
                timestamp_index,
            ])
            .await
            .map_err(|e| SoltraceError::Database(format!("Failed to create indexes: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl DatabaseBackend for MongoDbBackend {
    async fn run_migrations(&self) -> Result<()> {
        self.create_indexes().await?;
        tracing::info!("MongoDB migrations completed");
        Ok(())
    }

    async fn insert_event(&self, event: &DecodedEvent, raw: &RawEvent) -> Result<String> {
        let discriminator_hex = hex::encode_upper(event.discriminator);

        // Convert JSON data to BSON document
        let data_doc = bson::to_document(&event.data).map_err(|e| {
            SoltraceError::Database(format!("Failed to convert event data to BSON: {}", e))
        })?;

        let doc = EventDocument {
            id: None,
            slot: raw.slot as i64,
            signature: raw.signature.clone(),
            event_name: event.event_name.clone(),
            discriminator: discriminator_hex,
            data: data_doc,
            timestamp: raw.timestamp,
        };

        let result = self
            .collection
            .insert_one(doc)
            .await
            .map_err(|e| SoltraceError::Database(format!("Failed to insert event: {}", e)))?;

        Ok(result.inserted_id.to_string())
    }

    async fn get_events_by_slot_range(
        &self,
        start_slot: Slot,
        end_slot: Slot,
    ) -> Result<Vec<EventRecord>> {
        let filter = doc! {
            "slot": {
                "$gte": start_slot as i64,
                "$lte": end_slot as i64
            }
        };

        let mut cursor = self
            .collection
            .find(filter)
            .sort(doc! { "slot": 1 })
            .await
            .map_err(|e| SoltraceError::Database(format!("Failed to query events: {}", e)))?;

        let mut events = Vec::new();
        while cursor
            .advance()
            .await
            .map_err(|e| SoltraceError::Database(format!("Failed to advance cursor: {}", e)))?
        {
            let doc = cursor.deserialize_current().map_err(|e| {
                SoltraceError::Database(format!("Failed to deserialize event: {}", e))
            })?;
            events.push(doc.into());
        }

        Ok(events)
    }

    async fn get_events_by_name(&self, event_name: &str) -> Result<Vec<EventRecord>> {
        let filter = doc! { "event_name": event_name };

        let mut cursor = self
            .collection
            .find(filter)
            .sort(doc! { "slot": -1 })
            .await
            .map_err(|e| SoltraceError::Database(format!("Failed to query events: {}", e)))?;

        let mut events = Vec::new();
        while cursor
            .advance()
            .await
            .map_err(|e| SoltraceError::Database(format!("Failed to advance cursor: {}", e)))?
        {
            let doc = cursor.deserialize_current().map_err(|e| {
                SoltraceError::Database(format!("Failed to deserialize event: {}", e))
            })?;
            events.push(doc.into());
        }

        Ok(events)
    }

    async fn event_exists(&self, signature: &str) -> Result<bool> {
        let filter = doc! { "signature": signature };

        let count = self
            .collection
            .count_documents(filter)
            .await
            .map_err(|e| SoltraceError::Database(format!("Failed to count events: {}", e)))?;

        Ok(count > 0)
    }
}
