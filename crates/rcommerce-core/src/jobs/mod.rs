//! Background job processing system
//!
//! This module provides a robust background job processing system with:
//! - Async task processing
//! - Worker pool management
//! - Scheduled tasks (cron-like)
//! - Job status tracking
//! - Failure retry logic with exponential backoff
//! - Priority queues
//! - Dead letter queues
//! - Job serialization/deserialization
//! - Metrics and monitoring
//!
//! ## Architecture
//!
//! The system uses Redis as the backend (leveraging Phase 3.7) for:
//! - Job queue storage
//! - Job status tracking
//! - Worker coordination
//! - Scheduled job storage
//! - Dead letter queue
//! - Metrics storage
//!
//! ## Features
//!
//! ✅ **Job Processing**
//! - Async task execution
//! - Worker pool with configurable size
//! - Priority queues (high, normal, low)
//! - Job types with typed payloads
//! - Middleware support
//!
//! ✅ **Reliability**
//! - Automatic retry with exponential backoff
//! - Dead letter queue for failed jobs
//! - Job persistence across restarts
//! - Worker heartbeat and liveness checks
//! - job timeouts
//!
//! ✅ **Scheduling**
//! - Cron-like scheduling
//! - One-time scheduled jobs
//! - Recurring jobs
//! - Timezone support
//!
//! ✅ **Monitoring**
//! - Job success/failure metrics
//! - Queue depth tracking
//! - Worker utilization
//! - job latency measurements
//! - Alert thresholds

pub mod config;
pub mod dead_letter;
pub mod dunning_job;
pub mod job;
pub mod metrics;
pub mod queue;
pub mod retry;
pub mod scheduler;
pub mod worker;

// Re-export main types
pub use config::{JobConfig, SchedulerConfig, WorkerConfig};
pub use dead_letter::{DeadLetter, DeadLetterQueue};
pub use dunning_job::{DunningJob, DunningJobResult, DunningJobScheduler, DunningJobStats};
pub use job::{Job, JobId, JobPriority, JobQuery, JobResult, JobStatus};
pub use metrics::{JobMetrics, MetricsSummary};
pub use queue::{JobQueue, QueueStats};
pub use retry::{ExponentialBackoff, RetryAttempt, RetryHistory, RetryPolicy};
pub use scheduler::JobScheduler;
pub use worker::{Worker, WorkerId};
// JobError is defined in this module and re-exported automatically

/// Job processing result type
pub type JobProcessingResult<T> = Result<T, JobError>;

/// Error types for job processing
#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
pub enum JobError {
    #[error("Job serialization error: {0}")]
    Serialization(String),

    #[error("Job deserialization error: {0}")]
    Deserialization(String),

    #[error("Queue error: {0}")]
    Queue(String),

    #[error("Worker error: {0}")]
    Worker(String),

    #[error("Job execution failed: {0}")]
    Execution(String),

    #[error("Job timeout after {0}ms")]
    TimeoutMillis(u64),

    #[error("Job cancelled")]
    Cancelled,

    #[error("Job not found: {0}")]
    NotFound(JobId),
}

impl From<JobError> for crate::Error {
    fn from(err: JobError) -> Self {
        crate::Error::Other(err.to_string())
    }
}

impl From<crate::cache::CacheError> for JobError {
    fn from(err: crate::cache::CacheError) -> Self {
        JobError::Queue(format!("Cache error: {}", err))
    }
}

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Job middleware trait for cross-cutting concerns
#[async_trait::async_trait]
pub trait JobMiddleware: Send + Sync {
    /// Called before job execution
    async fn before_execution(&self, job: &Job) -> JobProcessingResult<()>;

    /// Called after job execution
    async fn after_execution(&self, job: &Job, result: &JobResult) -> JobProcessingResult<()>;

    /// Called on job failure
    async fn on_failure(&self, job: &Job, error: &JobError) -> JobProcessingResult<()>;
}

/// Job handler trait for executing jobs
#[async_trait::async_trait]
pub trait JobHandler: Send + Sync {
    /// Execute the job
    async fn handle(&self, job: Job) -> JobProcessingResult<JobResult>;
}

/// Job context passed to handlers
#[derive(Debug, Clone)]
pub struct JobContext {
    /// Job ID
    pub job_id: JobId,

    /// Queue name
    pub queue: String,

    /// Attempt number
    pub attempt: u32,

    /// Max attempts
    pub max_attempts: u32,

    /// Job started at
    pub started_at: chrono::DateTime<chrono::Utc>,

    /// Job timeout
    pub timeout: Duration,
}

impl JobContext {
    /// Create a new job context
    pub fn new(job_id: JobId, queue: String, max_attempts: u32, timeout: Duration) -> Self {
        Self {
            job_id,
            queue,
            attempt: 1,
            max_attempts,
            started_at: chrono::Utc::now(),
            timeout,
        }
    }

    /// Check if this is the last attempt
    pub fn is_last_attempt(&self) -> bool {
        self.attempt >= self.max_attempts
    }

    /// Get time elapsed since job started
    pub fn elapsed(&self) -> Duration {
        let now = Utc::now();
        let duration_ms = (now - self.started_at).num_milliseconds();
        if duration_ms > 0 {
            Duration::from_millis(duration_ms as u64)
        } else {
            Duration::from_millis(0)
        }
    }

    /// Check if job has timed out
    pub fn has_timed_out(&self) -> bool {
        self.elapsed() > self.timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_context() {
        let context = JobContext::new(
            JobId::new_v4(),
            "default".to_string(),
            3,
            Duration::from_secs(300),
        );

        assert_eq!(context.attempt, 1);
        assert_eq!(context.max_attempts, 3);
        assert!(!context.is_last_attempt());
        assert!(!context.has_timed_out());
    }

    #[test]
    fn test_job_error() {
        let error = JobError::Execution("test error".to_string());
        assert!(error.to_string().contains("test error"));

        let error = JobError::TimeoutMillis(30000);
        assert!(error.to_string().contains("timeout"));
    }
}
