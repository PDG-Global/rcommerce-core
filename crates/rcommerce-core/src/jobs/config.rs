//! Job processing configuration

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Main job processing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobConfig {
    /// Enable job processing
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Worker configuration
    #[serde(default)]
    pub worker: WorkerConfig,

    /// Queue configuration
    #[serde(default)]
    pub queue: QueueConfig,

    /// Scheduler configuration
    #[serde(default)]
    pub scheduler: SchedulerConfig,

    /// Retry configuration
    #[serde(default)]
    pub retry: RetryConfig,

    /// Metrics configuration
    #[serde(default)]
    pub metrics: MetricsConfig,

    /// Dead letter queue configuration
    #[serde(default)]
    pub dead_letter: DeadLetterConfig,
}

impl Default for JobConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            worker: WorkerConfig::default(),
            queue: QueueConfig::default(),
            scheduler: SchedulerConfig::default(),
            retry: RetryConfig::default(),
            metrics: MetricsConfig::default(),
            dead_letter: DeadLetterConfig::default(),
        }
    }
}

impl JobConfig {
    /// Development configuration (fewer workers, more logging)
    pub fn development() -> Self {
        Self {
            worker: WorkerConfig {
                pool_size: 2, // Smaller pool for development
                enable_logging: true,
                ..Default::default()
            },
            metrics: MetricsConfig {
                enabled: true,
                log_interval_secs: 30,
                ..Default::default()
            },
            ..Self::default()
        }
    }

    /// Production configuration (optimized for performance)
    pub fn production() -> Self {
        Self {
            worker: WorkerConfig {
                pool_size: 20, // Larger pool for production
                enable_logging: false,
                ..Default::default()
            },
            retry: RetryConfig {
                max_attempts: 5, // More retries in production
                ..Default::default()
            },
            metrics: MetricsConfig {
                enabled: true,
                log_interval_secs: 300, // 5 minutes
                ..Default::default()
            },
            dead_letter: DeadLetterConfig {
                enabled: true,
                max_age_secs: 86400, // 24 hours
                ..Default::default()
            },
            ..Self::default()
        }
    }
}

/// Worker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    /// Number of worker threads
    #[serde(default = "default_worker_pool_size")]
    pub pool_size: usize,

    /// Job execution timeout
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Max concurrent jobs per worker
    #[serde(default = "default_max_concurrent_jobs")]
    pub max_concurrent_jobs: usize,

    /// Enable worker logging
    #[serde(default = "default_true")]
    pub enable_logging: bool,

    /// Worker heartbeat interval
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_secs: u64,

    /// Enable job result persistence
    #[serde(default = "default_true")]
    pub persist_results: bool,

    /// Result TTL (seconds)
    #[serde(default = "default_result_ttl")]
    pub result_ttl_secs: u64,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            pool_size: 10,
            timeout_secs: 300, // 5 minutes
            max_concurrent_jobs: 5,
            enable_logging: true,
            heartbeat_interval_secs: 30,
            persist_results: true,
            result_ttl_secs: 86400, // 24 hours
        }
    }
}

impl WorkerConfig {
    /// Get job timeout as Duration
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }

    /// Get heartbeat interval as Duration
    pub fn heartbeat_interval(&self) -> Duration {
        Duration::from_secs(self.heartbeat_interval_secs)
    }

    /// Get result TTL as Duration
    pub fn result_ttl(&self) -> Duration {
        Duration::from_secs(self.result_ttl_secs)
    }
}

/// Queue configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueConfig {
    /// Default queue name
    #[serde(default = "default_queue_name")]
    pub default_queue: String,

    /// Queue names and their priority (higher = more important)
    #[serde(default = "default_queues")]
    pub queues: Vec<(String, u8)>,

    /// Max queue depth (0 = unlimited)
    #[serde(default)]
    pub max_depth: usize,

    /// Enable queue overflow protection
    #[serde(default = "default_true")]
    pub overflow_protection: bool,

    /// Queue overflow strategy (drop_newest, drop_oldest, block)
    #[serde(default = "default_overflow_strategy")]
    pub overflow_strategy: OverflowStrategy,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            default_queue: "default".to_string(),
            queues: vec![
                ("high".to_string(), 100),
                ("default".to_string(), 50),
                ("low".to_string(), 10),
            ],
            max_depth: 10000,
            overflow_protection: true,
            overflow_strategy: OverflowStrategy::Block,
        }
    }
}

/// Queue overflow strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverflowStrategy {
    /// Block new jobs when queue is full
    Block,

    /// Drop newest jobs when queue is full
    DropNewest,

    /// Drop oldest jobs when queue is full
    DropOldest,
}

impl OverflowStrategy {
    /// Check if strategy blocks when full
    pub fn should_block(&self) -> bool {
        matches!(self, OverflowStrategy::Block)
    }
}

/// Scheduler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    /// Enable scheduler
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Check interval for scheduled jobs
    #[serde(default = "default_check_interval")]
    pub check_interval_secs: u64,

    /// Max scheduled jobs
    #[serde(default = "default_max_scheduled_jobs")]
    pub max_scheduled_jobs: usize,

    /// Timezone for scheduling (e.g., "UTC", "America/New_York")
    #[serde(default = "default_timezone")]
    pub timezone: String,

    /// Enable cron parsing
    #[serde(default = "default_true")]
    pub enable_cron: bool,

    /// Max cron jobs
    #[serde(default = "default_max_cron_jobs")]
    pub max_cron_jobs: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_secs: 60, // Check every minute
            max_scheduled_jobs: 10000,
            timezone: "UTC".to_string(),
            enable_cron: true,
            max_cron_jobs: 1000,
        }
    }
}

impl SchedulerConfig {
    /// Get check interval as Duration
    pub fn check_interval(&self) -> Duration {
        Duration::from_secs(self.check_interval_secs)
    }
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Enable automatic retries
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Max retry attempts
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,

    /// Initial retry delay (seconds)
    #[serde(default = "default_initial_retry_delay")]
    pub initial_delay_secs: u64,

    /// Max retry delay (seconds)
    #[serde(default = "default_max_retry_delay")]
    pub max_delay_secs: u64,

    /// Retry backoff multiplier
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,

    /// Retry on timeout
    #[serde(default = "default_true")]
    pub retry_on_timeout: bool,

    /// Retry specific error types
    #[serde(default)]
    pub retry_on_errors: Vec<String>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 3,
            initial_delay_secs: 1,
            max_delay_secs: 3600, // 1 hour
            backoff_multiplier: 2.0,
            retry_on_timeout: true,
            retry_on_errors: vec!["network".to_string(), "database".to_string()],
        }
    }
}

impl RetryConfig {
    /// Get initial delay as Duration
    pub fn initial_delay(&self) -> Duration {
        Duration::from_secs(self.initial_delay_secs)
    }

    /// Get max delay as Duration
    pub fn max_delay(&self) -> Duration {
        Duration::from_secs(self.max_delay_secs)
    }
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable metrics collection
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Log metrics interval
    #[serde(default = "default_metrics_interval")]
    pub log_interval_secs: u64,

    /// Store metrics history
    #[serde(default = "default_true")]
    pub store_history: bool,

    /// History retention (seconds)
    #[serde(default = "default_history_retention")]
    pub history_retention_secs: u64,

    /// Track job latency
    #[serde(default = "default_true")]
    pub track_latency: bool,

    /// Alert thresholds
    #[serde(default)]
    pub alert_thresholds: AlertThresholds,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_interval_secs: 60,
            store_history: true,
            history_retention_secs: 86400, // 24 hours
            track_latency: true,
            alert_thresholds: AlertThresholds::default(),
        }
    }
}

impl MetricsConfig {
    /// Get log interval as Duration
    pub fn log_interval(&self) -> Duration {
        Duration::from_secs(self.log_interval_secs)
    }

    /// Get history retention as Duration
    pub fn history_retention(&self) -> Duration {
        Duration::from_secs(self.history_retention_secs)
    }
}

/// Alert thresholds for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// Queue depth threshold (0 = disabled)
    #[serde(default)]
    pub queue_depth: usize,

    /// Job failure rate threshold (0.0 - 1.0, 0 = disabled)
    #[serde(default)]
    pub failure_rate: f64,

    /// Job latency threshold (seconds, 0 = disabled)
    #[serde(default)]
    pub latency_secs: u64,

    /// Worker crash threshold (0 = disabled)
    #[serde(default)]
    pub worker_crashes: u32,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            queue_depth: 1000,
            failure_rate: 0.1, // 10%
            latency_secs: 300, // 5 minutes
            worker_crashes: 3,
        }
    }
}

/// Dead letter queue configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterConfig {
    /// Enable dead letter queue
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Max age of dead letters (seconds)
    #[serde(default = "default_dead_letter_age")]
    pub max_age_secs: u64,

    /// Max dead letters to store
    #[serde(default = "default_max_dead_letters")]
    pub max_dead_letters: usize,

    /// Alert on dead letters
    #[serde(default = "default_true")]
    pub alert_on_dead_letters: bool,
}

impl Default for DeadLetterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_age_secs: 86400, // 24 hours
            max_dead_letters: 10000,
            alert_on_dead_letters: true,
        }
    }
}

impl DeadLetterConfig {
    /// Get max age as Duration
    pub fn max_age(&self) -> Duration {
        Duration::from_secs(self.max_age_secs)
    }
}

// Default value helper functions
fn default_true() -> bool {
    true
}
#[allow(dead_code)]
fn default_false() -> bool {
    false
}
fn default_worker_pool_size() -> usize {
    10
}
fn default_timeout() -> u64 {
    300
}
fn default_max_concurrent_jobs() -> usize {
    5
}
fn default_heartbeat_interval() -> u64 {
    30
}
fn default_result_ttl() -> u64 {
    86400
}
fn default_queue_name() -> String {
    "default".to_string()
}
fn default_queues() -> Vec<(String, u8)> {
    vec![
        ("high".to_string(), 100),
        ("default".to_string(), 50),
        ("low".to_string(), 10),
    ]
}
fn default_overflow_strategy() -> OverflowStrategy {
    OverflowStrategy::Block
}
fn default_check_interval() -> u64 {
    60
}
fn default_max_scheduled_jobs() -> usize {
    10000
}
fn default_timezone() -> String {
    "UTC".to_string()
}
fn default_max_cron_jobs() -> usize {
    1000
}
fn default_max_attempts() -> u32 {
    3
}
fn default_initial_retry_delay() -> u64 {
    1
}
fn default_max_retry_delay() -> u64 {
    3600
}
fn default_backoff_multiplier() -> f64 {
    2.0
}
fn default_metrics_interval() -> u64 {
    60
}
fn default_history_retention() -> u64 {
    86400
}
fn default_dead_letter_age() -> u64 {
    86400
}
fn default_max_dead_letters() -> usize {
    10000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = JobConfig::default();
        assert!(config.enabled);
        assert_eq!(config.worker.pool_size, 10);
        assert_eq!(config.queue.queues.len(), 3);
    }

    #[test]
    fn test_development_config() {
        let config = JobConfig::development();
        assert_eq!(config.worker.pool_size, 2);
        assert!(config.worker.enable_logging);
    }

    #[test]
    fn test_production_config() {
        let config = JobConfig::production();
        assert_eq!(config.worker.pool_size, 20);
        assert_eq!(config.retry.max_attempts, 5);
    }

    #[test]
    fn test_overflow_strategy() {
        assert!(OverflowStrategy::Block.should_block());
        assert!(!OverflowStrategy::DropNewest.should_block());
        assert!(!OverflowStrategy::DropOldest.should_block());
    }

    #[test]
    fn test_alert_thresholds() {
        let thresholds = AlertThresholds::default();
        assert_eq!(thresholds.queue_depth, 1000);
        assert_eq!(thresholds.failure_rate, 0.1);
        assert_eq!(thresholds.latency_secs, 300);
        assert_eq!(thresholds.worker_crashes, 3);
    }
}
