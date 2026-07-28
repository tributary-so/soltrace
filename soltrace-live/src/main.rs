// Live indexer threads many RPC/decode/db/ws handles through async fns;
// arg-bundling is out of scope for this change.
#![allow(clippy::too_many_arguments)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use futures::StreamExt;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::{
    RpcTransactionConfig, RpcTransactionLogsConfig, RpcTransactionLogsFilter,
};
use solana_commitment_config::CommitmentConfig;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use solana_sdk::pubkey::Pubkey;
use soltrace_core::{
    cpi_dedup_index, create_backend, decode_cpi_events, load_idls, process_transaction,
    retry_with_rate_limit, types::RawEvent, utils::extract_event_from_log, Database, EventDecoder,
    EventQueue, IdlParser, ProgramPrefixConfig, QueueEvent,
};
#[cfg(feature = "kafka")]
use soltrace_core::{KafkaConfig, KafkaProducer};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};

/// Soltrace Live - Real-time Solana event indexer via WebSocket
#[derive(Parser)]
#[command(name = "soltrace-live")]
#[command(about = "Real-time Solana event indexer using WebSocket logs", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize database
    Init {
        /// Database URL
        #[arg(short, long, default_value = "sqlite:./soltrace.db", env("DB_URL"))]
        db_url: String,
    },
    /// Start real-time event indexing
    Run {
        /// Solana RPC WebSocket URL
        #[arg(
            short,
            long,
            default_value = "wss://api.mainnet-beta.solana.com",
            env("SOLANA_WS_URL")
        )]
        ws_url: String,

        /// Solana RPC HTTP URL (for gap backfill and validation)
        #[arg(
            short,
            long,
            default_value = "https://api.mainnet-beta.solana.com",
            env("SOLANA_RPC_URL")
        )]
        rpc_url: String,

        /// Program prefix mappings (format: program_id:prefix, e.g., "TRibg8...:tributary")
        #[arg(short = 'm', long, env("PROGRAM_PREFIXES"))]
        program_prefixes: String,

        /// Database URL
        #[arg(short, long, default_value = "sqlite:./soltrace.db", env("DB_URL"))]
        db_url: String,

        /// IDL directory path
        #[arg(short, long, default_value = "./idls", env("IDL_DIR"))]
        idl_dir: String,

        /// Log commitment level (processed, confirmed, finalized)
        #[arg(short, long, default_value = "confirmed", env("COMMITMENT"))]
        commitment: String,

        /// Reconnect delay in seconds
        #[arg(long, default_value = "5", env("RECONNECT_DELAY"))]
        reconnect_delay: u64,

        /// Maximum number of reconnection attempts (0 = infinite)
        #[arg(long, default_value = "0", env("MAX_RECONNECT_ATTEMPTS"))]
        max_reconnects: u32,

        /// WebSocket ping interval in seconds (0 = disable)
        #[arg(long, default_value = "30", env("WS_PING_INTERVAL"))]
        ping_interval: u64,

        /// Kafka broker URLs (comma-separated, enables Kafka if set)
        #[arg(long, env("KAFKA_BROKERS"))]
        kafka_brokers: Option<String>,

        /// Maximum retry attempts for gap backfill RPC requests
        #[arg(long, default_value = "3", env("MAX_RETRIES"))]
        max_retries: u32,

        /// Disable gap backfill on startup
        #[arg(long, env("NO_GAP_BACKFILL"))]
        no_gap_backfill: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if present
    dotenv::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init { db_url } => init_db(&db_url).await?,
        Commands::Run {
            ws_url,
            rpc_url,
            program_prefixes,
            db_url,
            idl_dir,
            commitment,
            reconnect_delay,
            max_reconnects,
            ping_interval,
            kafka_brokers,
            max_retries,
            no_gap_backfill,
        } => {
            run_indexer(
                ws_url,
                rpc_url,
                program_prefixes,
                db_url,
                idl_dir,
                commitment,
                reconnect_delay,
                max_reconnects,
                ping_interval,
                kafka_brokers,
                max_retries,
                no_gap_backfill,
            )
            .await?;
        }
    }

    Ok(())
}

async fn init_db(db_url: &str) -> Result<()> {
    info!("Initializing database...");

    let _db = create_backend(db_url).await?;
    info!("Database initialized successfully at: {}", db_url);

    Ok(())
}

async fn run_indexer(
    ws_url: String,
    rpc_url: String,
    program_prefixes: String,
    db_url: String,
    idl_dir: String,
    commitment: String,
    reconnect_delay: u64,
    max_reconnects: u32,
    ping_interval: u64,
    kafka_brokers: Option<String>,
    max_retries: u32,
    no_gap_backfill: bool,
) -> Result<()> {
    info!("Starting Soltrace Live indexer");
    info!("RPC URL: {}", rpc_url);
    info!("WebSocket URL: {}", ws_url);
    info!("Commitment: {}", commitment);
    info!("Reconnect delay: {}s", reconnect_delay);

    let kafka_producer: Option<Arc<dyn EventQueue>> = match &kafka_brokers {
        #[allow(unused_variables)]
        Some(brokers) => {
            #[cfg(feature = "kafka")]
            {
                let config = KafkaConfig::new(brokers.clone());
                match KafkaProducer::new(config) {
                    Ok(producer) => {
                        info!(
                            "Kafka enabled: {} (dynamic topics from event names)",
                            brokers
                        );
                        Some(Arc::new(producer))
                    }
                    Err(e) => {
                        error!("Failed to initialize Kafka producer: {}", e);
                        return Err(e);
                    }
                }
            }
            #[cfg(not(feature = "kafka"))]
            {
                error!("Kafka brokers configured but 'kafka' feature not enabled. Recompile with --features kafka");
                return Err(anyhow::anyhow!("Kafka feature not enabled"));
            }
        }
        None => {
            info!("Kafka not configured (set KAFKA_BROKERS to enable)");
            None
        }
    };

    // Initialize database
    let db = create_backend(&db_url).await?;
    info!("Database connected: {}", db_url);

    // Load IDLs first to extract program IDs
    let mut idl_parser = IdlParser::new();
    load_idls(&mut idl_parser, &idl_dir).await?;

    let loaded_idls = idl_parser.get_idls();
    info!("Loaded {} IDL(s) from {}", loaded_idls.len(), idl_dir);
    for (addr, idl) in loaded_idls {
        info!("  - {}: {} events", addr, idl.events.len());
    }

    // Create program prefix configuration from CLI/env
    let mut prefix_config = ProgramPrefixConfig::new();
    // Load programs from IDLs with default prefix
    prefix_config.load_from_idls(loaded_idls);
    // Apply custom prefix mappings from CLI/env
    if !program_prefixes.is_empty() {
        prefix_config.add_mappings_from_string(&program_prefixes);
        info!(
            "Applied {} custom program prefix mapping(s)",
            program_prefixes
        );
    }

    let program_ids = prefix_config.get_program_ids();
    if program_ids.is_empty() {
        error!("No IDLs found in directory. Use --idl-dir <path>");
        return Ok(());
    }

    // Convert program IDs to Pubkeys for WebSocket subscription
    let pubkeys: Vec<Pubkey> = program_ids
        .iter()
        .map(|s| s.parse::<Pubkey>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("Failed to parse program IDs: {}", e))?;

    // Create event decoder
    let event_decoder = Arc::new(EventDecoder::new(idl_parser, prefix_config));

    // Initialize RPC client for gap backfill
    let rpc_client = Arc::new(RpcClient::new(rpc_url.clone()));

    // Spawn gap backfill concurrently with WebSocket
    let backfill_handle = if !no_gap_backfill {
        let db_clone = db.clone();
        let event_decoder_clone = event_decoder.clone();
        let rpc_client_clone = rpc_client.clone();
        let kafka_producer_clone = kafka_producer.clone();
        let program_ids_clone = program_ids.clone();
        let commitment_clone = commitment.clone();
        let max_retries_clone = max_retries;

        Some(tokio::spawn(async move {
            gap_backfill(
                &rpc_client_clone,
                &program_ids_clone,
                &event_decoder_clone,
                &db_clone,
                kafka_producer_clone.as_ref(),
                &commitment_clone,
                max_retries_clone,
            )
            .await
        }))
    } else {
        info!("Gap backfill disabled");
        None
    };

    // Start WebSocket subscription with auto-reconnect
    let ws_result = run_websocket_loop(
        &ws_url,
        &pubkeys,
        event_decoder,
        db,
        kafka_producer,
        &commitment,
        reconnect_delay,
        max_reconnects,
        ping_interval,
        rpc_client,
        max_retries,
    )
    .await;

    // Wait for backfill to complete (if still running when WS exits)
    if let Some(handle) = backfill_handle {
        match handle.await {
            Ok(Ok(count)) => info!("Gap backfill completed: {} events backfilled", count),
            Ok(Err(e)) => error!("Gap backfill failed: {}", e),
            Err(e) => error!("Gap backfill task panicked: {}", e),
        }
    }

    ws_result
}

async fn gap_backfill(
    rpc_client: &Arc<RpcClient>,
    program_ids: &[String],
    event_decoder: &Arc<EventDecoder>,
    db: &Database,
    _kafka_producer: Option<&Arc<dyn EventQueue>>,
    commitment: &str,
    max_retries: u32,
) -> Result<usize> {
    let latest_sig = db.get_latest_signature().await?;
    let latest_sig = match latest_sig {
        Some(sig) => {
            info!("Gap backfill: latest stored signature = {}", sig);
            sig
        }
        None => {
            info!("Gap backfill: no events in DB, skipping gap fill");
            return Ok(0);
        }
    };

    let until_parsed = latest_sig
        .parse::<solana_sdk::signature::Signature>()
        .map_err(|e| anyhow::anyhow!("Invalid signature {}: {}", latest_sig, e))?;

    let commitment_config = parse_commitment(commitment)?;
    let mut total_events = 0;

    use solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config;

    for program_id_str in program_ids {
        let program_id = program_id_str
            .parse::<Pubkey>()
            .map_err(|e| anyhow::anyhow!("Invalid program ID: {}", e))?;

        let mut all_sigs = Vec::new();
        let mut before: Option<solana_sdk::signature::Signature> = None;
        let page_size = 1000usize;

        loop {
            let rpc = rpc_client.clone();
            let page = retry_with_rate_limit(
                || {
                    let rpc = rpc.clone();
                    let until = until_parsed;
                    async move {
                        let config = GetConfirmedSignaturesForAddress2Config {
                            before,
                            until: Some(until),
                            limit: Some(page_size),
                            commitment: Some(commitment_config),
                        };
                        rpc.get_signatures_for_address_with_config(&program_id, config)
                    }
                },
                max_retries,
            )
            .await
            .map_err(|e| anyhow::anyhow!("Gap backfill failed for {}: {}", program_id_str, e))?;

            let page_len = page.len();
            if page_len == 0 {
                break;
            }

            all_sigs.extend(page);

            if page_len < page_size {
                break;
            }

            if let Some(last) = all_sigs.last() {
                before = last
                    .signature
                    .parse::<solana_sdk::signature::Signature>()
                    .ok();
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        all_sigs.reverse();

        let sig_count = all_sigs.len();
        if sig_count == 0 {
            info!(
                "Gap backfill [{}]: no gap, DB is up to date",
                program_id_str
            );
            continue;
        }

        info!(
            "Gap backfill [{}]: found {} signature(s) in gap",
            program_id_str, sig_count
        );

        let mut processed = 0;
        for sig_info in &all_sigs {
            if let Some(_err) = &sig_info.err {
                continue;
            }

            let sig = sig_info
                .signature
                .parse::<solana_sdk::signature::Signature>()
                .map_err(|e| anyhow::anyhow!("Invalid signature: {}", e))?;

            let tx = retry_with_rate_limit(
                || {
                    let rpc = rpc_client.clone();
                    async move {
                        rpc.get_transaction_with_config(
                            &sig,
                            RpcTransactionConfig {
                                encoding: Some(
                                    solana_transaction_status::UiTransactionEncoding::Json,
                                ),
                                commitment: Some(commitment_config),
                                max_supported_transaction_version: Some(0),
                            },
                        )
                    }
                },
                max_retries,
            )
            .await;

            match tx {
                Ok(transaction) => {
                    match process_transaction(transaction, program_id_str, event_decoder, db).await
                    {
                        Ok(sigs) => {
                            processed += sigs.len();
                        }
                        Err(e) => {
                            warn!(
                                "Gap backfill: failed to process tx {}: {}",
                                sig_info.signature, e
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        "Gap backfill: failed to fetch tx {}: {}",
                        sig_info.signature, e
                    );
                }
            }

            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        info!(
            "Gap backfill [{}]: {} events from {} signatures",
            program_id_str, processed, sig_count
        );
        total_events += processed;
    }

    info!("Gap backfill complete: {} total events", total_events);
    Ok(total_events)
}

async fn run_websocket_loop(
    ws_url: &str,
    program_ids: &[Pubkey],
    event_decoder: Arc<EventDecoder>,
    db: Database,
    kafka_producer: Option<Arc<dyn EventQueue>>,
    commitment: &str,
    reconnect_delay: u64,
    max_reconnects: u32,
    ping_interval: u64,
    rpc_client: Arc<RpcClient>,
    max_retries: u32,
) -> Result<()> {
    let mut reconnect_count: u32 = 0;
    let program_ids_vec: Vec<_> = program_ids.iter().map(|p| p.to_string()).collect();

    loop {
        if max_reconnects > 0 && reconnect_count >= max_reconnects {
            error!(
                "Maximum reconnection attempts ({}) reached. Exiting.",
                max_reconnects
            );
            return Err(anyhow::anyhow!("Max reconnections exceeded"));
        }

        info!(
            "\nConnecting to WebSocket (attempt {})...",
            reconnect_count + 1
        );

        match websocket_handler(
            ws_url,
            program_ids,
            &program_ids_vec,
            event_decoder.clone(),
            db.clone(),
            kafka_producer.clone(),
            commitment,
            ping_interval,
            rpc_client.clone(),
            max_retries,
        )
        .await
        {
            Ok(_) => {
                info!("WebSocket connection closed normally, reconnecting...");
                reconnect_count += 1;
                let delay = if reconnect_count > 10 {
                    Duration::from_secs(60)
                } else {
                    Duration::from_secs(reconnect_delay * reconnect_count as u64)
                };
                info!("Reconnecting in {:?}...", delay);
                sleep(delay).await;
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                reconnect_count += 1;

                let delay = if reconnect_count > 10 {
                    Duration::from_secs(60)
                } else {
                    Duration::from_secs(reconnect_delay * reconnect_count as u64)
                };

                info!("Reconnecting in {:?}...", delay);
                sleep(delay).await;
            }
        }
    }
}

async fn websocket_handler(
    ws_url: &str,
    program_ids: &[Pubkey],
    program_ids_str: &[String],
    event_decoder: Arc<EventDecoder>,
    db: Database,
    kafka_producer: Option<Arc<dyn EventQueue>>,
    commitment: &str,
    ping_interval: u64,
    rpc_client: Arc<RpcClient>,
    max_retries: u32,
) -> Result<()> {
    info!("Connecting to WebSocket at: {}", ws_url);
    info!("Monitoring {} program(s):", program_ids.len());
    for pid in program_ids {
        info!("  - {}", pid);
    }

    // Parse commitment config
    let commitment_config = parse_commitment(commitment)?;

    // Create PubsubClient
    let pubsub_client = PubsubClient::new(ws_url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to WebSocket: {}", e))?;

    info!("WebSocket connected successfully");

    // Subscribe to logs for the specified programs
    let filter = RpcTransactionLogsFilter::Mentions(program_ids_str.to_vec());
    let logs_config = RpcTransactionLogsConfig {
        commitment: Some(commitment_config),
    };

    let (mut notifications, unsubscribe) = pubsub_client
        .logs_subscribe(filter, logs_config)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to subscribe to logs: {}", e))?;

    info!("Successfully subscribed to program logs");
    info!("WebSocket keep-alive: read timeout = {}s", ping_interval);

    // Create channel for processing logs asynchronously
    let (tx, mut rx) = mpsc::channel::<solana_client::rpc_response::RpcLogsResponse>(100);
    let db_clone = db.clone();
    let event_decoder_clone = event_decoder.clone();
    let kafka_producer_clone = kafka_producer.clone();
    let program_ids_clone: Vec<_> = program_ids.to_vec();
    let rpc_client_clone = rpc_client;

    // Spawn processing task
    let processor_handle = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            match process_logs_message(
                message,
                &program_ids_clone,
                &event_decoder_clone,
                &db_clone,
                kafka_producer_clone.as_ref(),
                &rpc_client_clone,
                commitment_config,
                max_retries,
            )
            .await
            {
                Ok(count) => {
                    if count > 0 {
                        debug!("Processed {} events", count);
                    }
                }
                Err(e) => {
                    error!("Error processing logs message: {}", e);
                }
            }
        }
    });

    // Main loop: receive notifications and send to processor
    let read_timeout = if ping_interval > 0 {
        Duration::from_secs(ping_interval)
    } else {
        Duration::from_secs(60) // default if disabled
    };

    let result: Result<()> = async {
        loop {
            match timeout(read_timeout, notifications.next()).await {
                Ok(Some(response)) => {
                    // Response is Response<RpcLogsResponse>, extract the value
                    if let Err(e) = tx.send(response.value).await {
                        error!("Failed to send log to processor: {}", e);
                        break;
                    }
                }
                Ok(None) => {
                    info!("WebSocket stream ended");
                    break;
                }
                Err(_) => {
                    // Timeout - connection is still alive but no messages
                    debug!(
                        "No messages received in {:?}, connection still alive",
                        read_timeout
                    );
                }
            }
        }
        Ok(())
    }
    .await;

    // Cleanup
    drop(tx);
    let _ = processor_handle.await;

    // Unsubscribe
    unsubscribe().await;

    result
}

fn parse_commitment(commitment: &str) -> Result<CommitmentConfig> {
    match commitment.to_lowercase().as_str() {
        "processed" => Ok(CommitmentConfig::processed()),
        "confirmed" => Ok(CommitmentConfig::confirmed()),
        "finalized" => Ok(CommitmentConfig::finalized()),
        _ => Err(anyhow::anyhow!(
            "Invalid commitment level: {}. Use 'processed', 'confirmed', or 'finalized'",
            commitment
        )),
    }
}

/// Process a logs message from PubsubClient
async fn process_logs_message(
    message: solana_client::rpc_response::RpcLogsResponse,
    program_ids: &[Pubkey],
    event_decoder: &EventDecoder,
    db: &Database,
    kafka_producer: Option<&Arc<dyn EventQueue>>,
    rpc_client: &Arc<RpcClient>,
    commitment_config: CommitmentConfig,
    max_retries: u32,
) -> Result<usize> {
    use chrono::Utc;

    // Skip failed transactions
    if let Some(err) = &message.err {
        debug!("Skipping failed transaction: {:?}", err);
        return Ok(0);
    }

    let signature = &message.signature;
    let logs = &message.logs;

    // Process logs for events
    let mut events_found = 0;

    for log in logs {
        for program_id in program_ids {
            if let Some(event_data) = extract_event_from_log(log) {
                // Decode event
                match event_decoder.decode_event(&program_id.to_string(), signature, &event_data) {
                    Ok(decoded_event) => {
                        // Create raw event record
                        let raw_event = RawEvent {
                            slot: 0, // Not provided in RpcLogsResponse
                            signature: signature.clone(),
                            program_id: *program_id,
                            log: log.clone(),
                            timestamp: Utc::now(),
                        };

                        // Store event in database
                        match db
                            .insert_event(&decoded_event, &raw_event, events_found)
                            .await
                        {
                            Ok(_) => {
                                info!(
                                    "Stored event: {} from {}",
                                    decoded_event.event_name, signature
                                );
                                events_found += 1;
                            }
                            Err(e) => {
                                let err_str = e.to_string();
                                if err_str.contains("UNIQUE constraint")
                                    || err_str.contains("duplicate")
                                {
                                    debug!("Event {} already exists, skipping", signature);
                                } else {
                                    error!("Failed to store event: {}", e);
                                }
                            }
                        }

                        // Send to Kafka if configured
                        if let Some(producer) = kafka_producer {
                            let queue_event = QueueEvent::new(
                                decoded_event.event_name.clone(),
                                signature.clone(),
                                program_id.to_string(),
                                decoded_event.data.clone(),
                            );
                            if let Err(e) = producer.send(&queue_event).await {
                                error!("Failed to send event to Kafka: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        debug!("Failed to decode event: {}", e);
                    }
                }
            }
        }
    }

    // emit_cpi! events live in inner instructions, NOT program logs. The WS
    // log notification only carries the signature; fetch the full transaction
    // (maxSupportedTransactionVersion=0 expands ALT keys) to access them.
    //
    // Decode failures fall back to hex (event.rs) and unknown-discriminator
    // events are skipped — never crashes the indexer (see decode_cpi_events).
    let sig = match signature.parse::<solana_sdk::signature::Signature>() {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to parse signature {}: {}", signature, e);
            return Ok(events_found);
        }
    };

    let tx_result = retry_with_rate_limit(
        || {
            let rpc = rpc_client.clone();
            async move {
                rpc.get_transaction_with_config(
                    &sig,
                    RpcTransactionConfig {
                        encoding: Some(solana_transaction_status::UiTransactionEncoding::Json),
                        commitment: Some(commitment_config),
                        max_supported_transaction_version: Some(0),
                    },
                )
            }
        },
        max_retries,
    )
    .await;

    let transaction = match tx_result {
        Ok(tx) => tx,
        Err(e) => {
            warn!("Failed to fetch tx {} for CPI decode: {}", signature, e);
            return Ok(events_found);
        }
    };

    let slot = transaction.slot;
    let timestamp = transaction
        .block_time
        .and_then(|bt| chrono::DateTime::from_timestamp(bt, 0))
        .unwrap_or_else(Utc::now);

    for (cpi, decoded_event) in decode_cpi_events(&transaction, signature, event_decoder) {
        let raw_event = RawEvent {
            slot,
            signature: signature.clone(),
            program_id: cpi.program_id,
            log: String::new(),
            timestamp,
        };

        match db
            .insert_event(
                &decoded_event,
                &raw_event,
                cpi_dedup_index(cpi.outer_index, cpi.inner_index),
            )
            .await
        {
            Ok(_) => {
                info!(
                    "Stored CPI event: {} from {}",
                    decoded_event.event_name, signature
                );
                events_found += 1;
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("UNIQUE constraint") || err_str.contains("duplicate") {
                    debug!("CPI event {} already exists, skipping", signature);
                } else {
                    error!("Failed to store CPI event: {}", e);
                }
            }
        }

        // Send to Kafka if configured
        if let Some(producer) = kafka_producer {
            let queue_event = QueueEvent::new(
                decoded_event.event_name.clone(),
                signature.clone(),
                cpi.program_id.to_string(),
                decoded_event.data.clone(),
            );
            if let Err(e) = producer.send(&queue_event).await {
                error!("Failed to send CPI event to Kafka: {}", e);
            }
        }
    }

    Ok(events_found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_program_id_parsing() {
        let programs =
            "11111111111111111111111111111111,TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
        let parsed: Vec<String> = programs
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], "11111111111111111111111111111111");
    }

    #[test]
    fn test_parse_commitment() {
        assert!(parse_commitment("confirmed").is_ok());
        assert!(parse_commitment("processed").is_ok());
        assert!(parse_commitment("finalized").is_ok());
        assert!(parse_commitment("invalid").is_err());
    }
}
