use thiserror::Error;

pub type Result<T> = std::result::Result<T, SoltraceError>;

#[derive(Error, Debug)]
pub enum SoltraceError {
    #[error("IDL parsing error: {0}")]
    IdlParse(String),

    #[error("Event decoding error: {0}")]
    EventDecode(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("SQLx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid IDL format: {0}")]
    InvalidIdl(String),

    #[error("Event not found in IDL: {0}")]
    EventNotFound(String),

    #[error("Discriminator mismatch")]
    DiscriminatorMismatch,

    #[error("Solana client error: {0}")]
    SolanaClient(String),
}
