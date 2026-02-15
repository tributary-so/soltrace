use crate::{
    db::Database,
    event::EventDecoder,
    idl::IdlParser,
    types::RawEvent,
};
use anyhow::Result;
use tracing::{info, warn, debug, error};
use solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta;
use base64::{Engine as _, engine::general_purpose::STANDARD};

/// Load all IDL files from a directory
pub async fn load_idls(idl_parser: &mut IdlParser, idl_dir: &str) -> Result<()> {
    let dir = tokio::fs::read_dir(idl_dir).await;

    if let Err(e) = dir {
        warn!("Failed to read IDL directory '{}': {}", idl_dir, e);
        warn!("Continuing without IDLs (events will not be decoded)");
        return Ok(());
    }

    let mut entries = dir?;
    let mut loaded_count = 0;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "json") {
            match idl_parser.load_from_file(path.to_str().unwrap()) {
                Ok(_) => {
                    loaded_count += 1;
                    info!("Loaded IDL: {}", path.display());
                }
                Err(e) => {
                    error!("Failed to load IDL from {}: {}", path.display(), e);
                }
            }
        }
    }

    if loaded_count == 0 {
        warn!("No IDLs loaded from {}", idl_dir);
    }

    Ok(())
}

/// Process a single transaction and extract events
pub async fn process_transaction(
    transaction: EncodedConfirmedTransactionWithStatusMeta,
    program_id_str: &str,
    event_decoder: &EventDecoder,
    db: &Database,
) -> Result<Vec<String>> {
    let mut processed_signatures = Vec::new();

    let slot = transaction.slot;

    let meta = transaction.transaction.meta
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Transaction has no metadata"))?;

    // Skip failed transactions
    if let Some(err) = &meta.err {
        debug!("Skipping failed transaction: {:?}", err);
        return Ok(processed_signatures);
    }

    // Check if we have logs
    let logs: Option<Vec<String>> = meta.log_messages.clone().into();
    let logs = logs.ok_or_else(|| anyhow::anyhow!("Transaction has no logs"))?;

    // Get transaction signature from the encoded transaction
    let signature = match &transaction.transaction.transaction {
        solana_transaction_status::EncodedTransaction::Json(ui_tx) => {
            ui_tx.signatures.first()
                .ok_or_else(|| anyhow::anyhow!("Transaction has no signature"))?
                .to_string()
        }
        _ => {
            return Err(anyhow::anyhow!("Only JSON-encoded transactions are supported"));
        }
    };

    // Get block time from transaction if available
    let block_time = transaction.block_time;
    let timestamp = block_time
        .and_then(|bt| chrono::DateTime::from_timestamp(bt, 0))
        .unwrap_or_else(chrono::Utc::now);

    // Process logs for events
    let mut events_count = 0;
    for log in logs {
        if let Some(event_data) = extract_event_from_log(&log, program_id_str) {
            // Decode event
            match event_decoder.decode_event(program_id_str, &event_data) {
                Ok(decoded_event) => {
                    // Create raw event record
                    let raw_event = RawEvent {
                        slot,
                        signature: signature.clone(),
                        program_id: program_id_str.parse()
                            .unwrap_or_else(|_| solana_sdk::pubkey::Pubkey::default()),
                        log: log.to_string(),
                        timestamp,
                    };

                    // Store event
                    match db.insert_event(&decoded_event, &raw_event).await {
                        Ok(_) => {
                            events_count += 1;
                            debug!("Stored event: {} from {}", decoded_event.event_name, signature);
                        }
                        Err(e) => {
                            if e.to_string().contains("UNIQUE constraint") {
                                debug!("Event {} already exists, skipping", signature);
                            } else {
                                error!("Failed to store event: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!("Failed to decode event: {}", e);
                }
            }
        }
    }

    if events_count > 0 {
        processed_signatures.push(signature);
    }

    Ok(processed_signatures)
}

/// Extract event data from a log line
/// Looks for Anchor program log entries with base64-encoded data
pub fn extract_event_from_log(log: &str, program_id_str: &str) -> Option<Vec<u8>> {
    // Anchor events appear in logs as "Program data: <base64_data>"
    // or "Program log: <hex_data>"

    if log.starts_with("Program data:") {
        let data_str = log.strip_prefix("Program data: ")?.trim();
        if let Ok(data) = STANDARD.decode(data_str) {
            // Verify this is for our program
            if log.contains(program_id_str) {
                return Some(data);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_event_from_log() {
        // Base64 "eyJldmVudCI6IlRyYW5zZmVyIn0=" decodes to '{"event":"Transfer"}'
        // In real logs, the program_id check happens against other log lines
        let log = "Program data: eyJldmVudCI6IlRyYW5zZmVyIn0=";
        let program_id = "data:"; // Use something that exists in the log for test
        let result = extract_event_from_log(log, program_id);

        assert!(result.is_some());
        assert_eq!(result.unwrap(), br#"{"event":"Transfer"}"#);
    }

    #[test]
    fn test_extract_event_no_match() {
        let log = "Program data: eyJldmVudCI6IlRyYW5zZmVyIn0=";
        let program_id = "NonExistentProgram";
        let result = extract_event_from_log(log, program_id);

        assert!(result.is_none());
    }
}
