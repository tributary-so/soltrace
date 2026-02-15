use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info};

/// Metrics for tracking indexer performance
#[derive(Debug)]
pub struct Metrics {
    /// Total number of events processed
    pub events_total: AtomicU64,
    /// Number of events by program ID
    pub events_by_program: Arc<tokio::sync::RwLock<HashMap<String, u64>>>,
    /// Number of events by event type
    pub events_by_type: Arc<tokio::sync::RwLock<HashMap<String, u64>>>,
    /// Number of transactions processed
    pub transactions_total: AtomicU64,
    /// Number of failed transactions
    pub transactions_failed: AtomicU64,
    /// Number of WebSocket reconnections
    pub ws_reconnections: AtomicU64,
    /// Number of RPC calls made
    pub rpc_calls: AtomicU64,
    /// Number of RPC call failures
    pub rpc_failures: AtomicU64,
    /// Processing start time
    pub start_time: Instant,
    /// Number of database insertions
    pub db_inserts: AtomicU64,
    /// Number of database insert failures
    pub db_insert_failures: AtomicU64,
    /// Number of duplicate events detected
    pub duplicate_events: AtomicU64,
    /// Number of events that failed to decode
    pub decode_failures: AtomicU64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            events_total: AtomicU64::new(0),
            events_by_program: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            events_by_type: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            transactions_total: AtomicU64::new(0),
            transactions_failed: AtomicU64::new(0),
            ws_reconnections: AtomicU64::new(0),
            rpc_calls: AtomicU64::new(0),
            rpc_failures: AtomicU64::new(0),
            start_time: Instant::now(),
            db_inserts: AtomicU64::new(0),
            db_insert_failures: AtomicU64::new(0),
            duplicate_events: AtomicU64::new(0),
            decode_failures: AtomicU64::new(0),
        }
    }

    /// Record a processed event
    pub fn record_event(&self, program_id: &str, event_type: &str) {
        self.events_total.fetch_add(1, Ordering::Relaxed);
        
        // Update program counter
        let program_id = program_id.to_string();
        let events_by_program = self.events_by_program.clone();
        tokio::spawn(async move {
            let mut map = events_by_program.write().await;
            *map.entry(program_id).or_insert(0) += 1;
        });
        
        // Update event type counter
        let event_type = event_type.to_string();
        let events_by_type = self.events_by_type.clone();
        tokio::spawn(async move {
            let mut map = events_by_type.write().await;
            *map.entry(event_type).or_insert(0) += 1;
        });
    }

    /// Record a transaction
    pub fn record_transaction(&self, failed: bool) {
        self.transactions_total.fetch_add(1, Ordering::Relaxed);
        if failed {
            self.transactions_failed.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a WebSocket reconnection
    pub fn record_ws_reconnection(&self) {
        self.ws_reconnections.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an RPC call
    pub fn record_rpc_call(&self, failed: bool) {
        self.rpc_calls.fetch_add(1, Ordering::Relaxed);
        if failed {
            self.rpc_failures.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a database insert
    pub fn record_db_insert(&self, failed: bool, duplicate: bool) {
        if failed {
            if duplicate {
                self.duplicate_events.fetch_add(1, Ordering::Relaxed);
            } else {
                self.db_insert_failures.fetch_add(1, Ordering::Relaxed);
            }
        } else {
            self.db_inserts.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a decode failure
    pub fn record_decode_failure(&self) {
        self.decode_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Get events per second
    pub fn events_per_second(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.events_total.load(Ordering::Relaxed) as f64 / elapsed
        } else {
            0.0
        }
    }

    /// Get total uptime in seconds
    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Get a snapshot of current metrics
    pub async fn snapshot(&self) -> MetricsSnapshot {
        let events_by_program = self.events_by_program.read().await.clone();
        let events_by_type = self.events_by_type.read().await.clone();
        
        MetricsSnapshot {
            events_total: self.events_total.load(Ordering::Relaxed),
            events_by_program,
            events_by_type,
            transactions_total: self.transactions_total.load(Ordering::Relaxed),
            transactions_failed: self.transactions_failed.load(Ordering::Relaxed),
            ws_reconnections: self.ws_reconnections.load(Ordering::Relaxed),
            rpc_calls: self.rpc_calls.load(Ordering::Relaxed),
            rpc_failures: self.rpc_failures.load(Ordering::Relaxed),
            uptime_seconds: self.uptime_seconds(),
            events_per_second: self.events_per_second(),
            db_inserts: self.db_inserts.load(Ordering::Relaxed),
            db_insert_failures: self.db_insert_failures.load(Ordering::Relaxed),
            duplicate_events: self.duplicate_events.load(Ordering::Relaxed),
            decode_failures: self.decode_failures.load(Ordering::Relaxed),
        }
    }

    /// Log current metrics summary
    pub async fn log_summary(&self) {
        let snapshot = self.snapshot().await;
        info!(
            "Metrics Summary: {} events total ({:.2} events/sec), {} transactions, {} reconnections, {} RPC calls ({} failed)",
            snapshot.events_total,
            snapshot.events_per_second,
            snapshot.transactions_total,
            snapshot.ws_reconnections,
            snapshot.rpc_calls,
            snapshot.rpc_failures
        );
        debug!("Events by program: {:?}", snapshot.events_by_program);
        debug!("Events by type: {:?}", snapshot.events_by_type);
    }
}

/// Snapshot of metrics at a point in time
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub events_total: u64,
    pub events_by_program: HashMap<String, u64>,
    pub events_by_type: HashMap<String, u64>,
    pub transactions_total: u64,
    pub transactions_failed: u64,
    pub ws_reconnections: u64,
    pub rpc_calls: u64,
    pub rpc_failures: u64,
    pub uptime_seconds: u64,
    pub events_per_second: f64,
    pub db_inserts: u64,
    pub db_insert_failures: u64,
    pub duplicate_events: u64,
    pub decode_failures: u64,
}

impl MetricsSnapshot {
    /// Export as JSON
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "events_total": self.events_total,
            "events_by_program": self.events_by_program,
            "events_by_type": self.events_by_type,
            "transactions_total": self.transactions_total,
            "transactions_failed": self.transactions_failed,
            "ws_reconnections": self.ws_reconnections,
            "rpc_calls": self.rpc_calls,
            "rpc_failures": self.rpc_failures,
            "uptime_seconds": self.uptime_seconds,
            "events_per_second": self.events_per_second,
            "db_inserts": self.db_inserts,
            "db_insert_failures": self.db_insert_failures,
            "duplicate_events": self.duplicate_events,
            "decode_failures": self.decode_failures,
        })
    }
}

/// Health check status
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
        }
    }
}

/// Health check for the indexer
pub struct HealthCheck {
    metrics: Arc<Metrics>,
    max_reconnections: u64,
    max_failure_rate: f64,
}

impl HealthCheck {
    pub fn new(metrics: Arc<Metrics>) -> Self {
        Self {
            metrics,
            max_reconnections: 10,
            max_failure_rate: 0.5, // 50% failure rate
        }
    }

    /// Configure max reconnections before marking as degraded
    pub fn with_max_reconnections(mut self, max: u64) -> Self {
        self.max_reconnections = max;
        self
    }

    /// Configure max failure rate before marking as degraded
    pub fn with_max_failure_rate(mut self, rate: f64) -> Self {
        self.max_failure_rate = rate;
        self
    }

    /// Check current health status
    pub fn check(&self) -> HealthStatus {
        let reconnections = self.metrics.ws_reconnections.load(Ordering::Relaxed);
        let rpc_calls = self.metrics.rpc_calls.load(Ordering::Relaxed);
        let rpc_failures = self.metrics.rpc_failures.load(Ordering::Relaxed);
        
        // If too many reconnections, mark as unhealthy
        if reconnections > self.max_reconnections * 2 {
            return HealthStatus::Unhealthy;
        }
        
        // Check RPC failure rate
        if rpc_calls > 0 {
            let failure_rate = rpc_failures as f64 / rpc_calls as f64;
            if failure_rate > self.max_failure_rate {
                return HealthStatus::Degraded;
            }
        }
        
        // Check for high reconnection count
        if reconnections > self.max_reconnections {
            return HealthStatus::Degraded;
        }
        
        HealthStatus::Healthy
    }

    /// Get health check result with details
    pub async fn health_check(&self) -> HealthCheckResult {
        let status = self.check();
        let snapshot = self.metrics.snapshot().await;
        
        HealthCheckResult {
            status,
            metrics: snapshot,
            message: match status {
                HealthStatus::Healthy => "All systems operational".to_string(),
                HealthStatus::Degraded => "System performance degraded".to_string(),
                HealthStatus::Unhealthy => "System unhealthy".to_string(),
            },
        }
    }
}

/// Health check result with metrics
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub status: HealthStatus,
    pub metrics: MetricsSnapshot,
    pub message: String,
}

impl HealthCheckResult {
    /// Export as JSON
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "status": self.status.to_string(),
            "message": self.message,
            "metrics": self.metrics.to_json(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_new() {
        let metrics = Metrics::new();
        assert_eq!(metrics.events_total.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.uptime_seconds(), 0);
    }

    #[tokio::test]
    async fn test_metrics_record_event() {
        let metrics = Metrics::new();
        metrics.record_event("program1", "Transfer");
        assert_eq!(metrics.events_total.load(Ordering::Relaxed), 1);
        // Wait for the async hashmap update to complete
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    #[test]
    fn test_metrics_record_transaction() {
        let metrics = Metrics::new();
        metrics.record_transaction(false);
        metrics.record_transaction(true);
        assert_eq!(metrics.transactions_total.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.transactions_failed.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_health_check_healthy() {
        let metrics = Arc::new(Metrics::new());
        let health = HealthCheck::new(metrics);
        assert_eq!(health.check(), HealthStatus::Healthy);
    }

    #[test]
    fn test_health_check_degraded_reconnections() {
        let metrics = Arc::new(Metrics::new());
        metrics.ws_reconnections.store(15, Ordering::Relaxed);
        let health = HealthCheck::new(metrics).with_max_reconnections(10);
        assert_eq!(health.check(), HealthStatus::Degraded);
    }

    #[test]
    fn test_health_check_unhealthy() {
        let metrics = Arc::new(Metrics::new());
        metrics.ws_reconnections.store(25, Ordering::Relaxed);
        let health = HealthCheck::new(metrics).with_max_reconnections(10);
        assert_eq!(health.check(), HealthStatus::Unhealthy);
    }

    #[test]
    fn test_health_check_failure_rate() {
        let metrics = Arc::new(Metrics::new());
        metrics.rpc_calls.store(100, Ordering::Relaxed);
        metrics.rpc_failures.store(60, Ordering::Relaxed);
        let health = HealthCheck::new(metrics).with_max_failure_rate(0.5);
        assert_eq!(health.check(), HealthStatus::Degraded);
    }
}
