use anyhow::Result;
use clap::{Parser, Subcommand};
use futures::StreamExt;
use solana_client::rpc_config::{RpcTransactionLogsConfig, RpcTransactionLogsFilter};
use solana_commitment_config::CommitmentConfig;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use solana_sdk::pubkey::Pubkey;
use soltrace_core::{
    load_idls, types::RawEvent, utils::extract_event_from_log, Database, EventDecoder, IdlParser,
};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info};

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

        /// Solana RPC HTTP URL (for initial validation)
        #[arg(
            short,
            long,
            default_value = "https://api.mainnet-beta.solana.com",
            env("SOLANA_RPC_URL")
        )]
        rpc_url: String,

        /// Comma-separated list of program IDs to index
        #[arg(short, long, env("PROGRAM_IDS"))]
        programs: String,

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
            programs,
            db_url,
            idl_dir,
            commitment,
            reconnect_delay,
            max_reconnects,
        } => {
            run_indexer(
                ws_url,
                rpc_url,
                programs,
                db_url,
                idl_dir,
                commitment,
                reconnect_delay,
                max_reconnects,
            )
            .await?;
        }
    }

    Ok(())
}

async fn init_db(db_url: &str) -> Result<()> {
    info!("Initializing database...");

    let _db = Database::new(db_url).await?;
    info!("Database initialized successfully at: {}", db_url);

    Ok(())
}

async fn run_indexer(
    ws_url: String,
    rpc_url: String,
    programs: String,
    db_url: String,
    idl_dir: String,
    commitment: String,
    reconnect_delay: u64,
    max_reconnects: u32,
) -> Result<()> {
    info!("Starting Soltrace Live indexer");
    info!("RPC URL: {}", rpc_url);
    info!("WebSocket URL: {}", ws_url);
    info!("Commitment: {}", commitment);
    info!("Reconnect delay: {}s", reconnect_delay);

    // Parse and validate program IDs
    let program_ids: Vec<Pubkey> = programs
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<Pubkey>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("Failed to parse program IDs: {}", e))?;

    if program_ids.is_empty() {
        error!("No program IDs specified. Use --programs <id1,id2,...>");
        return Ok(());
    }

    info!("Indexing {} program(s):", program_ids.len());
    for pid in &program_ids {
        info!("  - {}", pid);
    }

    // Initialize database
    let db = Arc::new(Database::new(&db_url).await?);
    info!("Database connected: {}", db_url);

    // Load IDLs
    let mut idl_parser = IdlParser::new();
    load_idls(&mut idl_parser, &idl_dir).await?;

    let loaded_idls = idl_parser.get_idls();
    info!("Loaded {} IDL(s) from {}", loaded_idls.len(), idl_dir);
    for (addr, idl) in loaded_idls {
        info!("  - {}: {} events", addr, idl.events.len());
    }

    // Create event decoder
    let event_decoder = Arc::new(EventDecoder::new(idl_parser));

    // Track processed signatures to avoid duplicates
    let processed_signatures = Arc::new(tokio::sync::Mutex::new(HashSet::<String>::new()));

    // Start WebSocket subscription with auto-reconnect
    run_websocket_loop(
        &ws_url,
        &program_ids,
        event_decoder,
        db,
        &commitment,
        reconnect_delay,
        max_reconnects,
        processed_signatures,
    )
    .await?;

    Ok(())
}

async fn run_websocket_loop(
    ws_url: &str,
    program_ids: &[Pubkey],
    event_decoder: Arc<EventDecoder>,
    db: Arc<Database>,
    commitment: &str,
    reconnect_delay: u64,
    max_reconnects: u32,
    processed_signatures: Arc<tokio::sync::Mutex<HashSet<String>>>,
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
            commitment,
            processed_signatures.clone(),
        )
        .await
        {
            Ok(_) => {
                info!("WebSocket connection closed normally");
                break;
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                reconnect_count += 1;

                let delay = if reconnect_count > 10 {
                    // Cap exponential backoff at ~17 minutes
                    Duration::from_secs(60 * 15)
                } else {
                    Duration::from_secs(reconnect_delay * 2u64.pow(reconnect_count.min(10)))
                };

                info!("Reconnecting in {:?}...", delay);
                sleep(delay).await;
            }
        }
    }

    Ok(())
}

async fn websocket_handler(
    ws_url: &str,
    program_ids: &[Pubkey],
    program_ids_str: &[String],
    event_decoder: Arc<EventDecoder>,
    db: Arc<Database>,
    commitment: &str,
    _processed_signatures: Arc<tokio::sync::Mutex<HashSet<String>>>,
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

    // Create channel for processing logs asynchronously
    let (tx, mut rx) = mpsc::channel::<solana_client::rpc_response::RpcLogsResponse>(100);
    let db_clone = db.clone();
    let event_decoder_clone = event_decoder.clone();
    let program_ids_clone: Vec<_> = program_ids.to_vec();

    // Spawn processing task
    let processor_handle = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            match process_logs_message(message, &program_ids_clone, &event_decoder_clone, &db_clone)
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
    let result: Result<()> = async {
        loop {
            match timeout(Duration::from_secs(60), notifications.next()).await {
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
                    debug!("No messages received in 60 seconds, connection still alive");
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
                match event_decoder.decode_event(&program_id.to_string(), &event_data) {
                    Ok(decoded_event) => {
                        // Create raw event record
                        let raw_event = RawEvent {
                            slot: 0, // Not provided in RpcLogsResponse
                            signature: signature.clone(),
                            program_id: *program_id,
                            log: log.clone(),
                            timestamp: Utc::now(),
                        };

                        // Store event
                        match db.insert_event(&decoded_event, &raw_event).await {
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
                    }
                    Err(e) => {
                        debug!("Failed to decode event: {}", e);
                    }
                }
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
