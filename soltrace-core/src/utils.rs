use crate::{
    db::Database, error::Result as CoreResult, event::EventDecoder, idl::IdlParser,
    onchain_idl::fetch_canonical_idl, types::CpiEvent, types::DecodedEvent, types::InnerInstructionInfo,
    types::ParsedIdl, types::RawEvent,
};
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use solana_sdk::pubkey::Pubkey;
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, UiInstruction, UiMessage,
};
use tracing::{debug, error, info, warn};

/// `emit_cpi!` wrapper discriminator — the first 8 bytes of every Anchor
/// `emit_cpi!` instruction (u64 `0x1d9acb512ea545e4` in little-endian, per
/// `anchor_lang::event::EVENT_IX_TAG`).
pub const EVENT_CPI_DISCRIMINATOR: [u8; 8] = 0x1d9acb512ea545e4u64.to_le_bytes();

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
        if path.extension().is_some_and(|ext| ext == "json") {
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

/// Fetch on-chain IDLs for `programs` via the program-metadata canonical PDA.
///
/// One-shot startup call: warn-and-continue per program (mirrors `load_idls`).
/// Sync because `fetch_canonical_idl` is a blocking RPC — no `.await` to hide.
pub fn load_onchain_idls(
    parser: &mut IdlParser,
    rpc: &solana_rpc_client::rpc_client::RpcClient,
    programs: &[Pubkey],
) {
    load_onchain_idls_with(parser, programs, |p| fetch_canonical_idl(rpc, p));
}

/// Inner loop factored out so tests can inject a fake fetcher without an RPC.
fn load_onchain_idls_with<F>(parser: &mut IdlParser, programs: &[Pubkey], mut fetch: F)
where
    F: FnMut(&Pubkey) -> CoreResult<Option<ParsedIdl>>,
{
    for program in programs {
        // ponytail: file-IDL precedence (Decision 1) — file IDLs are loaded
        // before this call, so any program already in the parser wins and the
        // on-chain fetch is skipped entirely (no wasted RPC).
        if parser.get_idls().contains_key(&program.to_string()) {
            debug!(program = %program, "IDL already loaded (file), skipping on-chain fetch");
            continue;
        }
        match fetch(program) {
            Ok(Some(idl)) => {
                info!(program = %program, "fetched on-chain IDL");
                parser.insert_or_replace(idl);
            }
            Ok(None) => warn!(program = %program, "no canonical Direct on-chain IDL"),
            Err(e) => warn!(program = %program, error = %e, "failed to fetch on-chain IDL"),
        }
    }
}

/// Drop programs that have no loaded IDL from the indexing set.
///
/// Without an IDL every event from a program hits the unknown-discriminator
/// debug-skip, so fetching its signatures/transactions is wasted RPC budget.
/// Returns the dropped program IDs so the caller can warn (operator likely
/// typo'd the id or forgot to install its IDL).
///
/// Live's on-chain-IDL programs are pending their IDL via `accountSubscribe`
/// and are re-added to the indexing set by the caller *after* this filter, so
/// their temporary IDL-less state here is expected, not a drop condition.
pub fn retain_indexable(
    program_ids: &mut Vec<String>,
    loaded: &std::collections::HashMap<String, ParsedIdl>,
) -> Vec<String> {
    let original = std::mem::take(program_ids);
    let (keep, dropped) = original
        .into_iter()
        .partition(|p| loaded.contains_key(p));
    *program_ids = keep;
    dropped
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

    let meta = transaction
        .transaction
        .meta
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Transaction has no metadata"))?;

    // Skip failed transactions
    if let Some(err) = &meta.err {
        debug!("Skipping failed transaction: {:?}", err);
        return Ok(processed_signatures);
    }

    // Logs are optional for CPI event extraction — `emit_cpi!` data lives in
    // inner instructions, not `Program data:` log lines. Default to empty so a
    // log-less transaction still reaches the CPI decode loop below.
    let logs: Option<Vec<String>> = meta.log_messages.clone().into();
    let logs = logs.unwrap_or_default();

    // Get transaction signature from the encoded transaction
    let signature = match &transaction.transaction.transaction {
        solana_transaction_status::EncodedTransaction::Json(ui_tx) => ui_tx
            .signatures
            .first()
            .ok_or_else(|| anyhow::anyhow!("Transaction has no signature"))?
            .to_string(),
        _ => {
            return Err(anyhow::anyhow!(
                "Only JSON-encoded transactions are supported"
            ));
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
        if let Some(event_data) = extract_event_from_log(&log) {
            // Decode event
            match event_decoder.decode_event(program_id_str, &signature, &event_data) {
                Ok(decoded_event) => {
                    // Create raw event record
                    let raw_event = RawEvent {
                        slot,
                        signature: signature.clone(),
                        program_id: program_id_str
                            .parse()
                            .unwrap_or_else(|_| solana_sdk::pubkey::Pubkey::default()),
                        log: log.to_string(),
                        timestamp,
                    };

                    // Store event
                    if store_event(db, &decoded_event, &raw_event, events_count).await {
                        events_count += 1;
                    }
                }
                Err(e) => {
                    debug!("Failed to decode event: {}", e);
                }
            }
        }
    }

    // emit_cpi! events live in inner instructions, NOT program logs — route the
    // unwrapped bytes through the same IDL decoder (hex fallback preserved).
    // Dedup by the on-chain (outer, inner) instruction position so reprocessing
    // is stable regardless of which events decode successfully.
    for (cpi, decoded_event) in decode_cpi_events(&transaction, &signature, event_decoder) {
        let raw_event = RawEvent {
            slot,
            signature: signature.clone(),
            program_id: cpi.program_id,
            log: String::new(),
            timestamp,
        };
        if store_event(
            db,
            &decoded_event,
            &raw_event,
            cpi_dedup_index(cpi.outer_index, cpi.inner_index),
        )
        .await
        {
            events_count += 1;
        }
    }

    if events_count > 0 {
        processed_signatures.push(signature);
    }

    Ok(processed_signatures)
}

/// Store one decoded event, returning `true` when a new row was inserted.
/// Dedup hits (UNIQUE constraint / duplicate) are logged at debug and count as
/// non-fatal — reprocessing a signature must not error or double-insert.
async fn store_event(
    db: &Database,
    decoded_event: &DecodedEvent,
    raw_event: &RawEvent,
    index: usize,
) -> bool {
    match db.insert_event(decoded_event, raw_event, index).await {
        Ok(_) => {
            debug!(
                "Stored event: {} from {}",
                decoded_event.event_name, raw_event.signature
            );
            true
        }
        Err(e) => {
            if e.to_string().contains("UNIQUE constraint") {
                debug!("Event {} already exists, skipping", raw_event.signature);
            } else {
                error!("Failed to store event: {}", e);
            }
            false
        }
    }
}

/// Decode every `emit_cpi!` event in a transaction's inner instructions,
/// routing the unwrapped `<event-disc(8)><borsh>` bytes through the existing
/// IDL decoder. Borsh decode failure falls back to hex (see `event.rs`) —
/// unknown-discriminator events are skipped with a debug log, never crashing.
///
/// Returns the originating `CpiEvent` (carrying its on-chain
/// `outer_index`/`inner_index`) alongside the decoded event, so callers can
/// build a stable dedup key from the instruction position rather than a
/// processing-order counter.
///
/// Pure (no database); `process_transaction` stores each result.
pub fn decode_cpi_events(
    tx: &EncodedConfirmedTransactionWithStatusMeta,
    signature: &str,
    event_decoder: &EventDecoder,
) -> Vec<(CpiEvent, DecodedEvent)> {
    let inner_ixs = extract_inner_instructions(tx);
    extract_cpi_events(&inner_ixs)
        .into_iter()
        .filter_map(|cpi| {
            match event_decoder.decode_event(&cpi.program_id.to_string(), signature, &cpi.data) {
                Ok(ev) => Some((cpi, ev)),
                Err(e) => {
                    debug!(
                        "Failed to decode CPI event (program {}, sig {}): {}",
                        cpi.program_id, signature, e
                    );
                    None
                }
            }
        })
        .collect()
}

/// Stable dedup index for a CPI event, derived from its on-chain position
/// (outer instruction index, inner-instruction index). Offset above the log
/// event counter range so a signature emitting both `emit!` (log) and
/// `emit_cpi!` events can never collide on the same `(signature, index, name)`
/// key.
///
/// Ceiling: `outer < 10_000`, `inner < 100_000` — far beyond any real tx's
/// inner-instruction count (Solana transactions are ~1232 bytes).
pub fn cpi_dedup_index(outer_index: u8, inner_index: usize) -> usize {
    const CPI_INDEX_BASE: usize = 1_000_000_000;
    const INNER_PER_OUTER: usize = 100_000;
    CPI_INDEX_BASE + (outer_index as usize) * INNER_PER_OUTER + inner_index
}

/// Extract event data from a log line
/// Looks for Anchor program log entries with base64-encoded data
pub fn extract_event_from_log(log: &str) -> Option<Vec<u8>> {
    // Anchor events appear in logs as "Program data: <base64_data>"
    // or "Program log: <hex_data>"

    if log.starts_with("Program data:") {
        let data_str = log.strip_prefix("Program data: ")?.trim();
        if let Ok(data) = STANDARD.decode(data_str) {
            // Verify this is for our program
            return Some(data);
        }
    }

    None
}

/// Extract inner (CPI) instructions from a transaction's metadata, resolving
/// account keys against the versioned-tx-aware account key table.
///
/// `getTransaction` with `maxSupportedTransactionVersion: 0` expands address
/// lookup table keys into `accountKeys`, so indices resolve directly. Inner
/// instructions whose account index falls outside the key table (e.g. ALT
/// accounts no longer on-chain) are skipped rather than crashing the indexer.
///
/// `emit_cpi!` event data lives in inner instructions — NOT program logs — so
/// this is the only way to surface Anchor `emit_cpi!` events.
pub fn extract_inner_instructions(
    tx: &EncodedConfirmedTransactionWithStatusMeta,
) -> Vec<InnerInstructionInfo> {
    let meta = match &tx.transaction.meta {
        Some(m) => m,
        None => return Vec::new(),
    };

    let inner_groups: Vec<_> = match meta.inner_instructions.clone().into() {
        Some(groups) => groups,
        None => return Vec::new(),
    };

    // Account keys: RPC expands ALT keys into account_keys when
    // maxSupportedTransactionVersion is set, so indices resolve directly.
    let account_keys: Vec<String> = match &tx.transaction.transaction {
        EncodedTransaction::Json(ui_tx) => match &ui_tx.message {
            UiMessage::Raw(raw) => raw.account_keys.clone(),
            // jsonParsed encoding is not used by soltrace.
            UiMessage::Parsed(_) => return Vec::new(),
        },
        _ => return Vec::new(),
    };

    let resolve = |idx: u8| -> Option<Pubkey> {
        account_keys
            .get(idx as usize)
            .and_then(|s| s.parse::<Pubkey>().ok())
    };

    let mut result = Vec::new();
    for group in &inner_groups {
        for (inner_index, ix) in group.instructions.iter().enumerate() {
            let compiled = match ix {
                UiInstruction::Compiled(c) => c,
                UiInstruction::Parsed(_) => continue,
            };

            let program_id = match resolve(compiled.program_id_index) {
                Some(pk) => pk,
                None => continue,
            };

            let accounts: Vec<Pubkey> = compiled
                .accounts
                .iter()
                .filter_map(|&i| resolve(i))
                .collect();

            let data = match solana_sdk::bs58::decode(&compiled.data).into_vec() {
                Ok(d) => d,
                Err(_) => continue,
            };

            result.push(InnerInstructionInfo {
                outer_index: group.index,
                inner_index,
                program_id,
                accounts,
                data,
            });
        }
    }

    result
}

/// Detect `emit_cpi!` self-CPIs in inner instructions and strip the 8-byte
/// `event_cpi` wrapper discriminator.
///
/// An Anchor `emit_cpi!` invokes the program's own event-authority PDA
/// (seeds `["__event_authority"]`) as `accounts[0]`, with instruction data
/// `[event_cpi disc(8)][event disc(8)][borsh]`. This filters those instructions
/// and returns the payload with the wrapper stripped, ready for
/// `EventDecoder::decode_event`.
///
/// Protocol-agnostic — works for any Anchor program using `emit_cpi!`.
pub fn extract_cpi_events(inner_ixs: &[InnerInstructionInfo]) -> Vec<CpiEvent> {
    inner_ixs
        .iter()
        .filter_map(|ix| {
            if ix.data.len() < EVENT_CPI_DISCRIMINATOR.len() {
                return None;
            }
            if ix.data[..EVENT_CPI_DISCRIMINATOR.len()] != EVENT_CPI_DISCRIMINATOR {
                return None;
            }
            let (event_authority, _) =
                Pubkey::find_program_address(&[b"__event_authority"], &ix.program_id);
            if ix.accounts.first() != Some(&event_authority) {
                return None;
            }
            Some(CpiEvent {
                outer_index: ix.outer_index,
                inner_index: ix.inner_index,
                program_id: ix.program_id,
                data: ix.data[EVENT_CPI_DISCRIMINATOR.len()..].to_vec(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_event_from_log() {
        // Base64 "eyJldmVudCI6IlRyYW5zZmVyIn0=" decodes to '{"event":"Transfer"}'
        // In real logs, the program_id check happens against other log lines
        let log = "Program data: eyJldmVudCI6IlRyYW5zZmVyIn0=";
        let result = extract_event_from_log(log);

        assert!(result.is_some());
        assert_eq!(result.unwrap(), br#"{"event":"Transfer"}"#);
    }

    #[test]
    fn test_extract_event_no_match() {
        let log = "Program log: Some other log";
        let result = extract_event_from_log(log);

        assert!(result.is_none());
    }

    // --- inner-instruction extraction tests ---

    use solana_sdk::bs58;

    /// Build a transaction with the given account keys and inner instructions
    /// (as raw JSON, matching the `getTransaction` wire format).
    fn build_tx(
        account_keys: serde_json::Value,
        inner_ix: serde_json::Value,
    ) -> EncodedConfirmedTransactionWithStatusMeta {
        let json = serde_json::json!({
            "slot": 42,
            "transaction": {
                "signatures": ["sig"],
                "message": {
                    "header": {
                        "numRequiredSignatures": 1,
                        "numReadonlySignedAccounts": 0,
                        "numReadonlyUnsignedAccounts": 1
                    },
                    "accountKeys": account_keys,
                    "recentBlockhash": "hash",
                    "instructions": []
                }
            },
            "meta": {
                "err": null,
                "status": { "Ok": null },
                "fee": 0,
                "preBalances": [],
                "postBalances": [],
                "innerInstructions": inner_ix
            },
            "blockTime": 0
        });
        serde_json::from_value(json).expect("failed to deserialize test transaction")
    }

    #[test]
    fn test_extract_inner_instructions_resolves_accounts_and_data() {
        let raw_data = vec![0xde, 0xad, 0xbe, 0xef];
        let data_b58 = bs58::encode(&raw_data).into_string();

        let tx = build_tx(
            serde_json::json!([
                "11111111111111111111111111111111",
                "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
            ]),
            serde_json::json!([{
                "index": 0,
                "instructions": [{
                    "programIdIndex": 1,
                    "accounts": [0],
                    "data": data_b58,
                    "stackHeight": 2
                }]
            }]),
        );

        let result = extract_inner_instructions(&tx);
        assert_eq!(result.len(), 1);
        let ix = &result[0];
        assert_eq!(ix.outer_index, 0);
        assert_eq!(ix.inner_index, 0);
        assert_eq!(
            ix.program_id,
            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
                .parse()
                .unwrap()
        );
        assert_eq!(ix.accounts.len(), 1);
        assert_eq!(
            ix.accounts[0],
            "11111111111111111111111111111111".parse().unwrap()
        );
        assert_eq!(ix.data, raw_data);
    }

    #[test]
    fn test_extract_inner_instructions_empty_when_none() {
        let tx = build_tx(
            serde_json::json!(["11111111111111111111111111111111"]),
            serde_json::json!([]),
        );
        assert!(extract_inner_instructions(&tx).is_empty());
    }

    #[test]
    fn test_extract_inner_instructions_skips_out_of_range_index() {
        let tx = build_tx(
            serde_json::json!(["11111111111111111111111111111111"]),
            serde_json::json!([{
                "index": 0,
                "instructions": [{
                    "programIdIndex": 99,
                    "accounts": [],
                    "data": bs58::encode(&[1, 2, 3]).into_string(),
                    "stackHeight": 2
                }]
            }]),
        );
        assert!(extract_inner_instructions(&tx).is_empty());
    }

    // --- CPI event extraction tests ---

    #[test]
    fn test_extract_cpi_events_detects_and_strips() {
        let program_id: Pubkey = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
            .parse()
            .unwrap();
        let (event_authority, _) =
            Pubkey::find_program_address(&[b"__event_authority"], &program_id);

        let event_bytes = vec![0xaa; 16];
        let mut data = EVENT_CPI_DISCRIMINATOR.to_vec();
        data.extend_from_slice(&event_bytes);

        let ix = InnerInstructionInfo {
            outer_index: 0,
            inner_index: 1,
            program_id,
            accounts: vec![event_authority],
            data,
        };

        let result = extract_cpi_events(std::slice::from_ref(&ix));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].program_id, program_id);
        assert_eq!(result[0].data, event_bytes);
        assert_eq!(result[0].outer_index, 0);
        assert_eq!(result[0].inner_index, 1);
    }

    #[test]
    fn test_extract_cpi_events_filters_wrong_authority() {
        let program_id: Pubkey = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
            .parse()
            .unwrap();
        let ix = InnerInstructionInfo {
            outer_index: 0,
            inner_index: 0,
            program_id,
            accounts: vec![Pubkey::default()],
            data: EVENT_CPI_DISCRIMINATOR.to_vec(),
        };
        assert!(extract_cpi_events(std::slice::from_ref(&ix)).is_empty());
    }

    #[test]
    fn test_extract_cpi_events_filters_wrong_discriminator() {
        let program_id: Pubkey = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
            .parse()
            .unwrap();
        let (event_authority, _) =
            Pubkey::find_program_address(&[b"__event_authority"], &program_id);
        let ix = InnerInstructionInfo {
            outer_index: 0,
            inner_index: 0,
            program_id,
            accounts: vec![event_authority],
            data: vec![0xff; 16],
        };
        assert!(extract_cpi_events(std::slice::from_ref(&ix)).is_empty());
    }

    // --- decode_cpi_events: route unwrapped bytes through IDL decoder ---

    use crate::{idl::IdlParser, types::ProgramPrefixConfig};

    // --- retain_indexable: skip programs with no IDL ---

    #[test]
    fn test_retain_indexable_drops_idl_less_programs() {
        let with_idl = Pubkey::new_unique();
        let parser = swap_idl(&with_idl);
        let loaded = parser.get_idls();

        let orphan = Pubkey::new_unique().to_string();
        let mut program_ids = vec![with_idl.to_string(), orphan.clone()];

        let dropped = retain_indexable(&mut program_ids, loaded);

        assert_eq!(program_ids, vec![with_idl.to_string()], "IDL-backed program kept");
        assert_eq!(dropped, vec![orphan], "IDL-less program dropped for caller to warn");
    }

    /// Minimal IDL for `program_id` with one event `Swap { amount: u64 }`.
    fn swap_idl(program_id: &Pubkey) -> IdlParser {
        let json = format!(
            r#"{{
                "address": "{}",
                "events": [{{ "name": "Swap" }}],
                "types": [{{
                    "name": "Swap",
                    "type": {{
                        "kind": "struct",
                        "fields": [{{ "name": "amount", "type": "u64" }}]
                    }}
                }}]
            }}"#,
            program_id
        );
        let mut parser = IdlParser::new();
        parser.load_from_str(&json).expect("IDL should parse");
        parser
    }

    /// Build a transaction whose single inner instruction is a valid
    /// `emit_cpi!` self-CPI to `program_id`'s event-authority PDA, carrying
    /// `event_payload` (`<event-disc(8)><borsh>`) as the inner event bytes.
    fn cpi_event_tx(
        program_id: &Pubkey,
        event_payload: &[u8],
    ) -> EncodedConfirmedTransactionWithStatusMeta {
        let (event_authority, _) =
            Pubkey::find_program_address(&[b"__event_authority"], program_id);
        let mut ix_data = EVENT_CPI_DISCRIMINATOR.to_vec();
        ix_data.extend_from_slice(event_payload);
        let data_b58 = bs58::encode(&ix_data).into_string();
        build_tx(
            serde_json::json!([event_authority.to_string(), program_id.to_string()]),
            serde_json::json!([{
                "index": 0,
                "instructions": [{
                    "programIdIndex": 1,
                    "accounts": [0],
                    "data": data_b58,
                    "stackHeight": 2
                }]
            }]),
        )
    }

    #[test]
    fn test_decode_cpi_events_routes_through_idl_decoder() {
        let program_id: Pubkey = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
            .parse()
            .unwrap();
        let decoder = EventDecoder::new(std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(swap_idl(&program_id))), ProgramPrefixConfig::new());

        // <disc(8)><borsh u64 = 42>
        let disc = IdlParser::calculate_discriminator("Swap");
        let mut payload = disc.to_vec();
        payload.extend_from_slice(&42u64.to_le_bytes());

        let tx = cpi_event_tx(&program_id, &payload);
        let decoded = decode_cpi_events(&tx, "sig123", &decoder);

        assert_eq!(decoded.len(), 1);
        let (cpi, event) = &decoded[0];
        assert_eq!(cpi.program_id, program_id);
        assert_eq!(event.event_name, "default_Swap");
        // u64 decodes to a JSON string (see IdlEventDecoder::decode_simple_type)
        assert_eq!(event.data["amount"], "42");
    }

    #[test]
    fn test_decode_cpi_events_keeps_hex_fallback_on_bad_borsh() {
        let program_id: Pubkey = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
            .parse()
            .unwrap();
        let decoder = EventDecoder::new(std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(swap_idl(&program_id))), ProgramPrefixConfig::new());

        // Swap discriminator matched, but borsh payload truncated (u64 needs 8,
        // give 2) -> IdlEventDecoder errors -> hex fallback must fire, not crash.
        let disc = IdlParser::calculate_discriminator("Swap");
        let mut payload = disc.to_vec();
        payload.extend_from_slice(&[9u8, 9u8]);

        let tx = cpi_event_tx(&program_id, &payload);
        let decoded = decode_cpi_events(&tx, "sig", &decoder);

        assert_eq!(decoded.len(), 1);
        let (_cpi, event) = &decoded[0];
        assert!(
            event.data.get("hex").is_some(),
            "hex fallback should fire on borsh failure; got: {}",
            event.data
        );
    }

    // --- jfsr: basic CPI-decoding coverage (M1 HANDOFF §6) ---

    use crate::db::generate_event_id;

    /// §6 row 2: an `emit_cpi!` event is carried in an inner instruction and
    /// NEVER in a `Program data:` log line — so the log-scraping path surfaces
    /// zero events for it, while the inner-instruction path surfaces it. This
    /// is the core reason the new path exists.
    #[test]
    fn test_cpi_event_not_surfaceed_by_log_path() {
        let program_id: Pubkey = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
            .parse()
            .unwrap();
        let decoder = EventDecoder::new(std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(swap_idl(&program_id))), ProgramPrefixConfig::new());

        let disc = IdlParser::calculate_discriminator("Swap");
        let mut payload = disc.to_vec();
        payload.extend_from_slice(&7u64.to_le_bytes());
        let tx = cpi_event_tx(&program_id, &payload);

        // Old path: `emit_cpi!` never writes a "Program data:" base64 line.
        assert!(extract_event_from_log("Program log: Swap emitted").is_none());
        assert!(extract_event_from_log("Program consumption: 12345").is_none());

        // New path: the inner-instruction decode surfaces the event.
        assert_eq!(
            decode_cpi_events(&tx, "sig", &decoder).len(),
            1,
            "inner-instruction path must surface the emit_cpi! event"
        );
    }

    /// §6 row 3: reprocessing a signature must not insert duplicate rows. The
    /// dedup key is `generate_event_id(signature, index, event_name)`; identical
    /// inputs hash to the same id (idempotent reprocess → ON CONFLICT DO
    /// NOTHING), and distinct indices collide-free.
    #[test]
    fn test_generate_event_id_is_deterministic_for_dedup() {
        let id_a = generate_event_id("sig9", 0, "prog_Swap");
        let id_b = generate_event_id("sig9", 0, "prog_Swap");
        assert_eq!(
            id_a, id_b,
            "same (sig, index, name) must hash to the same id — idempotent reprocess"
        );

        let id_other_index = generate_event_id("sig9", 1, "prog_Swap");
        assert_ne!(id_a, id_other_index, "different index must not collide");

        let id_other_name = generate_event_id("sig9", 0, "prog_Deposit");
        assert_ne!(id_a, id_other_name, "different event name must not collide");
    }

    /// The CPI dedup index is derived from the on-chain instruction position
    /// (outer, inner) — stable across reprocessing regardless of decode order.
    /// It must be deterministic, distinct per position, and never overlap the
    /// log-event counter range (which starts at 0) so a signature emitting both
    /// `emit!` and `emit_cpi!` events can't collide.
    #[test]
    fn test_cpi_dedup_index_stable_distinct_and_offset() {
        // Deterministic: same position → same index.
        assert_eq!(cpi_dedup_index(0, 0), cpi_dedup_index(0, 0));

        // Distinct per (outer, inner) pair.
        assert_ne!(cpi_dedup_index(0, 0), cpi_dedup_index(0, 1));
        assert_ne!(cpi_dedup_index(0, 1), cpi_dedup_index(1, 0));
        assert_ne!(cpi_dedup_index(1, 0), cpi_dedup_index(1, 1));

        // Offset above the log counter range — a log event at index 0 and a
        // CPI event at (0,0) must NOT share a dedup index.
        assert!(
            cpi_dedup_index(0, 0) > 1_000,
            "CPI index must sit far above the log-event counter range"
        );

        // The full dedup key (sig, cpi_index, name) is idempotent and distinct.
        let idx = cpi_dedup_index(3, 7);
        let id_a = generate_event_id("sigX", idx, "prog_Swap");
        let id_b = generate_event_id("sigX", idx, "prog_Swap");
        assert_eq!(id_a, id_b, "idempotent reprocess");
        assert_ne!(
            id_a,
            generate_event_id("sigX", cpi_dedup_index(3, 8), "prog_Swap"),
            "different inner position → different id"
        );
    }

    /// `decode_cpi_events` must surface the on-chain (outer, inner) position so
    /// callers can build the stable dedup key — not a processing-order counter.
    #[test]
    fn test_decode_cpi_events_surfaces_instruction_indices() {
        let program_id: Pubkey = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
            .parse()
            .unwrap();
        let decoder = EventDecoder::new(std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(swap_idl(&program_id))), ProgramPrefixConfig::new());

        let disc = IdlParser::calculate_discriminator("Swap");
        let mut payload = disc.to_vec();
        payload.extend_from_slice(&42u64.to_le_bytes());

        // cpi_event_tx nests the event at outer=0, inner=0.
        let tx = cpi_event_tx(&program_id, &payload);
        let decoded = decode_cpi_events(&tx, "sig", &decoder);

        assert_eq!(decoded.len(), 1);
        let (cpi, _event) = &decoded[0];
        assert_eq!(cpi.outer_index, 0);
        assert_eq!(cpi.inner_index, 0);
    }

    /// An `emit_cpi!` whose program has no loaded IDL (unknown discriminator)
    /// must be skipped with a debug log and never crash the indexer.
    #[test]
    fn test_decode_cpi_events_skips_program_without_idl() {
        let unknown: Pubkey = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
            .parse()
            .unwrap();
        // Decoder with NO IDL loaded for `unknown`.
        let decoder = EventDecoder::new(std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(IdlParser::new())), ProgramPrefixConfig::new());

        let disc = IdlParser::calculate_discriminator("Swap");
        let mut payload = disc.to_vec();
        payload.extend_from_slice(&1u64.to_le_bytes());
        let tx = cpi_event_tx(&unknown, &payload);

        // Unknown discriminator -> decode_event errs -> skipped, not panicked.
        assert!(
            decode_cpi_events(&tx, "sig", &decoder).is_empty(),
            "unknown-discriminator CPI event must be skipped, not crash"
        );
    }

    // --- load_onchain_idls_with: warn-and-continue injection tests ---

    use crate::types::ParsedIdl;

    fn make_idl(address: &str) -> ParsedIdl {
        serde_json::from_str(&format!(
            r#"{{"name":"T","events":[],"address":"{}"}}"#,
            address
        ))
        .unwrap()
    }

    fn pk(s: &str) -> Pubkey {
        s.parse().unwrap()
    }

    #[test]
    fn test_load_onchain_idls_inserts_ok_some() {
        let mut parser = IdlParser::new();
        let programs = vec![
            pk("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"),
            pk("11111111111111111111111111111111"),
        ];
        load_onchain_idls_with(&mut parser, &programs, |p| {
            Ok(Some(make_idl(&p.to_string())))
        });

        assert_eq!(parser.get_idls().len(), 2, "both IDLs should land");
        assert!(parser
            .get_idls()
            .contains_key("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"));
    }

    #[test]
    fn test_load_onchain_idls_skips_ok_none() {
        let mut parser = IdlParser::new();
        let programs = vec![pk("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")];
        load_onchain_idls_with(&mut parser, &programs, |_| Ok(None));

        assert!(parser.get_idls().is_empty(), "Ok(None) must not insert");
    }

    #[test]
    fn test_load_onchain_idls_skips_err() {
        let mut parser = IdlParser::new();
        let programs = vec![pk("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")];
        load_onchain_idls_with(&mut parser, &programs, |_| {
            Err(crate::error::SoltraceError::IdlParse("boom".into()))
        });

        assert!(
            parser.get_idls().is_empty(),
            "Err must not insert and must not propagate"
        );
    }

    #[test]
    fn test_load_onchain_idls_mixed_results() {
        let mut parser = IdlParser::new();
        let good = pk("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
        let none = pk("11111111111111111111111111111111");
        let errd = pk("SysvarRent111111111111111111111111111111111");
        let programs = vec![good, none, errd];
        load_onchain_idls_with(&mut parser, &programs, |p| {
            if p == &good {
                Ok(Some(make_idl(&p.to_string())))
            } else if p == &none {
                Ok(None)
            } else {
                Err(crate::error::SoltraceError::IdlParse("bad".into()))
            }
        });

        assert_eq!(parser.get_idls().len(), 1, "only the Ok(Some) entry lands");
        assert!(parser
            .get_idls()
            .contains_key("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"));
    }

    #[test]
    fn test_load_onchain_idls_empty_programs_is_noop() {
        let mut parser = IdlParser::new();
        load_onchain_idls_with(&mut parser, &[], |_| Ok(Some(make_idl("X"))));
        assert!(parser.get_idls().is_empty());
    }

    #[test]
    fn test_load_onchain_idls_preserves_existing_idl() {
        // Decision 1: file primary — a program already in the parser must NOT be
        // overwritten by an on-chain fetch. The fetcher must not even be called.
        let mut parser = IdlParser::new();
        let addr = pk("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
        parser.insert_or_replace(make_idl(&addr.to_string()));
        let mut fetch_called = false;
        load_onchain_idls_with(&mut parser, std::slice::from_ref(&addr), |_| {
            fetch_called = true;
            Ok(Some(make_idl("SHOULD_NOT_INSERT")))
        });

        assert!(!fetch_called, "fetcher must not be called for existing IDL");
        assert_eq!(parser.get_idls().len(), 1, "existing IDL must not be replaced");
        assert!(parser
            .get_idls()
            .contains_key("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"));
    }

    // --- Epic 1 integration tests (soltrace-as68) ---

    use std::io::Write as _;

    /// Build a program-metadata Metadata account blob (zlib+utf8+Direct)
    /// carrying `idl_json` for `program`. Mirrors the onchain_idl::tests layout.
    fn metadata_blob(program: &Pubkey, idl_json: &[u8]) -> Vec<u8> {
        use spl_program_metadata_client::types::{Compression, DataSource, Encoding};

        let mut enc =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(idl_json).unwrap();
        let data = enc.finish().unwrap();

        let mut seed = [0u8; 16];
        seed[..3].copy_from_slice(b"idl");
        let mut buf = Vec::new();
        buf.push(2u8); // disc = Metadata
        buf.extend_from_slice(program.as_ref()); // program (32)
        buf.extend_from_slice(&[0u8; 32]); // authority = None
        buf.push(1); // mutable
        buf.push(1); // canonical
        buf.extend_from_slice(&seed); // seed (16)
        buf.push(Encoding::Utf8 as u8);
        buf.push(Compression::Zlib as u8);
        buf.push(1u8); // format = Json
        buf.push(DataSource::Direct as u8);
        buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&data);
        buf
    }

    /// File IDL (program A) + on-chain decoded IDL (program B) coexist in the
    /// parser after load_onchain_idls_with processes a real Metadata blob.
    #[test]
    fn test_epic1_file_and_onchain_coexist() {
        let file_prog = pk("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
        let onchain_prog = pk("11111111111111111111111111111111");

        let mut parser = IdlParser::new();
        parser
            .load_from_str(
                r#"{"name":"File","events":[],"address":"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"}"#,
            )
            .unwrap();

        let blob = metadata_blob(
            &onchain_prog,
            br#"{"name":"Chain","events":[],"address":"11111111111111111111111111111111"}"#,
        );

        load_onchain_idls_with(&mut parser, &[onchain_prog], |p| {
            crate::onchain_idl::decode_metadata_account(&blob, p)
        });

        let idls = parser.get_idls();
        assert_eq!(idls.len(), 2, "file + on-chain IDLs must coexist");
        assert!(idls.contains_key("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"));
        assert!(idls.contains_key("11111111111111111111111111111111"));
        assert_eq!(
            idls.get("11111111111111111111111111111111")
                .unwrap()
                .name
                .as_deref(),
            Some("Chain"),
            "on-chain IDL must be the decoded value"
        );
    }

    /// ArcSwap hot-reload: EventDecoder reader observes a swapped-in IDL
    /// (simulates a live subscription pushing a new on-chain IDL).
    #[test]
    fn test_epic1_arcswap_reader_sees_swapped_idl() {
        let prog = pk("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

        // v1: empty parser (IDL not yet fetched).
        let shared = std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(IdlParser::new()));
        let decoder = EventDecoder::new(shared.clone(), ProgramPrefixConfig::new());

        let disc = IdlParser::calculate_discriminator("Swap");
        let payload: Vec<u8> = disc.iter().copied().chain(42u64.to_le_bytes()).collect();

        // Before swap: no IDL → decode fails cleanly.
        assert!(decoder.decode_event(&prog.to_string(), "sig", &payload).is_err());

        // Swap in a parser with the Swap event (on-chain IDL arrived).
        shared.store(std::sync::Arc::new(swap_idl(&prog)));

        // After swap: decode succeeds — reader sees the new state without panic.
        let decoded = decoder
            .decode_event(&prog.to_string(), "sig", &payload)
            .expect("reader must see the swapped-in IDL");
        assert_eq!(decoded.event_name, "default_Swap");
        assert_eq!(decoded.data["amount"], "42");
    }
}
