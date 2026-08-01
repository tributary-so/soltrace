// Processing fns thread many RPC/decode/db handles; arg-bundling is out of
// scope for this change.
#![allow(clippy::too_many_arguments)]

use anyhow::Result;
use clap::Parser;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use soltrace_core::{
    create_backend, load_idls, process_transaction, retry_with_rate_limit, Database,
    EventDecoder, IdlParser, ProgramPrefixConfig,
};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::task;
use tracing::{debug, error, info, warn};

/// Soltrace Backfill - Historical Solana event indexer
#[derive(Parser)]
#[command(name = "soltrace-backfill")]
#[command(about = "Backfill historical Solana events from RPC", long_about = None)]
struct Cli {
    /// Solana RPC URL
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

    /// On-chain program IDs to fetch Anchor IDLs from via program-metadata
    /// (comma-separated base58, e.g. "Prog1,Prog2")
    #[arg(long, env("ONCHAIN_PROGRAMS"), default_value = "")]
    onchain_programs: String,

    /// Page size for signature pagination (fetches ALL transactions)
    #[arg(short, long, default_value = "1000", env("LIMIT"))]
    limit: u64,

    /// Batch size for fetching transactions
    #[arg(short = 'b', long, default_value = "100", env("BATCH_SIZE"))]
    batch_size: usize,

    /// Delay between batches (milliseconds)
    #[arg(short = 'w', long, default_value = "100", env("BATCH_DELAY"))]
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
    info!(
        "Fetching all signatures (page size: {}) per program",
        cli.limit
    );
    info!("Batch size: {}", cli.batch_size);
    info!("Concurrency: {}", cli.concurrency);
    info!("Max retries: {}", cli.max_retries);

    // (a) Load file IDLs first (Decision 1: file primary)
    let mut idl_parser = IdlParser::new();
    load_idls(&mut idl_parser, &cli.idl_dir).await?;

    // RPC client constructed early — needed for the on-chain IDL fetch below.
    let rpc_client = Arc::new(RpcClient::new(cli.rpc_url.clone()));

    // (b) One-shot on-chain IDL fetch for explicitly-listed programs (backfill
    // is point-in-time, so no subscription task). File precedence is preserved
    // inside load_onchain_idls; per-program failures warn-and-continue.
    let onchain_programs = parse_onchain_programs(&cli.onchain_programs)?;
    if !onchain_programs.is_empty() {
        info!(
            "Fetching on-chain IDL(s) for {} program(s)...",
            onchain_programs.len()
        );
        soltrace_core::load_onchain_idls(&mut idl_parser, &rpc_client, &onchain_programs);
    }

    // (c) Prefix config from all loaded IDLs (file + on-chain)
    let loaded_idls = idl_parser.get_idls();
    info!("Loaded {} IDL(s) total", loaded_idls.len());
    for (addr, idl) in loaded_idls {
        info!("  - {}: {} events", addr, idl.events.len());
    }

    // Create program prefix configuration from CLI/env
    let mut prefix_config = ProgramPrefixConfig::new();
    // Load programs from IDLs with default prefix
    prefix_config.load_from_idls(loaded_idls);
    // Apply custom prefix mappings from CLI/env
    if !cli.program_prefixes.is_empty() {
        prefix_config.add_mappings_from_string(&cli.program_prefixes);
        info!(
            "Applied {} custom program prefix mapping(s)",
            cli.program_prefixes
        );
    }

    let mut program_ids = prefix_config.get_program_ids();
    // Drop programs with no IDL — without one, every event decodes to the
    // unknown-discriminator debug-skip, so fetching their signatures/txs is
    // wasted RPC. On-chain fetch failures land here too (backfill is
    // point-in-time, so a failed fetch means no IDL, ever).
    let dropped = soltrace_core::retain_indexable(&mut program_ids, loaded_idls);
    for pid in &dropped {
        warn!(
            "No IDL for program {}; skipping (install an IDL or drop it from --program-prefixes)",
            pid
        );
    }
    if program_ids.is_empty() {
        error!("No IDLs found in directory. Use --idl-dir <path>");
        return Ok(());
    }

    info!("Indexing {} program(s):", program_ids.len());
    for pid in &program_ids {
        let prefix = prefix_config.get_prefix(pid);
        info!("  - {} (prefix: {})", pid, prefix);
    }

    // (d) Wrap parser in ArcSwap + create event decoder
    let event_decoder = Arc::new(EventDecoder::new(
        Arc::new(soltrace_core::ArcSwap::from_pointee(idl_parser)),
        prefix_config,
    ));

    // Initialize database
    let db = create_backend(&cli.db_url).await?;
    info!("Database connected: {}", cli.db_url);

    // Track processed signatures across all programs
    let mut processed_signatures: HashSet<String> = HashSet::new();

    // Process each program
    let mut total_signatures_fetched = 0;
    let mut total_events_processed = 0;

    for program_id_str in &program_ids {
        info!("\nProcessing program: {}", program_id_str);

        // Validate and parse program ID
        let program_id = program_id_str
            .parse::<Pubkey>()
            .map_err(|e| anyhow::anyhow!("Invalid program ID {}: {}", program_id_str, e))?;

        // Check if program exists with retry
        let account = retry_with_rate_limit(
            || async { rpc_client.get_account(&program_id) },
            cli.max_retries,
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fetch account {}: {}", program_id_str, e))?;

        if account.owner == solana_sdk_ids::system_program::ID {
            warn!(
                "Program {} is not a program (owner is System Program)",
                program_id_str
            );
            continue;
        }

        // Get signatures for this program with pagination
        info!("Fetching all signatures for program {}...", program_id_str);

        use solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config;
        let mut all_signatures = Vec::new();
        let mut before: Option<solana_sdk::signature::Signature> = None;
        let page_size = cli.limit as usize;

        loop {
            let rpc = rpc_client.clone();
            let page = retry_with_rate_limit(
                || {
                    let rpc = rpc.clone();
                    async move {
                        let config = GetConfirmedSignaturesForAddress2Config {
                            before,
                            until: None,
                            limit: Some(page_size),
                            commitment: Some(CommitmentConfig::confirmed()),
                        };
                        rpc.get_signatures_for_address_with_config(&program_id, config)
                    }
                },
                cli.max_retries,
            )
            .await
            .map_err(|e| {
                anyhow::anyhow!("Failed to get signatures for {}: {}", program_id_str, e)
            })?;

            let page_len = page.len();
            info!(
                "Fetched page of {} signatures (total so far: {})",
                page_len,
                all_signatures.len() + page_len
            );

            if page_len == 0 {
                break;
            }

            if let Some(last) = page.last() {
                before = last
                    .signature
                    .parse::<solana_sdk::signature::Signature>()
                    .ok();
            }

            all_signatures.extend(page);

            if page_len < page_size {
                break;
            }

            tokio::time::sleep(Duration::from_millis(cli.batch_delay)).await;
        }

        let signatures_count = all_signatures.len();
        info!("Found {} total signatures", signatures_count);
        total_signatures_fetched += signatures_count;

        let signature_strings: Vec<String> = all_signatures
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
        )
        .await?;

        total_events_processed += program_events;
        info!(
            "Program {} complete: {} events processed",
            program_id_str, program_events
        );

        // Delay between programs to avoid rate limiting
        tokio::time::sleep(Duration::from_millis(cli.batch_delay)).await;
    }

    info!("\nBackfill complete!");
    info!("Total signatures fetched: {}", total_signatures_fetched);
    info!("Total events processed: {}", total_events_processed);
    info!(
        "Unique signatures processed: {}",
        processed_signatures.len()
    );

    Ok(())
}

/// Parse a comma-separated list of base58 program IDs into `Pubkey`s.
///
/// Empty/whitespace entries are dropped. Hard-errors on the first malformed
/// entry (Decision 11: operator typo must be loud, not silently dropped).
fn parse_onchain_programs(csv: &str) -> Result<Vec<Pubkey>> {
    csv.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<Pubkey>()
                .map_err(|e| anyhow::anyhow!("Invalid --onchain-programs entry '{s}': {e}"))
        })
        .collect()
}

async fn process_signatures_concurrent(
    rpc_client: Arc<RpcClient>,
    signatures: Vec<String>,
    program_id_str: String,
    event_decoder: Arc<EventDecoder>,
    db: Database,
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
                )
                .await
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
            info!(
                "Progress: {}/{} signatures processed, {} events found",
                processed_count, total, events_count
            );
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
    let sig = signature
        .parse::<solana_sdk::signature::Signature>()
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
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to fetch transaction: {}", e))?;

    // Process transaction
    match process_transaction(transaction, program_id_str, event_decoder, db).await {
        Ok(processed) => Ok(processed.len()),
        Err(e) => Err(anyhow::anyhow!("Failed to process transaction: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_program_parsing() {
        let programs = "Prog1,Prog2,Prog3";
        let parsed: Vec<String> = programs.split(',').map(|s| s.trim().to_string()).collect();

        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0], "Prog1");
    }

    #[test]
    fn test_parse_onchain_programs_valid() {
        let csv = "11111111111111111111111111111111,TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
        let parsed = parse_onchain_programs(csv).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed[0].to_string(),
            "11111111111111111111111111111111"
        );
    }

    #[test]
    fn test_parse_onchain_programs_empty_is_noop() {
        assert!(parse_onchain_programs("").unwrap().is_empty());
        assert!(parse_onchain_programs(" , , ").unwrap().is_empty());
    }

    #[test]
    fn test_parse_onchain_programs_invalid_hard_errors() {
        assert!(parse_onchain_programs("NOTABASE58").is_err());
        assert!(parse_onchain_programs(
            "11111111111111111111111111111111,BAD!!"
        )
        .is_err());
    }

    // --- Regression (soltrace-b4md): --onchain-programs is OPTIONAL with an
    // empty default → operators who don't pass it see zero behavioral change.

    #[test]
    fn test_cli_onchain_programs_defaults_empty_when_absent() {
        let cli = Cli::parse_from(["soltrace-backfill", "--program-prefixes", ""]);
        assert_eq!(cli.onchain_programs, "", "absent flag must default to empty");
    }

    #[test]
    fn test_cli_onchain_programs_accepted_when_present() {
        let cli = Cli::parse_from([
            "soltrace-backfill",
            "--program-prefixes",
            "",
            "--onchain-programs",
            "11111111111111111111111111111111",
        ]);
        assert_eq!(cli.onchain_programs, "11111111111111111111111111111111");
    }
}
