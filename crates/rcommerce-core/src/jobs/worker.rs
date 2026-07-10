//! Worker implementation for job processing

use crate::jobs::{
    Job, JobConfig, JobContext, JobError, JobHandler, JobId, JobProcessingResult, JobQueue,
    JobStatus, RetryAttempt, RetryHistory,
};
use std::{collections::HashMap, sync::Arc};
use tokio::{
    sync::{Mutex, RwLock},
    task::JoinHandle,
    time::{sleep, Duration},
};
use tracing::{error, info, warn};
use uuid::Uuid;

/// Unique worker identifier
pub type WorkerId = Uuid;

/// Worker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerState {
    /// Starting up
    Starting,

    /// Running and processing jobs
    Running,

    /// Paused, not processing jobs
    Paused,

    /// Shutting down
    Stopping,

    /// Stopped
    Stopped,

    /// Failed/error state
    Failed,
}

/// Individual worker
pub struct Worker {
    /// Worker ID
    pub id: WorkerId,

    /// Worker name
    pub name: String,

    /// Queue this worker processes
    pub queue: Arc<JobQueue>,

    /// Configuration
    config: Arc<JobConfig>,

    /// State
    state: Arc<RwLock<WorkerState>>,

    /// Current job (if any)
    current_job: Arc<Mutex<Option<JobId>>>,

    /// Job handler
    handler: Arc<dyn JobHandler>,

    /// Total jobs processed
    jobs_processed: Arc<Mutex<u64>>,

    /// Successful jobs
    jobs_succeeded: Arc<Mutex<u64>>,

    /// Failed jobs
    jobs_failed: Arc<Mutex<u64>>,

    /// Retry history
    retry_history: Arc<Mutex<HashMap<JobId, RetryHistory>>>,
}

impl Worker {
    /// Create a new worker
    pub fn new(
        name: impl Into<String>,
        queue: JobQueue,
        config: Arc<JobConfig>,
        handler: Arc<dyn JobHandler>,
    ) -> Self {
        let name = name.into();
        let id = WorkerId::new_v4();

        info!(
            "Creating worker: id={}, name={}, queue={}",
            id,
            name,
            queue.name()
        );

        Self {
            id,
            name,
            queue: Arc::new(queue),
            config,
            state: Arc::new(RwLock::new(WorkerState::Starting)),
            current_job: Arc::new(Mutex::new(None)),
            handler,
            jobs_processed: Arc::new(Mutex::new(0)),
            jobs_succeeded: Arc::new(Mutex::new(0)),
            jobs_failed: Arc::new(Mutex::new(0)),
            retry_history: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start worker
    pub async fn start(self: Arc<Self>) -> JobProcessingResult<JoinHandle<()>> {
        info!("Starting worker: id={}, name={}", self.id, self.name);

        *self.state.write().await = WorkerState::Running;

        let self_clone = self.clone();
        let handle = tokio::spawn(async move {
            self_clone.run().await;
        });

        Ok(handle)
    }

    /// Run worker loop
    async fn run(&self) {
        info!("Worker {} running", self.id);

        while *self.state.read().await == WorkerState::Running {
            // Dequeue a job
            match self.queue.dequeue(1).await {
                Ok(Some(job)) => {
                    // Process job
                    let result = self.process_job(job).await;

                    // Update counters
                    let mut processed = self.jobs_processed.lock().await;
                    *processed += 1;
                    drop(processed);

                    match result {
                        Ok(_) => {
                            let mut succeeded = self.jobs_succeeded.lock().await;
                            *succeeded += 1;
                        }
                        Err(_) => {
                            let mut failed = self.jobs_failed.lock().await;
                            *failed += 1;
                        }
                    }
                }
                Ok(None) => {
                    // No job available, wait a bit
                    sleep(Duration::from_millis(100)).await;
                }
                Err(e) => {
                    error!("Worker {} error dequeuing job: {}", self.id, e);
                    sleep(Duration::from_secs(1)).await;
                }
            }
        }

        info!("Worker {} stopped", self.id);
        *self.state.write().await = WorkerState::Stopped;
    }

    /// Process a single job
    async fn process_job(&self, mut job: Job) -> JobProcessingResult<()> {
        info!(
            "Worker {} processing job: id={}, type={}, attempt={}/{}",
            self.id, job.id, job.job_type, job.attempt, job.max_attempts
        );

        // Mark job as started
        job.mark_started(self.id);
        self.queue.save_job(&job).await?;

        // Set current job
        *self.current_job.lock().await = Some(job.id);

        // Create job context
        let _context = JobContext::new(
            job.id,
            self.queue.name().to_string(),
            job.max_attempts,
            Duration::from_secs(job.timeout_secs),
        );

        // Execute job with timeout
        let result = tokio::time::timeout(
            Duration::from_secs(job.timeout_secs),
            self.handler.handle(job.clone()),
        )
        .await;

        match result {
            Ok(Ok(_job_result)) => {
                // Job succeeded
                job.mark_completed();
                self.queue.save_job(&job).await?;
                self.queue
                    .update_job_status(&job.id, JobStatus::Completed)
                    .await?;

                info!("Worker {} completed job: id={}", self.id, job.id);
                Ok(())
            }
            Ok(Err(job_error)) => {
                // Job failed during execution
                self.handle_job_failure(job, job_error).await
            }
            Err(_) => {
                // Job timed out
                let timeout_error = JobError::TimeoutMillis(job.timeout_secs * 1000);
                self.handle_job_failure(job, timeout_error).await
            }
        }
    }

    /// Handle job failure
    async fn handle_job_failure(&self, mut job: Job, error: JobError) -> JobProcessingResult<()> {
        warn!(
            "Worker {} job failed: id={}, error={}, attempt={}/{}",
            self.id, job.id, error, job.attempt, job.max_attempts
        );

        // Update retry history
        let mut history = self.retry_history.lock().await;
        let job_history = history.entry(job.id).or_insert_with(RetryHistory::new);

        // Get retry delay and add to history
        let retry_delay = self.config.retry.initial_delay();
        let attempt = RetryAttempt::new(job.attempt, error.clone(), retry_delay);
        job_history.add_attempt(attempt);

        // Check if job can be retried
        if job.can_retry() {
            // Re-enqueue job for retry
            job.mark_failed();
            self.queue.save_job(&job).await?;

            // Re-enqueue with delay
            job.scheduled_for = Some(chrono::Utc::now().timestamp() + retry_delay.as_secs() as i64);
            self.queue.enqueue(&job).await?;

            info!(
                "Worker {} re-enqueued job for retry: id={}, delay={:?}",
                self.id, job.id, retry_delay
            );
        } else {
            // Job has failed all retries, mark as dead
            job.mark_dead();
            self.queue.save_job(&job).await?;
            self.queue
                .update_job_status(&job.id, JobStatus::Dead)
                .await?;

            error!(
                "Worker {} job permanently failed: id={}, attempts={}",
                self.id, job.id, job.attempt
            );
        }

        Ok(())
    }

    /// Get worker statistics
    pub async fn stats(&self) -> WorkerStats {
        WorkerStats {
            id: self.id,
            name: self.name.clone(),
            queue_name: self.queue.name().to_string(),
            state: *self.state.read().await,
            current_job: *self.current_job.lock().await,
            jobs_processed: *self.jobs_processed.lock().await,
            jobs_succeeded: *self.jobs_succeeded.lock().await,
            jobs_failed: *self.jobs_failed.lock().await,
        }
    }

    /// Pause worker
    pub async fn pause(&self) {
        info!("Pausing worker: id={}", self.id);
        *self.state.write().await = WorkerState::Paused;
    }

    /// Resume worker
    pub async fn resume(&self) {
        info!("Resuming worker: id={}", self.id);
        *self.state.write().await = WorkerState::Running;
    }

    /// Stop worker
    pub async fn stop(&self) {
        info!("Stopping worker: id={}", self.id);
        *self.state.write().await = WorkerState::Stopping;
    }

    /// Check if worker is running
    pub async fn is_running(&self) -> bool {
        *self.state.read().await == WorkerState::Running
    }

    /// Get current job ID
    pub async fn current_job(&self) -> Option<JobId> {
        *self.current_job.lock().await
    }
}

impl Clone for Worker {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            name: self.name.clone(),
            queue: self.queue.clone(),
            config: self.config.clone(),
            state: self.state.clone(),
            current_job: self.current_job.clone(),
            handler: self.handler.clone(),
            jobs_processed: self.jobs_processed.clone(),
            jobs_succeeded: self.jobs_succeeded.clone(),
            jobs_failed: self.jobs_failed.clone(),
            retry_history: self.retry_history.clone(),
        }
    }
}

/// Worker statistics
#[derive(Debug, Clone)]
pub struct WorkerStats {
    /// Worker ID
    pub id: WorkerId,

    /// Worker name
    pub name: String,

    /// Queue name
    pub queue_name: String,

    /// Current state
    pub state: WorkerState,

    /// Current job ID (if any)
    pub current_job: Option<JobId>,

    /// Total jobs processed
    pub jobs_processed: u64,

    /// Successful jobs
    pub jobs_succeeded: u64,

    /// Failed jobs
    pub jobs_failed: u64,
}

impl WorkerStats {
    /// Calculate success rate
    pub fn success_rate(&self) -> f64 {
        if self.jobs_processed == 0 {
            0.0
        } else {
            self.jobs_succeeded as f64 / self.jobs_processed as f64
        }
    }

    /// Calculate failure rate
    pub fn failure_rate(&self) -> f64 {
        if self.jobs_processed == 0 {
            0.0
        } else {
            self.jobs_failed as f64 / self.jobs_processed as f64
        }
    }

    /// Format as human-readable
    pub fn format(&self) -> String {
        let state_str = match self.state {
            WorkerState::Starting => "starting",
            WorkerState::Running => "running",
            WorkerState::Paused => "paused",
            WorkerState::Stopping => "stopping",
            WorkerState::Stopped => "stopped",
            WorkerState::Failed => "failed",
        };

        format!(
            "Worker '{}' [{}]: state={}, processed={}, success_rate={:.1}%, current_job={:?}",
            self.name,
            self.id,
            state_str,
            self.jobs_processed,
            self.success_rate() * 100.0,
            self.current_job
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{RedisConfig, RedisPool};
    use crate::jobs::{Job, JobConfig, JobHandler, JobResult};

    // Mock handler for testing
    struct MockHandler;

    #[async_trait::async_trait]
    impl JobHandler for MockHandler {
        async fn handle(&self, _job: Job) -> JobProcessingResult<JobResult> {
            Ok(JobResult::success(None))
        }
    }

    #[tokio::test]
    async fn test_worker_creation() {
        let config = RedisConfig::default();
        let pool = RedisPool::new(config).await;

        if let Ok(pool) = pool {
            let queue = JobQueue::new(pool, "test_queue");
            let job_config = Arc::new(JobConfig::default());
            let handler = Arc::new(MockHandler);

            let worker = Worker::new("test_worker", queue, job_config, handler);

            assert_eq!(worker.name, "test_worker");
            assert_eq!(*worker.state.read().await, WorkerState::Starting);
        }
    }

    #[tokio::test]
    async fn test_worker_lifecycle() {
        let config = RedisConfig::default();
        let pool = RedisPool::new(config).await;

        if let Ok(pool) = pool {
            let queue = JobQueue::new(pool, "test_queue");
            let job_config = Arc::new(JobConfig::default());
            let handler = Arc::new(MockHandler);

            let worker = Arc::new(Worker::new("test_worker", queue, job_config, handler));

            // Start worker
            let handle = worker.clone().start().await.unwrap();

            // Should be running
            assert!(worker.is_running().await);

            // Pause
            worker.pause().await;
            assert_eq!(*worker.state.read().await, WorkerState::Paused);

            // Resume
            worker.resume().await;
            assert!(worker.is_running().await);

            // Stop
            worker.stop().await;
            handle.await.unwrap();

            assert_eq!(*worker.state.read().await, WorkerState::Stopped);
        }
    }

    #[tokio::test]
    async fn test_worker_stats() {
        let config = RedisConfig::default();
        let pool = RedisPool::new(config).await;

        if let Ok(pool) = pool {
            let queue = JobQueue::new(pool, "test_queue");
            let job_config = Arc::new(JobConfig::default());
            let handler = Arc::new(MockHandler);

            let worker = Arc::new(Worker::new("test_worker", queue, job_config, handler));

            let stats = worker.stats().await;

            assert_eq!(stats.name, "test_worker");
            assert_eq!(stats.jobs_processed, 0);
            assert_eq!(stats.success_rate(), 0.0);
        }
    }
}
