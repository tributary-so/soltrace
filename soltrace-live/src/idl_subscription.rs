//! Live on-chain IDL subscription via program-metadata `accountSubscribe`.
//!
//! Dedicated WS connection (separate from `logs_subscribe`). For each program,
//! derives the canonical IDL PDA and subscribes. Push notifications decode the
//! `Metadata` account and hot-swap into the shared parser. Terminal states
//! (`SetImmutable` → unsubscribe, `Close` → drop + unsubscribe) are handled.
//! Reconnect loop mirrors `main.rs`'s capped-backoff pattern.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use futures::stream::SelectAll;
use solana_account_decoder_client_types::UiAccountEncoding;
use solana_client::rpc_config::RpcAccountInfoConfig;
use solana_commitment_config::CommitmentConfig;
use solana_pubsub_client::nonblocking::pubsub_client::PubsubClient;
use solana_sdk::pubkey::Pubkey;
use soltrace_core::{ArcSwap, IdlParser, decode_metadata_account, derive_canonical_idl_pda};
use spl_program_metadata_client::accounts::Metadata;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{error, info, warn};

/// What the subscription loop does after processing one push.
#[derive(Debug, PartialEq, Eq)]
enum SubscriptionAction {
    Keep,
    Unsubscribe,
}

/// Pure, testable handler for one `accountSubscribe` push.
///
/// `account_data` is raw `Metadata` bytes (`None`/empty = account closed). On
/// successful decode: clones the current parser, inserts the IDL, atomically
/// swaps. On `Ok(None)` (non-Direct/wrong program) or `Err` (decode failure):
/// warns and retains last-known-good — does NOT swap. Returns `Unsubscribe`
/// when the account was closed or is now immutable.
fn handle_account_notification(
    program: &Pubkey,
    account_data: Option<&[u8]>,
    shared_parser: &Arc<ArcSwap<IdlParser>>,
) -> SubscriptionAction {
    let bytes = match account_data.filter(|b| !b.is_empty()) {
        Some(b) => b,
        None => {
            let mut fresh = (**shared_parser.load()).clone();
            fresh.remove(&program.to_string());
            shared_parser.store(Arc::new(fresh));
            warn!(program = %program, "on-chain IDL account closed; removed from parser");
            return SubscriptionAction::Unsubscribe;
        }
    };

    match decode_metadata_account(bytes, program) {
        Ok(Some(idl)) => {
            let address = idl.address.clone();
            let mut fresh = (**shared_parser.load()).clone();
            fresh.insert_or_replace(idl);
            shared_parser.store(Arc::new(fresh));
            info!(program = %program, %address, "on-chain IDL hot-swapped");

            // ponytail: double-parse Metadata to read `mutable`. Alternative is
            // extending decode_metadata_account's signature (Epic 1's delivered
            // API). One extra borsh deserialise of a ~90-byte header is free.
            if let Ok(meta) = Metadata::from_bytes(bytes) {
                if !meta.mutable {
                    info!(program = %program, "IDL frozen (SetImmutable); unsubscribing");
                    return SubscriptionAction::Unsubscribe;
                }
            }
            SubscriptionAction::Keep
        }
        Ok(None) => {
            warn!(program = %program, "on-chain IDL push skipped (non-Direct or wrong program)");
            SubscriptionAction::Keep
        }
        Err(e) => {
            warn!(program = %program, error = %e, "on-chain IDL decode failed; retaining last-known-good");
            SubscriptionAction::Keep
        }
    }
}

/// Spawn the dedicated IDL subscription task.
///
/// Owns its own WS connection (does NOT share `logs_subscribe`'s). On each
/// reconnect, re-subscribes to all program PDAs. When all PDAs become immutable
/// or are closed, the task exits cleanly. Task errors log and return without
/// taking down the indexer.
pub fn spawn_idl_subscription_task(
    programs: Vec<Pubkey>,
    ws_url: String,
    commitment: CommitmentConfig,
    shared_parser: Arc<ArcSwap<IdlParser>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut attempts: u32 = 0;
        loop {
            match run_subscription_session(&programs, &ws_url, commitment, &shared_parser).await {
                Ok(true) => {
                    attempts += 1;
                    let secs = backoff_delay(attempts);
                    info!("IDL subscription session ended; reconnecting in {secs}s");
                    sleep(Duration::from_secs(secs)).await;
                }
                Ok(false) => {
                    info!("all IDL subscriptions exhausted (immutable/closed); stopping task");
                    break;
                }
                Err(e) => {
                    attempts += 1;
                    let secs = backoff_delay(attempts);
                    error!("IDL subscription error: {e}; reconnecting in {secs}s");
                    sleep(Duration::from_secs(secs)).await;
                }
            }
        }
    })
}

fn backoff_delay(attempts: u32) -> u64 {
    if attempts > 10 {
        60
    } else {
        attempts.max(1) as u64
    }
}

/// One WS session: connect, subscribe to all PDAs, process until disconnect.
///
/// Returns `Ok(true)` when streams ended unexpectedly (reconnect), `Ok(false)`
/// when every subscription was intentionally exhausted (done), `Err` on
/// connect failure.
async fn run_subscription_session(
    programs: &[Pubkey],
    ws_url: &str,
    commitment: CommitmentConfig,
    shared_parser: &Arc<ArcSwap<IdlParser>>,
) -> Result<bool, String> {
    let pubsub = PubsubClient::new(ws_url)
        .await
        .map_err(|e| format!("WS connect failed: {e}"))?;

    let config = RpcAccountInfoConfig {
        encoding: Some(UiAccountEncoding::Base64),
        commitment: Some(commitment),
        data_slice: None,
        min_context_slot: None,
    };

    let mut select: SelectAll<_> = SelectAll::new();
    let mut unsubs = HashMap::new();
    let mut active = 0usize;

    for program in programs {
        let pda = derive_canonical_idl_pda(program);
        match pubsub.account_subscribe(&pda, Some(config.clone())).await {
            Ok((stream, unsub)) => {
                let prog = *program;
                select.push(stream.map(move |resp| (prog, resp)));
                unsubs.insert(*program, unsub);
                active += 1;
                info!(%program, pda = %pda, "subscribed to on-chain IDL account");
            }
            Err(e) => {
                warn!(%program, pda = %pda, "IDL account_subscribe failed: {e}");
            }
        }
    }

    if active == 0 {
        return Err("no IDL subscriptions established".into());
    }

    while let Some((program, resp)) = select.next().await {
        // lamports=0 ⇒ account closed; don't even try to decode.
        let data = if resp.value.lamports == 0 {
            None
        } else {
            resp.value.data.decode()
        };
        if handle_account_notification(&program, data.as_deref(), shared_parser)
            == SubscriptionAction::Unsubscribe
        {
            if let Some(unsub) = unsubs.remove(&program) {
                unsub().await;
                active -= 1;
            }
        }
    }

    // All streams ended. If every subscription was intentionally unsubscribed
    // → task is done. Otherwise streams dropped (disconnect) → reconnect.
    // ponytail: on reconnect we re-subscribe to ALL programs, including ones
    // that were immutable/closed. They get one immediate push then re-unsubscribe.
    // One wasted round-trip per dead PDA per reconnect — cheaper than tracking
    // per-program terminal state across sessions.
    Ok(active > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::pubkey;

    /// Synthesise a `Metadata` account blob matching the borsh layout.
    /// Mirrors onchain_idl.rs's test helper (Compression::None, Encoding::Utf8).
    fn build_idl_account(
        program: &Pubkey,
        mutable: bool,
        data_source_byte: u8,
        idl_json: &[u8],
    ) -> Vec<u8> {
        let mut seed = [0u8; 16];
        seed[..3].copy_from_slice(b"idl");
        let mut buf = Vec::new();
        buf.push(2u8); // AccountDiscriminator::Metadata
        buf.extend_from_slice(program.as_ref()); // program (32)
        buf.extend_from_slice(&[0u8; 32]); // authority (None = zeros)
        buf.push(if mutable { 1 } else { 0 });
        buf.push(1); // canonical
        buf.extend_from_slice(&seed); // seed (16)
        buf.push(1u8); // encoding = Utf8
        buf.push(0u8); // compression = None
        buf.push(1u8); // format = Json
        buf.push(data_source_byte);
        buf.extend_from_slice(&(idl_json.len() as u32).to_le_bytes());
        buf.extend_from_slice(idl_json);
        buf
    }

    const PROGRAM: Pubkey = pubkey!("TRibg8W8zmPHQqWtyAD1rEBRXEdyU13Mu6qX1Sg42tJ");
    const PROGRAM_STR: &str = "TRibg8W8zmPHQqWtyAD1rEBRXEdyU13Mu6qX1Sg42tJ";

    fn idl_json() -> Vec<u8> {
        format!(
            r#"{{"name":"TestIDL","events":[],"address":"{}"}}"#,
            PROGRAM_STR
        )
        .into_bytes()
    }

    fn empty_parser() -> Arc<ArcSwap<IdlParser>> {
        Arc::new(ArcSwap::from_pointee(IdlParser::new()))
    }

    // ── SetData: new IDL swapped in ──────────────────────────────────────

    #[test]
    fn setdata_push_swaps_in_new_idl() {
        let parser = empty_parser();
        let blob = build_idl_account(&PROGRAM, true, 0, &idl_json());

        let action = handle_account_notification(&PROGRAM, Some(&blob), &parser);

        assert_eq!(action, SubscriptionAction::Keep);
        let loaded = parser.load();
        let idl = loaded.get_idls().get(PROGRAM_STR).expect("IDL present");
        assert_eq!(idl.name.as_deref(), Some("TestIDL"));
    }

    // ── Close: program removed, returns Unsubscribe ─────────────────────

    #[test]
    fn close_push_removes_idl_and_unsubscribes() {
        let parser = empty_parser();
        // Seed with an IDL first.
        handle_account_notification(
            &PROGRAM,
            Some(&build_idl_account(&PROGRAM, true, 0, &idl_json())),
            &parser,
        );
        assert!(parser.load().get_idls().contains_key(PROGRAM_STR));

        let action = handle_account_notification(&PROGRAM, None, &parser);

        assert_eq!(action, SubscriptionAction::Unsubscribe);
        assert!(!parser.load().get_idls().contains_key(PROGRAM_STR));
    }

    #[test]
    fn empty_data_treated_as_close() {
        let parser = empty_parser();
        let action = handle_account_notification(&PROGRAM, Some(&[]), &parser);
        assert_eq!(action, SubscriptionAction::Unsubscribe);
    }

    // ── Garbage: last-known-good retained ───────────────────────────────

    #[test]
    fn garbage_decode_retains_last_known_good() {
        let parser = empty_parser();
        // Seed with a valid IDL.
        handle_account_notification(
            &PROGRAM,
            Some(&build_idl_account(&PROGRAM, true, 0, &idl_json())),
            &parser,
        );

        // Push garbage that looks like a Metadata account but has corrupt data.
        let garbage = build_idl_account(&PROGRAM, true, 0, b"not valid json");
        let action = handle_account_notification(&PROGRAM, Some(&garbage), &parser);

        assert_eq!(action, SubscriptionAction::Keep);
        // Original IDL is still there.
        let loaded = parser.load();
        let idl = loaded
            .get_idls()
            .get(PROGRAM_STR)
            .expect("original retained");
        assert_eq!(idl.name.as_deref(), Some("TestIDL"));
    }

    // ── Non-Direct data_source: no swap, returns Keep ───────────────────

    #[test]
    fn non_direct_data_source_returns_keep_no_swap() {
        let parser = empty_parser();
        // data_source_byte = 1 (Url)
        let blob = build_idl_account(&PROGRAM, true, 1, &idl_json());

        let action = handle_account_notification(&PROGRAM, Some(&blob), &parser);

        assert_eq!(action, SubscriptionAction::Keep);
        assert!(parser.load().get_idls().is_empty());
    }

    // ── SetImmutable: IDL swapped, returns Unsubscribe ──────────────────

    #[test]
    fn setimmutable_swaps_and_returns_unsubscribe() {
        let parser = empty_parser();
        let blob = build_idl_account(&PROGRAM, false, 0, &idl_json());

        let action = handle_account_notification(&PROGRAM, Some(&blob), &parser);

        assert_eq!(action, SubscriptionAction::Unsubscribe);
        // IDL IS swapped in (we decode before checking mutable).
        assert!(parser.load().get_idls().contains_key(PROGRAM_STR));
    }

    // ── Wrong program: no swap ──────────────────────────────────────────

    #[test]
    fn wrong_program_returns_keep_no_swap() {
        let parser = empty_parser();
        let other = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
        let blob = build_idl_account(&PROGRAM, true, 0, &idl_json());

        let action = handle_account_notification(&other, Some(&blob), &parser);

        assert_eq!(action, SubscriptionAction::Keep);
        assert!(parser.load().get_idls().is_empty());
    }

    // ── Backoff helper ──────────────────────────────────────────────────

    #[test]
    fn backoff_capped_at_60s() {
        assert_eq!(backoff_delay(1), 1);
        assert_eq!(backoff_delay(5), 5);
        assert_eq!(backoff_delay(10), 10);
        assert_eq!(backoff_delay(11), 60);
        assert_eq!(backoff_delay(100), 60);
    }

    // ── Arc-swap visibility: concurrent writer + reader never tears ─────
    //
    // Mirrors event.rs::arcswap_load_never_tears_under_concurrent_swap.
    // A writer hammers handle_account_notification (full clone+store per push)
    // while a reader loads the parser and inspects IDL fields. The reader must
    // never observe a torn/partial state — always a coherent old or new revision.

    #[test]
    fn concurrent_swap_and_read_never_tears() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let parser = empty_parser();
        let stop = Arc::new(AtomicBool::new(false));
        let mut handles = vec![];

        // Writer
        {
            let parser = parser.clone();
            let stop = stop.clone();
            handles.push(std::thread::spawn(move || {
                let mut i = 0u32;
                while !stop.load(Ordering::Relaxed) {
                    let idl = format!(
                        r#"{{"name":"V{}","events":[],"address":"{}"}}"#,
                        i % 200,
                        PROGRAM_STR
                    );
                    let blob = build_idl_account(&PROGRAM, true, 0, idl.as_bytes());
                    handle_account_notification(&PROGRAM, Some(&blob), &parser);
                    i += 1;
                }
            }));
        }

        // Readers
        for _ in 0..3 {
            let parser = parser.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..5_000 {
                    let loaded = parser.load();
                    if let Some(idl) = loaded.get_idls().get(PROGRAM_STR) {
                        let name = idl.name.as_ref().expect("name present if IDL exists");
                        assert!(name.starts_with('V'), "torn read: name={name}");
                    }
                    // Empty (pre-first-write) is also valid — just not partial.
                }
            }));
        }

        std::thread::sleep(std::time::Duration::from_millis(150));
        stop.store(true, Ordering::Relaxed);
        for h in handles {
            h.join().expect("thread panicked");
        }

        assert!(parser.load().get_idls().contains_key(PROGRAM_STR));
    }

    // ── Multi-program: independent swaps ───────────────────────────────

    #[test]
    fn multiple_programs_swap_independently() {
        let parser = empty_parser();
        let prog_b = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
        let prog_b_str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

        let idl_a = idl_json();
        let idl_b: Vec<u8> =
            format!(r#"{{"name":"IDB","events":[],"address":"{}"}}"#, prog_b_str).into_bytes();

        let blob_a = build_idl_account(&PROGRAM, true, 0, &idl_a);
        let blob_b = build_idl_account(&prog_b, true, 0, &idl_b);

        handle_account_notification(&PROGRAM, Some(&blob_a), &parser);
        handle_account_notification(&prog_b, Some(&blob_b), &parser);

        let loaded = parser.load();
        assert_eq!(loaded.get_idls().len(), 2);
        assert_eq!(
            loaded.get_idls().get(PROGRAM_STR).unwrap().name.as_deref(),
            Some("TestIDL")
        );
        assert_eq!(
            loaded.get_idls().get(prog_b_str).unwrap().name.as_deref(),
            Some("IDB")
        );
    }

    // ── Close one program leaves others intact ─────────────────────────

    #[test]
    fn close_one_program_leaves_others_intact() {
        let parser = empty_parser();
        let prog_b = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
        let prog_b_str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

        // Seed both
        handle_account_notification(
            &PROGRAM,
            Some(&build_idl_account(&PROGRAM, true, 0, &idl_json())),
            &parser,
        );
        handle_account_notification(
            &prog_b,
            Some(&build_idl_account(
                &prog_b,
                true,
                0,
                format!(r#"{{"name":"B","events":[],"address":"{}"}}"#, prog_b_str).as_bytes(),
            )),
            &parser,
        );

        // Close A
        handle_account_notification(&PROGRAM, None, &parser);

        let loaded = parser.load();
        assert!(!loaded.get_idls().contains_key(PROGRAM_STR));
        assert!(loaded.get_idls().contains_key(prog_b_str));
    }

    // ── WS connection failure → Err (reconnect path trigger) ───────────
    //
    // ponytail: port 1 gives instant connection-refused — exercises the
    // PubsubClient::new Err → Err("WS connect failed") path without a mock.
    // The full reconnect-loop re-subscribe behaviour (all PDAs re-subscribed on
    // reconnect) requires a mock WS server; per the bean's skip clause it is
    // verified manually (localnet demo). The backoff_delay test covers the
    // timing; this test covers the error propagation.

    #[tokio::test]
    async fn session_returns_err_when_ws_unreachable() {
        let parser = empty_parser();
        let result = run_subscription_session(
            &[PROGRAM],
            "ws://127.0.0.1:1",
            CommitmentConfig::confirmed(),
            &parser,
        )
        .await;
        assert!(
            result.is_err(),
            "dead endpoint should return Err, got {result:?}"
        );
        // Parser untouched.
        assert!(parser.load().get_idls().is_empty());
    }
}
