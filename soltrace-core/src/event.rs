use crate::{
    error::{Result, SoltraceError},
    idl::IdlParser,
    idl_event::IdlEventDecoder,
    types::{DecodedEvent, IdlEventDefinition, ProgramPrefixConfig},
};
use arc_swap::ArcSwap;
use std::sync::Arc;

#[derive(Clone)]
pub struct EventDecoder {
    idl_parser: Arc<ArcSwap<IdlParser>>,
    prefix_config: ProgramPrefixConfig,
}

impl EventDecoder {
    pub fn new(idl_parser: Arc<ArcSwap<IdlParser>>, prefix_config: ProgramPrefixConfig) -> Self {
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

        // ponytail: one load() per call yields a consistent snapshot of the parser.
        // A concurrent swap (on-chain IDL hot-reload via ArcSwap::store) cannot tear
        // a mid-decode view — the Guard pins one coherent revision until dropped.
        let parser = self.idl_parser.load();

        // Find event definition by discriminator
        let event_def = parser
            .find_event_by_discriminator(program_id, &discriminator)
            .ok_or_else(|| {
                SoltraceError::EventDecode(format!(
                    "No event found with discriminator: {:02x?}",
                    discriminator
                ))
            })?;

        let empty_types: Vec<serde_json::Value> = vec![];
        let types = parser
            .get_idls()
            .get(program_id)
            .and_then(|idl| idl.types.as_ref())
            .unwrap_or(&empty_types);

        // Decode the event data using IDL-based decoder
        let decoded =
            self.decode_event_data(program_id, signature, &event_def, event_data, types)?;

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
        types: &[serde_json::Value],
    ) -> Result<serde_json::Value> {
        let empty_fields: Vec<crate::types::IdlField> = vec![];
        let fields = event_def.fields.as_ref().unwrap_or(&empty_fields);

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
        let idl_parser = Arc::new(ArcSwap::from_pointee(IdlParser::new()));
        let prefix_config = ProgramPrefixConfig::new();
        let decoder = EventDecoder::new(idl_parser, prefix_config);

        let result = decoder.decode_event("test_program", "test_signature", &[]);
        assert!(result.is_err());
    }

    /// Concurrent readers + a swapping writer must never observe a torn parser
    /// state: every decode_event either fully resolves (Ok) or cleanly reports
    /// the event missing (Err), never a panic or corrupt decode.
    #[test]
    fn arcswap_load_never_tears_under_concurrent_swap() {
        const PROGRAM: &str = "Test111111111111111111111111111111";

        let with_event = {
            let mut p = IdlParser::new();
            p.load_from_str(
                r#"{"address":"Test111111111111111111111111111111","events":[{"name":"TestEvent"}]}"#,
            )
            .unwrap();
            p
        };
        let without_event = IdlParser::new();
        let discriminator = IdlParser::calculate_discriminator("TestEvent");
        let payload: Vec<u8> = discriminator.iter().copied().chain([0u8; 8]).collect();

        let decoder = Arc::new(EventDecoder::new(
            Arc::new(ArcSwap::from_pointee(with_event)),
            ProgramPrefixConfig::new(),
        ));

        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut handles = vec![];

        // Writer: flip between the parser with the event and an empty one.
        {
            let shared = decoder.idl_parser.clone();
            let stop = stop.clone();
            let without = Arc::new(without_event);
            handles.push(std::thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    // ponytail: store() swaps the whole Arc atomically; readers load()
                    // either the old or the new revision, never a half-written map.
                    shared.store(Arc::new(IdlParser::new()));
                    shared.store(without.clone());
                    let mut fresh = IdlParser::new();
                    fresh
                        .load_from_str(
                            r#"{"address":"Test111111111111111111111111111111","events":[{"name":"TestEvent"}]}"#,
                        )
                        .unwrap();
                    shared.store(Arc::new(fresh));
                }
            }));
        }

        // Readers: hammer decode_event; assert every result is Ok or a clean Err.
        for _ in 0..4 {
            let decoder = decoder.clone();
            let payload = payload.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..2000 {
                    match decoder.decode_event(PROGRAM, "sig", &payload) {
                        Ok(_) => {} // saw a revision with the event present
                        Err(SoltraceError::EventDecode(m)) => {
                            assert!(m.contains("No event found"), "unexpected error: {m}");
                        }
                        Err(e) => panic!("unexpected error variant: {e:?}"),
                    }
                }
            }));
        }

        std::thread::sleep(std::time::Duration::from_millis(150));
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        for h in handles {
            h.join().expect("reader/writer thread panicked");
        }
    }
}
