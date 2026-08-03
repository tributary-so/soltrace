use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

/// Retry an operation that might fail due to rate limiting
/// Automatically detects rate limit errors and uses longer delays
pub async fn retry_with_rate_limit<T, E, F, Fut>(operation: F, max_retries: u32) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut last_error = None;

    for attempt in 0..=max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                let error_str = e.to_string().to_lowercase();
                last_error = Some(e);

                if attempt < max_retries {
                    // Check if it's a rate limit error
                    let is_rate_limit = error_str.contains("rate limit")
                        || error_str.contains("429")
                        || error_str.contains("too many requests");

                    let delay = if is_rate_limit {
                        // Longer delay for rate limits
                        Duration::from_secs((attempt + 1) as u64 * 5)
                    } else {
                        // Standard exponential backoff
                        Duration::from_millis(100 * 2u64.pow(attempt))
                    };

                    let delay = std::cmp::min(delay, Duration::from_secs(60));

                    if is_rate_limit {
                        warn!(
                            "Rate limit hit (attempt {}/{}). Waiting {:?}...",
                            attempt + 1,
                            max_retries + 1,
                            delay
                        );
                    } else {
                        debug!(
                            "Operation failed (attempt {}/{}). Retrying in {:?}...",
                            attempt + 1,
                            max_retries + 1,
                            delay
                        );
                    }

                    sleep(delay).await;
                }
            }
        }
    }

    Err(last_error.unwrap())
}
