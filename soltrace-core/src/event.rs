use crate::{
    error::{Result, SoltraceError},
    idl::IdlParser,
    idl_event::IdlEventDecoder,
    types::{DecodedEvent, IdlEventDefinition, ProgramPrefixConfig},
};

#[derive(Clone)]
pub struct EventDecoder {
    idl_parser: IdlParser,
    prefix_config: ProgramPrefixConfig,
}

impl EventDecoder {
    pub fn new(idl_parser: IdlParser, prefix_config: ProgramPrefixConfig) -> Self {
        Self {
            idl_parser,
            prefix_config,
        }
    }

    /// Decode an Anchor event from raw data bytes
    ///
    /// Anchor event format:
    /// - 8 bytes: discriminator (sha256("event:<name>")[..8])
    /// - Remaining bytes: borsh-encoded event data
    pub fn decode_event(
        &self,
        program_id: &str,
        signature: &str,
        data: &[u8],
    ) -> Result<DecodedEvent> {
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

        // Decode the event data using IDL-based decoder
        let decoded = self.decode_event_data(program_id, signature, &event_def, event_data)?;

        // Prefix event name with program prefix
        let prefix = self.prefix_config.get_prefix(program_id);
        let prefixed_event_name = format!("{}_{}", prefix, event_def.name);

        Ok(DecodedEvent {
            event_name: prefixed_event_name,
            data: decoded,
            discriminator,
        })
    }

    /// Decode event data using IDL-based borsh deserialization
    fn decode_event_data(
        &self,
        program_id: &str,
        signature: &str,
        event_def: &IdlEventDefinition,
        data: &[u8],
    ) -> Result<serde_json::Value> {
        let empty_fields: Vec<crate::types::IdlField> = vec![];
        let fields = event_def.fields.as_ref().unwrap_or(&empty_fields);

        let empty_types: Vec<serde_json::Value> = vec![];
        let types = self
            .idl_parser
            .get_idls()
            .get(program_id)
            .and_then(|idl| idl.types.as_ref())
            .unwrap_or(&empty_types);

        // Use new IDL-based decoder
        match IdlEventDecoder::decode(data, fields, types) {
            Ok(decoded) => Ok(decoded),
            Err(e) => {
                // Log detailed warning for decode failure
                tracing::warn!(
                    "ID Decode Failed for event '{}' (program_id: {}, signature: {}): {}. Fallback to hex encoding. Data length: {} bytes, fields defined: {}",
                    event_def.name,
                    program_id,
                    signature,
                    e,
                    data.len(),
                    fields.len()
                );

                // Fallback to hex encoding if decoding fails
                let hex = hex::encode_upper(data);
                Ok(serde_json::json!({
                    "hex": hex,
                    "length": data.len(),
                    "decode_error": e.to_string(),
                    "event_name": event_def.name,
                    "field_count": fields.len(),
                    "timestamp": chrono::Utc::now().to_rfc3339()
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
        let prefix_config = ProgramPrefixConfig::new();
        let decoder = EventDecoder::new(idl_parser, prefix_config);

        let result = decoder.decode_event("test_program", "test_signature", &[]);
        assert!(result.is_err());
    }
}
