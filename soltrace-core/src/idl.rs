use crate::{
    error::{Result, SoltraceError},
    types::{EventDiscriminator, IdlEventDefinition, ParsedIdl},
};
use anchor_lang::solana_program::hash::hash;
use std::collections::HashMap;

#[derive(Clone)]
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
        let hash = hash(preimage.as_bytes());
        let mut discriminator = [0u8; 8];
        discriminator.copy_from_slice(&hash.to_bytes()[..8]);
        discriminator
    }

    /// Find event name by discriminator
    pub fn find_event_by_discriminator(
        &self,
        program_id: &str,
        discriminator: &[u8],
    ) -> Option<IdlEventDefinition> {
        let idl = self.idls.get(program_id)?;
        let event = idl
            .events
            .iter()
            .find(|e| Self::calculate_discriminator(&e.name).as_slice() == discriminator)?;

        // If event has fields, return it directly
        if event.fields.is_some() {
            return Some(event.clone());
        }

        // Otherwise, look for event definition in the types array
        if let Some(types) = &idl.types {
            for type_def in types {
                if let Some(type_name) = type_def.get("name") {
                    if let Some(name_str) = type_name.as_str() {
                        if name_str == event.name {
                            // Found the type definition, extract fields
                            if let Some(type_obj) = type_def.get("type") {
                                if let Some(kind) = type_obj.get("kind") {
                                    if let Some(kind_str) = kind.as_str() {
                                        if kind_str == "struct" {
                                            if let Some(fields) = type_obj.get("fields") {
                                                match serde_json::from_value::<
                                                    Vec<crate::types::IdlField>,
                                                >(
                                                    fields.clone()
                                                ) {
                                                    Ok(fields_vec) => {
                                                        return Some(IdlEventDefinition {
                                                            name: event.name.clone(),
                                                            fields: Some(fields_vec),
                                                            r#type: Some(type_obj.clone()),
                                                        });
                                                    }
                                                    Err(e) => {
                                                        eprintln!(
                                                            "Failed to parse fields for {}: {}",
                                                            event.name, e
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fallback: return the event as-is (no fields)
        Some(event.clone())
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

    #[test]
    fn test_event_fields_from_types() {
        let idl_json = r#"{
            "address": "Test111111111111111111111111111111",
            "events": [
                {
                    "name": "TestEvent",
                    "discriminator": [1, 2, 3, 4, 5, 6, 7, 8]
                }
            ],
            "types": [
                {
                    "name": "TestEvent",
                    "type": {
                        "kind": "struct",
                        "fields": [
                            {"name": "field1", "type": "u64"},
                            {"name": "field2", "type": "pubkey"}
                        ]
                    }
                }
            ]
        }"#;

        let mut parser = IdlParser::new();
        parser.load_from_str(idl_json).unwrap();

        let discriminator = IdlParser::calculate_discriminator("TestEvent");
        let event_def = parser
            .find_event_by_discriminator("Test111111111111111111111111111111", &discriminator)
            .expect("Should find event");

        assert_eq!(event_def.name, "TestEvent");
        assert!(event_def.fields.is_some());
        let fields = event_def.fields.unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "field1");
        assert_eq!(fields[0].field_type, "u64");
        assert_eq!(fields[1].name, "field2");
        assert_eq!(fields[1].field_type, "pubkey");
    }

    #[test]
    fn test_payment_record_fields_from_idl() {
        let idl_json = r#"{
            "address": "TRibg8W8zmPHQqWtyAD1rEBRXEdyU13Mu6qX1Sg42tJ",
            "events": [
                {
                    "name": "PaymentRecord",
                    "discriminator": [42, 100, 253, 124, 170, 186, 231, 186]
                }
            ],
            "types": [
                {
                    "name": "PaymentRecord",
                    "type": {
                        "kind": "struct",
                        "fields": [
                            {"name": "payment_policy", "type": "pubkey"},
                            {"name": "gateway", "type": "pubkey"},
                            {"name": "amount", "type": "u64"},
                            {"name": "timestamp", "type": "i64"},
                            {"name": "memo", "type": {"array": ["u8", 64]}},
                            {"name": "record_id", "type": "u32"}
                        ]
                    }
                }
            ]
        }"#;

        let mut parser = IdlParser::new();
        parser.load_from_str(idl_json).unwrap();

        let discriminator = IdlParser::calculate_discriminator("PaymentRecord");
        assert_eq!(discriminator, [42, 100, 253, 124, 170, 186, 231, 186]);

        let event_def = parser
            .find_event_by_discriminator(
                "TRibg8W8zmPHQqWtyAD1rEBRXEdyU13Mu6qX1Sg42tJ",
                &discriminator,
            )
            .expect("Should find PaymentRecord event");

        assert_eq!(event_def.name, "PaymentRecord");
        assert!(event_def.fields.is_some());
        let fields = event_def.fields.unwrap();
        assert_eq!(fields.len(), 6);
        assert_eq!(fields[0].name, "payment_policy");
        assert_eq!(fields[1].name, "gateway");
        assert_eq!(fields[2].name, "amount");
        assert_eq!(fields[3].name, "timestamp");
        assert_eq!(fields[4].name, "memo");
        assert_eq!(fields[5].name, "record_id");
    }
}
