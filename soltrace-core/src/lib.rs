pub mod db;
pub mod error;
pub mod event;
pub mod idl;
pub mod idl_event;
pub mod onchain_idl;
pub mod queue;
pub mod retry;
pub mod types;
pub mod utils;

pub use db::{create_backend, Database, DatabaseBackend, EventRecord};
pub use error::{Result, SoltraceError};
pub use event::EventDecoder;
pub use idl::IdlParser;
pub use idl_event::IdlEventDecoder;
// Re-exported so binaries can build the shared Arc<ArcSwap<IdlParser>> handed to
// EventDecoder (and, later, the live subscription task) without a direct arc-swap dep.
pub use arc_swap::ArcSwap;
pub use onchain_idl::{
    decode_metadata_account, derive_canonical_idl_pda, fetch_canonical_idl, PROGRAM_METADATA_ID,
};
pub use queue::{EventQueue, QueueEvent};
#[cfg(feature = "kafka")]
pub use queue::kafka::{KafkaConfig, KafkaProducer};
pub use retry::retry_with_rate_limit;
pub use types::DecodedEvent;
pub use types::{EventDiscriminator, CpiEvent, InnerInstructionInfo, ProgramId, ProgramPrefixConfig, Slot};
pub use utils::{
    cpi_dedup_index, decode_cpi_events, extract_cpi_events, extract_event_from_log,
    extract_inner_instructions, load_idls, load_onchain_idls, process_transaction,
    retain_indexable, EVENT_CPI_DISCRIMINATOR,
};

// Re-export anchor_lang types for users who want to define their own events
pub use anchor_lang::Discriminator;
pub use anchor_lang::Event;
