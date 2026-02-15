use anyhow::Result;
use clap::Parser;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::{RpcSignaturesForAddressConfig, RpcTransactionConfig};
use solana_sdk::pubkey::Pubkey;
use soltrace_core::{
    db::Database,
    event::EventDecoder,
    idl::IdlParser,
    utils::{load_idls, process_transaction},
};
use std::collections::HashSet;
use tracing::{info, error, debug, warn};
use tracing_subscriber;

/// Soltrace Backfill - Historical Solana event indexer
#[derive(Parser)]
#[command(name = "soltrace-backfill")]
#[command(about = "Backfill historical Solana events from RPC", long_about = None)]
struct Cli {
    /// Solana RPC URL
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

    /// Number of signatures to fetch (latest N transactions)
    #[arg(short, long, default_value = "1000")]
    limit: u64,

    /// Batch size for fetching transactions
    #[arg(short, long, default_value = "100")]
    batch_size: usize,

    /// Delay between batches (milliseconds)
    #[arg(short, long, default_value = "100")]
    batch_delay: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    run_backfill(cli).await?;

    Ok(())
}

async fn run_backfill(cli: Cli) -> Result<()> {
    info!("Starting Soltrace Backfill");
    info!("RPC URL: {}", cli.rpc_url);
    info!("Fetching latest {} signatures per program", cli.limit);
    info!("Batch size: {}", cli.batch_size);

    // Parse program IDs
    let program_ids: Vec<String> = cli
        .programs
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if program_ids.is_empty() {
        error!("No program IDs specified. Use --programs <id1,id2,...>");
        return Ok(());
    }

    info!("Indexing {} program(s):", program_ids.len());
    for pid in &program_ids {
        info!("  - {}", pid);
    }

    // Initialize database
    let db = Database::new(&cli.db_url).await?;
    info!("Database connected: {}", cli.db_url);

    // Load IDLs (using shared utility)
    let mut idl_parser = IdlParser::new();
    load_idls(&mut idl_parser, &cli.idl_dir).await?;

    let loaded_idls = idl_parser.get_idls();
    info!("Loaded {} IDL(s) from {}", loaded_idls.len(), cli.idl_dir);

    for (addr, idl) in loaded_idls {
        info!("  - {}: {} events", addr, idl.events.len());
    }

    // Create event decoder
    let event_decoder = EventDecoder::new(idl_parser);

    // Initialize RPC client
    let rpc_client = RpcClient::new(cli.rpc_url);

    // Track processed signatures across all programs
    let mut processed_signatures: HashSet<String> = HashSet::new();

    // Process each program
    let mut total_signatures_fetched = 0;
    let mut total_events_processed = 0;

    for program_id_str in &program_ids {
        info!("\nProcessing program: {}", program_id_str);

        // Validate and parse program ID
        let program_id = program_id_str.parse::<Pubkey>()
            .map_err(|e| anyhow::anyhow!("Invalid program ID {}: {}", program_id_str, e))?;

        // Check if program exists
        let account = rpc_client.get_account(&program_id)
            .map_err(|e| anyhow::anyhow!("Failed to fetch account {}: {}", program_id_str, e))?;

        if account.owner == solana_sdk::system_program::id() {
            warn!("Program {} is not a program (owner is System Program)", program_id_str);
            continue;
        }

        // Get signatures for this program
        info!("Fetching signatures for program {}...", program_id_str);

        let config = RpcSignaturesForAddressConfig {
            before: None,
            until: None,
            limit: Some(cli.limit),
            commitment: Some(solana_sdk::commitment_config::CommitmentConfig::confirmed()),
        };

        let signatures = rpc_client.get_signatures_for_address_with_config(&program_id, config)
            .map_err(|e| anyhow::anyhow!("Failed to get signatures for {}: {}", program_id_str, e))?;

        let signatures_count = signatures.len();
        info!("Found {} signatures", signatures_count);
        total_signatures_fetched += signatures_count;

        // Process signatures in batches
        let mut batch_start = 0;
        let mut program_events = 0;

        while batch_start < signatures_count {
            let batch_end = (batch_start + cli.batch_size).min(signatures_count);
            info!("Processing batch {} to {} of {} signatures",
                  batch_start + 1, batch_end, signatures_count);

            let batch_signatures: Vec<_> = signatures[batch_start..batch_end]
                .iter()
                .map(|sig| sig.signature.clone())
                .collect();

            // Fetch transactions in batch
            match process_signature_batch(
                &rpc_client,
                &batch_signatures,
                program_id_str,
                &event_decoder,
                &db,
                &mut processed_signatures,
            ) {
                Ok(events_count) => {
                    program_events += events_count;
                    info!("  Processed {} events", events_count);
                }
                Err(e) => {
                    error!("  Batch failed: {}", e);
                }
            }

            batch_start = batch_end;

            // Add delay between batches
            tokio::time::sleep(tokio::time::Duration::from_millis(cli.batch_delay)).await;
        }

        total_events_processed += program_events;
        info!("Program {} complete: {} events processed", program_id_str, program_events);
    }

    info!("\nBackfill complete!");
    info!("Total signatures fetched: {}", total_signatures_fetched);
    info!("Total events processed: {}", total_events_processed);
    info!("Unique signatures processed: {}", processed_signatures.len());

    Ok(())
}

async fn process_signature_batch(
    rpc_client: &RpcClient,
    signatures: &[String],
    program_id_str: &str,
    event_decoder: &EventDecoder,
    db: &Database,
    processed_signatures: &mut HashSet<String>,
) -> Result<usize> {
    use solana_transaction_status::UiTransactionEncoding;

    let mut events_processed = 0;

    // Fetch transactions
    for signature in signatures {
        // Skip if already processed
        if processed_signatures.contains(signature) {
            continue;
        }

        // Parse signature
        let sig = match signature.parse::<solana_sdk::signature::Signature>() {
            Ok(sig) => sig,
            Err(e) => {
                debug!("Failed to parse signature {}: {}", signature, e);
                continue;
            }
        };

        // Fetch transaction
        let transaction = match rpc_client.get_transaction_with_config(
            &sig,
            RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::Json),
                commitment: Some(solana_sdk::commitment_config::CommitmentConfig::confirmed()),
                max_supported_transaction_version: Some(0),
            },
        ) {
            Ok(tx) => tx,
            Err(e) => {
                debug!("Failed to fetch transaction {}: {}", signature, e);
                continue;
            }
        };

        // Process transaction (using shared utility)
        match process_transaction(transaction, program_id_str, event_decoder, db) {
            Ok(processed) => {
                events_processed += processed.len();
                processed_signatures.extend(processed);
            }
            Err(e) => {
                debug!("Failed to process transaction {}: {}", signature, e);
            }
        }
    }

    Ok(events_processed)
}
