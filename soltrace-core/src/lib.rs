pub mod idl;
pub mod event;
pub mod error;
pub mod db;
pub mod types;
pub mod utils;
pub mod retry;
pub mod validation;
pub mod metrics;
pub mod borsh_decoder;

pub use idl::IdlParser;
pub use event::EventDecoder;
pub use types::DecodedEvent;
pub use error::{Result, SoltraceError};
pub use db::{Database, EventRecord};
pub use types::{Slot, ProgramId, EventDiscriminator};
pub use utils::{load_idls, process_transaction, extract_event_from_log};
pub use retry::{retry_with_backoff, retry_with_rate_limit, concurrent_process, process_batches};
pub use validation::{validate_program_id, validate_program_ids, validate_rpc_url, validate_ws_url};
pub use metrics::{Metrics, MetricsSnapshot, HealthCheck, HealthStatus};
pub use borsh_decoder::BorshDecoder;

// Re-export anchor_lang types for users who want to define their own events
pub use anchor_lang::Event;
pub use anchor_lang::Discriminator;
