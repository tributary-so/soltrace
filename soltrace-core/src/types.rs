use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;

pub type Slot = u64;
pub type ProgramId = Pubkey;
pub type EventDiscriminator = [u8; 8];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedIdl {
    #[serde(default)]
    pub version: Option<String>,
    pub name: Option<String>,
    pub events: Vec<IdlEventDefinition>,
    pub address: String,

    #[serde(default)]
    pub metadata: Option<IdlMetadata>,

    #[serde(default)]
    pub instructions: Option<serde_json::Value>,

    #[serde(default)]
    pub accounts: Option<serde_json::Value>,

    #[serde(default)]
    pub errors: Option<serde_json::Value>,

    #[serde(default)]
    pub types: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdlMetadata {
    #[serde(default)]
    pub version: Option<String>,

    #[serde(default)]
    pub name: Option<String>,

    #[serde(default)]
    pub spec: Option<String>,

    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdlEventDefinition {
    pub name: String,
    #[serde(default)]
    pub fields: Option<Vec<IdlField>>,
    #[serde(default)]
    pub r#type: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdlField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: serde_json::Value,
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

/// Configuration for program-to-prefix mapping
#[derive(Debug, Clone)]
pub struct ProgramPrefixConfig {
    pub default_prefix: String,
    pub program_mappings: HashMap<String, String>,
}

impl ProgramPrefixConfig {
    pub fn new() -> Self {
        Self {
            default_prefix: "default".to_string(),
            program_mappings: HashMap::new(),
        }
    }

    pub fn add_mapping(&mut self, program_id: &str, prefix: &str) {
        self.program_mappings
            .insert(program_id.to_string(), prefix.to_string());
    }

    pub fn get_prefix(&self, program_id: &str) -> String {
        self.program_mappings
            .get(program_id)
            .cloned()
            .unwrap_or_else(|| self.default_prefix.clone())
    }
}

impl Default for ProgramPrefixConfig {
    fn default() -> Self {
        Self::new()
    }
}
