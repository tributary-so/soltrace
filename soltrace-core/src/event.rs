use crate::{
    borsh_decoder::BorshDecoder,
    error::{Result, SoltraceError},
    idl::IdlParser,
    types::{DecodedEvent, IdlEventDefinition},
};

#[derive(Clone)]
pub struct EventDecoder {
    idl_parser: IdlParser,
}

impl EventDecoder {
    pub fn new(idl_parser: IdlParser) -> Self {
        Self { idl_parser }
    }

    /// Decode an Anchor event from raw data bytes
    ///
    /// Anchor event format:
    /// - 8 bytes: discriminator (sha256("event:<name>")[..8])
    /// - Remaining bytes: borsh-encoded event data
    pub fn decode_event(&self, program_id: &str, data: &[u8]) -> Result<DecodedEvent> {
        if data.len() < 8 {
            return Err(SoltraceError::EventDecode(
                "Event data too short (< 8 bytes)".to_string(),
            ));
        }

        let discriminator: [u8; 8] = data[..8].try_into().unwrap();
        let event_data = &data[8..];

        // Find event definition by discriminator
        let event_def = self
            .idl_parser
            .find_event_by_discriminator(program_id, &discriminator)
            .ok_or_else(|| {
                SoltraceError::EventDecode(format!(
                    "No event found with discriminator: {:02x?}",
                    discriminator
                ))
            })?;

        // Decode the event data
        let decoded = self.decode_event_data(event_def, event_data)?;

        Ok(DecodedEvent {
            event_name: event_def.name.clone(),
            data: decoded,
            discriminator,
        })
    }

    /// Decode event data using borsh deserialization
    fn decode_event_data(
        &self,
        event_def: &IdlEventDefinition,
        data: &[u8],
    ) -> Result<serde_json::Value> {
        // Try to decode using the IDL field definitions
        match BorshDecoder::decode_event_data(data, &event_def.fields) {
            Ok(decoded) => Ok(decoded),
            Err(_) => {
                // Fallback to hex encoding if decoding fails
                let hex = hex::encode_upper(data);
                Ok(serde_json::json!({
                    "hex": hex,
                    "length": data.len(),
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_empty_data() {
        let idl_parser = IdlParser::new();
        let decoder = EventDecoder::new(idl_parser);

        let result = decoder.decode_event("test_program", &[]);
        assert!(result.is_err());
    }
}
