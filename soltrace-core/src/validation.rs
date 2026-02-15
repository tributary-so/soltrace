use crate::error::{Result, SoltraceError};
use solana_sdk::pubkey::Pubkey;
use std::path::Path;

/// Validate a Solana program ID string
pub fn validate_program_id(program_id: &str) -> Result<Pubkey> {
    program_id.parse::<Pubkey>().map_err(|e| {
        SoltraceError::InvalidIdl(format!("Invalid program ID '{}': {}", program_id, e))
    })
}

/// Validate a list of program IDs
pub fn validate_program_ids(program_ids: &[String]) -> Result<Vec<Pubkey>> {
    program_ids
        .iter()
        .map(|id| validate_program_id(id))
        .collect()
}

/// Validate that a directory exists and is readable
pub fn validate_directory(path: &str) -> Result<()> {
    let path = Path::new(path);

    if !path.exists() {
        return Err(SoltraceError::InvalidIdl(format!(
            "Directory does not exist: {}",
            path.display()
        )));
    }

    if !path.is_dir() {
        return Err(SoltraceError::InvalidIdl(format!(
            "Path is not a directory: {}",
            path.display()
        )));
    }

    // Try to read the directory to check permissions
    std::fs::read_dir(path).map_err(|e| {
        SoltraceError::InvalidIdl(format!("Cannot read directory '{}': {}", path.display(), e))
    })?;

    Ok(())
}

/// Validate a database URL
pub fn validate_db_url(url: &str) -> Result<()> {
    if url.is_empty() {
        return Err(SoltraceError::InvalidIdl(
            "Database URL cannot be empty".to_string(),
        ));
    }

    // Check for valid SQLite URL
    if url.starts_with("sqlite:") {
        let path = url.strip_prefix("sqlite:").unwrap_or(url);

        // For SQLite, check that the parent directory exists
        if let Some(parent) = Path::new(path).parent() {
            if !parent.exists() {
                return Err(SoltraceError::InvalidIdl(format!(
                    "SQLite database directory does not exist: {}",
                    parent.display()
                )));
            }
        }
    }

    // PostgreSQL URLs are validated at connection time
    Ok(())
}

/// Validate an RPC URL
pub fn validate_rpc_url(url: &str) -> Result<()> {
    if url.is_empty() {
        return Err(SoltraceError::InvalidIdl(
            "RPC URL cannot be empty".to_string(),
        ));
    }

    // Basic URL validation
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(SoltraceError::InvalidIdl(format!(
            "Invalid RPC URL '{}': must start with http:// or https://",
            url
        )));
    }

    Ok(())
}

/// Validate a WebSocket URL
pub fn validate_ws_url(url: &str) -> Result<()> {
    if url.is_empty() {
        return Err(SoltraceError::InvalidIdl(
            "WebSocket URL cannot be empty".to_string(),
        ));
    }

    // Basic URL validation
    if !url.starts_with("ws://") && !url.starts_with("wss://") {
        return Err(SoltraceError::InvalidIdl(format!(
            "Invalid WebSocket URL '{}': must start with ws:// or wss://",
            url
        )));
    }

    Ok(())
}

/// Validate commitment level string
pub fn validate_commitment(commitment: &str) -> Result<()> {
    let valid = ["processed", "confirmed", "finalized"];

    if !valid.contains(&commitment.to_lowercase().as_str()) {
        return Err(SoltraceError::InvalidIdl(format!(
            "Invalid commitment level '{}': must be one of {:?}",
            commitment, valid
        )));
    }

    Ok(())
}

/// Configuration validator for backfill
pub struct BackfillConfig {
    pub rpc_url: String,
    pub programs: Vec<String>,
    pub db_url: String,
    pub idl_dir: String,
    pub limit: u64,
    pub batch_size: usize,
    pub batch_delay: u64,
    pub concurrency: usize,
    pub max_retries: u32,
}

impl BackfillConfig {
    /// Validate all configuration fields
    pub fn validate(&self) -> Result<()> {
        // Validate RPC URL
        validate_rpc_url(&self.rpc_url)?;

        // Validate program IDs
        if self.programs.is_empty() {
            return Err(SoltraceError::InvalidIdl(
                "At least one program ID must be specified".to_string(),
            ));
        }

        for program_id in &self.programs {
            validate_program_id(program_id)?;
        }

        // Validate database URL
        validate_db_url(&self.db_url)?;

        // Validate IDL directory
        validate_directory(&self.idl_dir)?;

        // Validate numeric parameters
        if self.limit == 0 {
            return Err(SoltraceError::InvalidIdl(
                "Limit must be greater than 0".to_string(),
            ));
        }

        if self.batch_size == 0 {
            return Err(SoltraceError::InvalidIdl(
                "Batch size must be greater than 0".to_string(),
            ));
        }

        if self.concurrency == 0 {
            return Err(SoltraceError::InvalidIdl(
                "Concurrency must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

/// Configuration validator for live indexer
pub struct LiveConfig {
    pub ws_url: String,
    pub rpc_url: String,
    pub programs: Vec<String>,
    pub db_url: String,
    pub idl_dir: String,
    pub commitment: String,
    pub reconnect_delay: u64,
    pub max_reconnects: u32,
}

impl LiveConfig {
    /// Validate all configuration fields
    pub fn validate(&self) -> Result<()> {
        // Validate WebSocket URL
        validate_ws_url(&self.ws_url)?;

        // Validate RPC URL
        validate_rpc_url(&self.rpc_url)?;

        // Validate program IDs
        if self.programs.is_empty() {
            return Err(SoltraceError::InvalidIdl(
                "At least one program ID must be specified".to_string(),
            ));
        }

        for program_id in &self.programs {
            validate_program_id(program_id)?;
        }

        // Validate database URL
        validate_db_url(&self.db_url)?;

        // Validate IDL directory
        validate_directory(&self.idl_dir)?;

        // Validate commitment
        validate_commitment(&self.commitment)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_program_id_valid() {
        let valid_id = "11111111111111111111111111111111";
        assert!(validate_program_id(valid_id).is_ok());
    }

    #[test]
    fn test_validate_program_id_invalid() {
        let invalid_id = "not-a-valid-pubkey";
        assert!(validate_program_id(invalid_id).is_err());
    }

    #[test]
    fn test_validate_rpc_url_valid() {
        assert!(validate_rpc_url("https://api.mainnet-beta.solana.com").is_ok());
        assert!(validate_rpc_url("http://localhost:8899").is_ok());
    }

    #[test]
    fn test_validate_rpc_url_invalid() {
        assert!(validate_rpc_url("").is_err());
        assert!(validate_rpc_url("ftp://example.com").is_err());
    }

    #[test]
    fn test_validate_ws_url_valid() {
        assert!(validate_ws_url("wss://api.mainnet-beta.solana.com").is_ok());
        assert!(validate_ws_url("ws://localhost:8900").is_ok());
    }

    #[test]
    fn test_validate_ws_url_invalid() {
        assert!(validate_ws_url("").is_err());
        assert!(validate_ws_url("http://example.com").is_err());
    }

    #[test]
    fn test_validate_commitment() {
        assert!(validate_commitment("confirmed").is_ok());
        assert!(validate_commitment("processed").is_ok());
        assert!(validate_commitment("finalized").is_ok());
        assert!(validate_commitment("invalid").is_err());
    }
}
