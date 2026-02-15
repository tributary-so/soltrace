use anyhow::Result;
use clap::Parser;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_sdk::pubkey::Pubkey;
use solana_commitment_config::CommitmentConfig;
use soltrace_core::{
    Database,
    EventDecoder,
    IdlParser,
    load_idls,
    process_transaction,
    retry_with_rate_limit,
};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, error, debug, warn};
use tokio::task;

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

    /// Number of concurrent transaction fetches
    #[arg(long, default_value = "10")]
    concurrency: usize,

    /// Maximum retry attempts for failed requests
    #[arg(long, default_value = "3")]
    max_retries: u32,
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

    run_backfill(cli).await?;

    Ok(())
}

async fn run_backfill(cli: Cli) -> Result<()> {
    info!("Starting Soltrace Backfill");
    info!("RPC URL: {}", cli.rpc_url);
    info!("Fetching latest {} signatures per program", cli.limit);
    info!("Batch size: {}", cli.batch_size);
    info!("Concurrency: {}", cli.concurrency);
    info!("Max retries: {}", cli.max_retries);

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
    let db = Arc::new(Database::new(&cli.db_url).await?);
    info!("Database connected: {}", cli.db_url);

    // Load IDLs
    let mut idl_parser = IdlParser::new();
    load_idls(&mut idl_parser, &cli.idl_dir).await?;

    let loaded_idls = idl_parser.get_idls();
    info!("Loaded {} IDL(s) from {}", loaded_idls.len(), cli.idl_dir);
    for (addr, idl) in loaded_idls {
        info!("  - {}: {} events", addr, idl.events.len());
    }

    // Create event decoder
    let event_decoder = Arc::new(EventDecoder::new(idl_parser));

    // Initialize RPC client
    let rpc_client = Arc::new(RpcClient::new(cli.rpc_url));

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

        // Check if program exists with retry
        let account = retry_with_rate_limit(
            || async { rpc_client.get_account(&program_id) },
            cli.max_retries,
        ).await
        .map_err(|e| anyhow::anyhow!("Failed to fetch account {}: {}", program_id_str, e))?;

        if account.owner == solana_sdk_ids::system_program::ID {
            warn!("Program {} is not a program (owner is System Program)", program_id_str);
            continue;
        }

        // Get signatures for this program with retry
        info!("Fetching signatures for program {}...", program_id_str);

        use solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config;
        let signatures = retry_with_rate_limit(
            || async {
                let config = GetConfirmedSignaturesForAddress2Config {
                    before: None,
                    until: None,
                    limit: Some(cli.limit as usize),
                    commitment: Some(CommitmentConfig::confirmed()),
                };
                rpc_client.get_signatures_for_address_with_config(&program_id, config)
            },
            cli.max_retries,
        ).await
        .map_err(|e| anyhow::anyhow!("Failed to get signatures for {}: {}", program_id_str, e))?;

        let signatures_count = signatures.len();
        info!("Found {} signatures", signatures_count);
        total_signatures_fetched += signatures_count;

        // Process signatures with concurrency
        let signature_strings: Vec<String> = signatures
            .iter()
            .map(|sig| sig.signature.clone())
            .filter(|sig| !processed_signatures.contains(sig))
            .collect();

        let program_id_for_processing = program_id_str.clone();
        let program_events = process_signatures_concurrent(
            rpc_client.clone(),
            signature_strings,
            program_id_for_processing,
            event_decoder.clone(),
            db.clone(),
            &mut processed_signatures,
            cli.concurrency,
            cli.max_retries,
        ).await?;

        total_events_processed += program_events;
        info!("Program {} complete: {} events processed", program_id_str, program_events);

        // Delay between programs to avoid rate limiting
        tokio::time::sleep(Duration::from_millis(cli.batch_delay)).await;
    }

    info!("\nBackfill complete!");
    info!("Total signatures fetched: {}", total_signatures_fetched);
    info!("Total events processed: {}", total_events_processed);
    info!("Unique signatures processed: {}", processed_signatures.len());

    Ok(())
}

async fn process_signatures_concurrent(
    rpc_client: Arc<RpcClient>,
    signatures: Vec<String>,
    program_id_str: String,
    event_decoder: Arc<EventDecoder>,
    db: Arc<Database>,
    processed_signatures: &mut HashSet<String>,
    concurrency: usize,
    max_retries: u32,
) -> Result<usize> {
    let total = signatures.len();
    let mut processed_count = 0;
    let mut events_count = 0;

    // Process signatures in chunks to avoid overwhelming the RPC
    for chunk in signatures.chunks(concurrency * 2) {
        let mut handles = Vec::new();

        for signature in chunk.iter() {
            let rpc_client = rpc_client.clone();
            let program_id_str = program_id_str.clone();
            let event_decoder = event_decoder.clone();
            let db = db.clone();
            let sig_for_task = signature.clone();

            let handle = task::spawn(async move {
                process_single_signature(
                    &rpc_client,
                    &sig_for_task,
                    &program_id_str,
                    &event_decoder,
                    &db,
                    max_retries,
                ).await
            });

            handles.push((signature.clone(), handle));
        }

        // Wait for all tasks in this chunk
        for (signature, handle) in handles {
            processed_count += 1;
            
            match handle.await {
                Ok(Ok(event_count)) => {
                    events_count += event_count;
                    processed_signatures.insert(signature);
                }
                Ok(Err(e)) => {
                    debug!("Failed to process signature {}: {}", signature, e);
                }
                Err(e) => {
                    error!("Task panicked for signature {}: {}", signature, e);
                }
            }
        }

        // Progress update every 100 signatures
        if processed_count % 100 == 0 || processed_count >= total {
            info!("Progress: {}/{} signatures processed, {} events found", 
                  processed_count, total, events_count);
        }
    }

    Ok(events_count)
}

async fn process_single_signature(
    rpc_client: &RpcClient,
    signature: &str,
    program_id_str: &str,
    event_decoder: &EventDecoder,
    db: &Database,
    max_retries: u32,
) -> Result<usize> {
    // Parse signature
    let sig = signature.parse::<solana_sdk::signature::Signature>()
        .map_err(|e| anyhow::anyhow!("Invalid signature: {}", e))?;

    // Fetch transaction with retry
    let transaction = retry_with_rate_limit(
        || async {
            rpc_client.get_transaction_with_config(
                &sig,
                RpcTransactionConfig {
                    encoding: Some(solana_transaction_status::UiTransactionEncoding::Json),
                    commitment: Some(CommitmentConfig::confirmed()),
                    max_supported_transaction_version: Some(0),
                },
            )
        },
        max_retries,
    ).await
    .map_err(|e| anyhow::anyhow!("Failed to fetch transaction: {}", e))?;

    // Process transaction
    match process_transaction(transaction, program_id_str, event_decoder, db).await {
        Ok(processed) => Ok(processed.len()),
        Err(e) => Err(anyhow::anyhow!("Failed to process transaction: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_program_parsing() {
        let programs = "Prog1,Prog2,Prog3";
        let parsed: Vec<String> = programs
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0], "Prog1");
    }
}
