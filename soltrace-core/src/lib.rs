pub mod db;
pub mod error;
pub mod event;
pub mod idl;
pub mod idl_event;
pub mod queue;
pub mod retry;
pub mod types;
pub mod utils;

pub use db::{create_backend, Database, DatabaseBackend, EventRecord};
pub use error::{Result, SoltraceError};
pub use event::EventDecoder;
pub use idl::IdlParser;
pub use idl_event::IdlEventDecoder;
pub use queue::{EventQueue, QueueEvent};
#[cfg(feature = "kafka")]
pub use queue::kafka::{KafkaConfig, KafkaProducer};
pub use retry::retry_with_rate_limit;
pub use types::DecodedEvent;
pub use types::{EventDiscriminator, CpiEvent, InnerInstructionInfo, ProgramId, ProgramPrefixConfig, Slot};
pub use utils::{
    cpi_dedup_index, decode_cpi_events, extract_cpi_events, extract_event_from_log,
    extract_inner_instructions, load_idls, process_transaction, EVENT_CPI_DISCRIMINATOR,
};

// Re-export anchor_lang types for users who want to define their own events
pub use anchor_lang::Discriminator;
pub use anchor_lang::Event;
