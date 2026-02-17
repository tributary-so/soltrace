pub mod db;
pub mod error;
pub mod event;
pub mod idl;
pub mod idl_event;
pub mod metrics;
pub mod retry;
pub mod types;
pub mod utils;
pub mod validation;

pub use db::{Database, DatabaseBackend, EventRecord};
pub use error::{Result, SoltraceError};
pub use event::EventDecoder;
pub use idl::IdlParser;
pub use idl_event::IdlEventDecoder;
pub use metrics::{HealthCheck, HealthStatus, Metrics, MetricsSnapshot};
pub use retry::{concurrent_process, process_batches, retry_with_backoff, retry_with_rate_limit};
pub use types::DecodedEvent;
pub use types::{EventDiscriminator, ProgramId, Slot};
pub use utils::{extract_event_from_log, load_idls, process_transaction};
pub use validation::{
    validate_program_id, validate_program_ids, validate_rpc_url, validate_ws_url,
};

// Re-export anchor_lang types for users who want to define their own events
pub use anchor_lang::Discriminator;
pub use anchor_lang::Event;
