use std::time::Duration;
use tokio::time::sleep;
use tracing::{warn, debug};

/// Retry an async operation with exponential backoff
/// 
/// # Arguments
/// * `operation` - The async operation to retry
/// * `max_retries` - Maximum number of retry attempts (0 = no retries)
/// * `base_delay` - Base delay between retries
/// * `max_delay` - Maximum delay between retries
/// 
/// # Returns
/// The result of the operation if successful, or the last error
pub async fn retry_with_backoff<T, E, F, Fut>(
    operation: F,
    max_retries: u32,
    base_delay: Duration,
    max_delay: Duration,
) -> Result<T, E>
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
                let error_str = e.to_string();
                last_error = Some(e);
                
                if attempt < max_retries {
                    // Calculate delay with exponential backoff
                    let delay = std::cmp::min(
                        base_delay * 2u32.pow(attempt),
                        max_delay,
                    );
                    
                    warn!(
                        "Operation failed (attempt {}/{}): {}. Retrying in {:?}...",
                        attempt + 1,
                        max_retries + 1,
                        error_str,
                        delay
                    );
                    
                    sleep(delay).await;
                }
            }
        }
    }
    
    Err(last_error.unwrap())
}

/// Retry an operation that might fail due to rate limiting
/// Automatically detects rate limit errors and uses longer delays
pub async fn retry_with_rate_limit<T, E, F, Fut>(
    operation: F,
    max_retries: u32,
) -> Result<T, E>
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

/// Process items concurrently with a limit on the number of concurrent operations
/// 
/// # Arguments
/// * `items` - The items to process
/// * `concurrency` - Maximum number of concurrent operations
/// * `processor` - The async function to process each item
pub async fn concurrent_process<T, R, E, F, Fut>(
    items: Vec<T>,
    concurrency: usize,
    processor: F,
) -> Vec<Result<R, E>>
where
    T: Send + 'static,
    R: Send + 'static,
    E: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<R, E>> + Send,
{
    use futures::stream::{self, StreamExt};
    
    stream::iter(items)
        .map(|item| {
            let processor = &processor;
            async move { processor(item).await }
        })
        .buffer_unordered(concurrency)
        .collect::<Vec<_>>()
        .await
}

/// Process items in batches with progress reporting
pub async fn process_batches<T, R, E, F, Fut>(
    items: Vec<T>,
    batch_size: usize,
    processor: F,
    on_batch_complete: impl Fn(usize, usize, usize), // (batch_num, total_batches, items_processed)
) -> Vec<Result<R, E>>
where
    T: Send + Clone + 'static,
    R: Send + 'static,
    E: Send + 'static,
    F: Fn(Vec<T>) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Vec<Result<R, E>>> + Send,
{
    let total_items = items.len();
    let total_batches = (total_items + batch_size - 1) / batch_size;
    let mut results = Vec::with_capacity(total_items);
    
    for (batch_num, batch) in items.chunks(batch_size).enumerate() {
        let batch_vec = batch.to_vec();
        let batch_results = processor(batch_vec).await;
        let batch_processed = batch_results.len();
        results.extend(batch_results);
        
        on_batch_complete(batch_num + 1, total_batches, batch_processed);
    }
    
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_retry_with_backoff_success() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let attempts = AtomicUsize::new(0);
        let result = retry_with_backoff(
            || async {
                let current = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                if current < 3 {
                    Err::<i32, &str>("not yet")
                } else {
                    Ok(42)
                }
            },
            5,
            Duration::from_millis(10),
            Duration::from_millis(100),
        ).await;
        
        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_failure() {
        let result = retry_with_backoff(
            || async { Err::<i32, &str>("always fails") },
            2,
            Duration::from_millis(10),
            Duration::from_millis(100),
        ).await;
        
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_concurrent_process() {
        let items: Vec<i32> = (0..10).collect();
        
        let results = concurrent_process(
            items,
            3,
            |item| async move { Ok::<i32, &str>(item * 2) },
        ).await;
        
        assert_eq!(results.len(), 10);
        let sum: i32 = results.into_iter().map(|r| r.unwrap()).sum();
        assert_eq!(sum, 90); // 0+2+4+6+8+10+12+14+16+18 = 90
    }
}
