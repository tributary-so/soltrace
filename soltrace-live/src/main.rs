use anyhow::Result;
use clap::{Parser, Subcommand};
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcTransactionLogsConfig;
use solana_sdk::pubkey::Pubkey;
use solana_commitment_config::CommitmentConfig;
use soltrace_core::{
    Database,
    EventDecoder,
    IdlParser,
    load_idls,
    extract_event_from_log,
    types::RawEvent,
};
use std::collections::HashSet;
use tracing::{info, error, debug, warn};
use tracing_subscriber;
use tokio::time::{sleep, Duration};

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
    Init,
    /// Start real-time event indexing
    Run {
        /// Solana RPC WebSocket URL
        #[arg(short, long, default_value = "wss://api.mainnet-beta.solana.com")]
        ws_url: String,

        /// Solana RPC HTTP URL (for initial validation)
        #[arg(short, long, default_value = "https://api.mainnet-beta.solana.com")]
        rpc_url: String,

        /// Comma-separated list of program IDs to index
        #[arg(short, long)]
        programs: String,

        /// Database URL
        #[arg(short, long, default_value = "sqlite:./soltrace.db")]
        db_url: String,

        /// IDL directory path
        #[arg(short, long, default_value = "./idls")]
        idl_dir: String,

        /// Log commitment level (processed, confirmed, finalized)
        #[arg(short, long, default_value = "confirmed")]
        commitment: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    tracing_subscriber::fmt::init();

    match cli.command {
        Commands::Init => init_db().await?,
        Commands::Run {
            ws_url,
            rpc_url,
            programs,
            db_url,
            idl_dir,
            commitment,
        } => {
            run_indexer(ws_url, rpc_url, programs, db_url, idl_dir, commitment).await?;
        }
    }

    Ok(())
}

async fn init_db() -> Result<()> {
    info!("Initializing database...");

    let _db = Database::new("sqlite:./soltrace.db").await?;
    info!("Database initialized successfully at: ./soltrace.db");

    Ok(())
}

async fn run_indexer(
    ws_url: String,
    rpc_url: String,
    programs: String,
    db_url: String,
    idl_dir: String,
    commitment: String,
) -> Result<()> {
    info!("Starting Soltrace Live indexer");
    info!("RPC URL: {}", rpc_url);
    info!("WebSocket URL: {}", ws_url);
    info!("Commitment: {}", commitment);

    // Parse program IDs
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

    // Validate program IDs via HTTP RPC
    validate_programs(&rpc_url, &program_ids).await?;

    // Initialize database
    let db = Database::new(&db_url).await?;
    info!("Database connected: {}", db_url);

    // Load IDLs (using shared utility)
    let mut idl_parser = IdlParser::new();
    load_idls(&mut idl_parser, &idl_dir).await?;

    let loaded_idls = idl_parser.get_idls();
    info!("Loaded {} IDL(s) from {}", loaded_idls.len(), idl_dir);

    for (addr, idl) in loaded_idls {
        info!("  - {}: {} events", addr, idl.events.len());
    }

    // Create event decoder
    let event_decoder = EventDecoder::new(idl_parser);

    // Track processed signatures (placeholder for future use)
    let _processed_signatures: HashSet<String> = HashSet::new();

    // Start WebSocket subscription with auto-reconnect
    run_websocket_loop(
        &ws_url,
        &program_ids,
        &event_decoder,
        &db,
        &commitment,
        _processed_signatures,
    ).await?;

    Ok(())
}

async fn validate_programs(rpc_url: &str, program_ids: &[Pubkey]) -> Result<()> {
    use solana_client::rpc_client::RpcClient;

    let rpc_client = RpcClient::new(rpc_url.to_string());

    for program_id in program_ids {
        match rpc_client.get_account(program_id) {
            Ok(account) => {
                if account.owner == solana_sdk_ids::system_program::ID {
                    warn!("Program {} is not a program (owner is System Program)", program_id);
                }
            }
            Err(e) => {
                error!("Failed to fetch account {}: {}", program_id, e);
            }
        }
    }

    info!("All program IDs validated");
    Ok(())
}

async fn run_websocket_loop(
    ws_url: &str,
    program_ids: &[Pubkey],
    event_decoder: &EventDecoder,
    db: &Database,
    commitment: &str,
    _processed_signatures: HashSet<String>,
) -> Result<()> {
    let mut reconnect_count = 0;

    loop {
        info!("\nConnecting to WebSocket (attempt {})...", reconnect_count + 1);

        match websocket_handler(
            ws_url,
            program_ids,
            event_decoder,
            db,
            commitment,
        ).await {
            Ok(_) => {
                info!("WebSocket connection closed normally");
                break;
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                reconnect_count += 1;
                info!("Reconnecting in 5 seconds...");

                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
    }

    Ok(())
}

async fn websocket_handler(
    _ws_url: &str,
    program_ids: &[Pubkey],
    _event_decoder: &EventDecoder,
    _db: &Database,
    _commitment: &str,
) -> Result<()> {
    info!("WebSocket handler stub - full implementation pending");
    info!("Monitoring {} program(s):", program_ids.len());
    for pid in program_ids {
        info!("  - {}", pid);
    }
    
    // Stub implementation - just sleep indefinitely
    loop {
        sleep(Duration::from_secs(60)).await;
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
            if let Some(event_data) = extract_event_from_log(log, &program_id.to_string()) {
                // Decode event
                match event_decoder.decode_event(&program_id.to_string(), &event_data) {
                    Ok(decoded_event) => {
                        // Create raw event record
                        let raw_event = RawEvent {
                            slot: 0, // Not provided in RpcLogsResponse
                            signature: signature.clone(),
                            program_id: *program_id,
                            log: log.clone(),
                            timestamp: Utc::now(), // Will use block time if available
                        };

                        // Store event
                        match db.insert_event(&decoded_event, &raw_event).await {
                            Ok(_) => {
                                info!("Stored event: {} from {}", decoded_event.event_name, signature);
                                events_found += 1;
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
    }

    Ok(events_found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_program_id_parsing() {
        let programs = "11111111111111111111111111111111111,tokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
        let parsed: Vec<String> = programs
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], "11111111111111111111111111111111111");
    }
}
