use solana_sdk::pubkey::Pubkey;
use serde::{Deserialize, Serialize};

pub type Slot = u64;
pub type ProgramId = Pubkey;
pub type EventDiscriminator = [u8; 8];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedIdl {
    pub version: String,
    pub name: Option<String>,
    pub events: Vec<IdlEventDefinition>,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdlEventDefinition {
    pub name: String,
    pub fields: Vec<IdlField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdlField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
}

/// Represents a decoded Anchor event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedEvent {
    pub event_name: String,
    pub data: serde_json::Value,
    pub discriminator: EventDiscriminator,
}

/// Raw event data from Solana logs
#[derive(Debug, Clone)]
pub struct RawEvent {
    pub slot: Slot,
    pub signature: String,
    pub program_id: ProgramId,
    pub log: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
