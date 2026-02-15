use crate::{
    error::{Result, SoltraceError},
    types::{EventDiscriminator, IdlEventDefinition, ParsedIdl},
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub struct IdlParser {
    idls: HashMap<String, ParsedIdl>, // program_id -> ParsedIdl
}

impl IdlParser {
    pub fn new() -> Self {
        Self {
            idls: HashMap::new(),
        }
    }

    /// Load an IDL from a JSON file
    pub fn load_from_file(&mut self, path: &str) -> Result<()> {
        let content = std::fs::read_to_string(path)?;
        let idl: ParsedIdl = serde_json::from_str(&content)
            .map_err(|e| SoltraceError::IdlParse(format!("Failed to parse IDL JSON: {}", e)))?;

        self.idls.insert(idl.address.clone(), idl);
        Ok(())
    }

    /// Load an IDL from a JSON string
    pub fn load_from_str(&mut self, json: &str) -> Result<()> {
        let idl: ParsedIdl = serde_json::from_str(json)
            .map_err(|e| SoltraceError::IdlParse(format!("Failed to parse IDL JSON: {}", e)))?;

        self.idls.insert(idl.address.clone(), idl);
        Ok(())
    }

    /// Get all loaded IDLs
    pub fn get_idls(&self) -> &HashMap<String, ParsedIdl> {
        &self.idls
    }

    /// Get event definitions for a program
    pub fn get_events(&self, program_id: &str) -> Option<&Vec<IdlEventDefinition>> {
        self.idls.get(program_id).map(|idl| &idl.events)
    }

    /// Calculate event discriminator for an Anchor event
    /// Anchor uses: sha256("event:<event_name>")[..8]
    pub fn calculate_discriminator(event_name: &str) -> EventDiscriminator {
        let preimage = format!("event:{}", event_name);
        let hash = Sha256::digest(preimage.as_bytes());
        let mut discriminator = [0u8; 8];
        discriminator.copy_from_slice(&hash[..8]);
        discriminator
    }

    /// Find event name by discriminator
    pub fn find_event_by_discriminator(
        &self,
        program_id: &str,
        discriminator: &[u8],
    ) -> Option<&IdlEventDefinition> {
        let events = self.get_events(program_id)?;
        events
            .iter()
            .find(|event| Self::calculate_discriminator(&event.name).as_slice() == discriminator)
    }
}

impl Default for IdlParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discriminator_calculation() {
        let discriminator = IdlParser::calculate_discriminator("Transfer");
        assert_eq!(discriminator.len(), 8);
    }
}
