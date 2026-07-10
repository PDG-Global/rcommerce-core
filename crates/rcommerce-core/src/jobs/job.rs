//! Job types and definitions

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

/// Unique job identifier
pub type JobId = Uuid;

/// Job priority levels
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
pub enum JobPriority {
    /// High priority jobs (processed first)
    High = 100,

    /// Normal priority jobs (default)
    #[default]
    Normal = 50,

    /// Low priority jobs (processed last)
    Low = 10,
}

impl fmt::Display for JobPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobPriority::High => write!(f, "high"),
            JobPriority::Normal => write!(f, "normal"),
            JobPriority::Low => write!(f, "low"),
        }
    }
}

impl JobPriority {
    /// Convert to integer weight
    pub fn weight(&self) -> u8 {
        match self {
            JobPriority::High => 100,
            JobPriority::Normal => 50,
            JobPriority::Low => 10,
        }
    }

    /// Create from string
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "high" => Some(JobPriority::High),
            "normal" => Some(JobPriority::Normal),
            "low" => Some(JobPriority::Low),
            _ => None,
        }
    }
}

/// Job execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum JobStatus {
    /// Job is pending execution
    #[default]
    Pending,

    /// Job is being processed
    Running,

    /// Job completed successfully
    Completed,

    /// Job failed (will retry if configured)
    Failed,

    /// Job permanently failed (in dead letter queue)
    Dead,

    /// Job was cancelled
    Cancelled,

    /// Job timed out
    TimedOut,
}

impl fmt::Display for JobStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JobStatus::Pending => write!(f, "pending"),
            JobStatus::Running => write!(f, "running"),
            JobStatus::Completed => write!(f, "completed"),
            JobStatus::Failed => write!(f, "failed"),
            JobStatus::Dead => write!(f, "dead"),
            JobStatus::Cancelled => write!(f, "cancelled"),
            JobStatus::TimedOut => write!(f, "timed_out"),
        }
    }
}

impl JobStatus {
    /// Check if job is terminal (won't change)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            JobStatus::Completed | JobStatus::Dead | JobStatus::Cancelled | JobStatus::TimedOut
        )
    }

    /// Check if job can be retried
    pub fn is_retryable(&self) -> bool {
        matches!(self, JobStatus::Failed | JobStatus::TimedOut)
    }

    /// Check if job is active
    pub fn is_active(&self) -> bool {
        matches!(self, JobStatus::Pending | JobStatus::Running)
    }
}

/// Job result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    /// Success status
    pub success: bool,

    /// Result data
    pub data: Option<serde_json::Value>,

    /// Error message if failed
    pub error: Option<String>,

    /// Result metadata
    pub metadata: HashMap<String, String>,
}

impl JobResult {
    /// Create a successful result
    pub fn success(data: Option<serde_json::Value>) -> Self {
        Self {
            success: true,
            data,
            error: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a failed result
    pub fn failure(error: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
            metadata: HashMap::new(),
        }
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// Job definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Unique job ID
    pub id: JobId,

    /// Job type
    pub job_type: String,

    /// Job payload
    pub payload: serde_json::Value,

    /// Job priority
    pub priority: JobPriority,

    /// Queue name
    pub queue: String,

    /// Job status
    pub status: JobStatus,

    /// Current attempt number
    pub attempt: u32,

    /// Max retry attempts
    pub max_attempts: u32,

    /// Created timestamp
    pub created_at: i64,

    /// Scheduled for timestamp (if delayed)
    pub scheduled_for: Option<i64>,

    /// Started timestamp
    pub started_at: Option<i64>,

    /// Completed timestamp
    pub completed_at: Option<i64>,

    /// Worker ID (if assigned)
    pub worker_id: Option<Uuid>,

    /// Tags for categorization
    pub tags: Vec<String>,

    /// Job metadata
    pub metadata: HashMap<String, String>,

    /// Timeout (seconds)
    pub timeout_secs: u64,
}

impl Job {
    /// Create a new job
    pub fn new(
        job_type: impl Into<String>,
        payload: serde_json::Value,
        queue: impl Into<String>,
    ) -> Self {
        let job_id = JobId::new_v4();
        let now = chrono::Utc::now().timestamp();

        Self {
            id: job_id,
            job_type: job_type.into(),
            payload,
            priority: JobPriority::default(),
            queue: queue.into(),
            status: JobStatus::default(),
            attempt: 0,
            max_attempts: 3,
            created_at: now,
            scheduled_for: None,
            started_at: None,
            completed_at: None,
            worker_id: None,
            tags: vec![],
            metadata: HashMap::new(),
            timeout_secs: 300, // 5 minutes default
        }
    }

    /// Create with specific priority
    pub fn with_priority(mut self, priority: JobPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Create with tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Schedule for later execution
    pub fn schedule_for(mut self, timestamp: i64) -> Self {
        self.scheduled_for = Some(timestamp);
        self
    }

    /// Set max attempts
    pub fn with_max_attempts(mut self, max_attempts: u32) -> Self {
        self.max_attempts = max_attempts;
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Check if job is scheduled for later
    pub fn is_scheduled(&self) -> bool {
        self.scheduled_for.is_some()
    }

    /// Check if job should be executed now
    pub fn should_execute_now(&self) -> bool {
        if let Some(scheduled_for) = self.scheduled_for {
            chrono::Utc::now().timestamp() >= scheduled_for
        } else {
            true // Not scheduled, execute immediately
        }
    }

    /// Get time until execution (if scheduled)
    pub fn time_until_execution(&self) -> Option<i64> {
        self.scheduled_for.map(|scheduled| {
            let now = chrono::Utc::now().timestamp();
            (scheduled - now).max(0)
        })
    }

    /// Mark as started
    pub fn mark_started(&mut self, worker_id: Uuid) {
        self.status = JobStatus::Running;
        self.worker_id = Some(worker_id);
        self.started_at = Some(chrono::Utc::now().timestamp());
        self.attempt += 1;
    }

    /// Mark as completed
    pub fn mark_completed(&mut self) {
        self.status = JobStatus::Completed;
        self.completed_at = Some(chrono::Utc::now().timestamp());
    }

    /// Mark as failed
    pub fn mark_failed(&mut self) {
        self.status = JobStatus::Failed;
        self.completed_at = Some(chrono::Utc::now().timestamp());
    }

    /// Mark as dead (in dead letter queue)
    pub fn mark_dead(&mut self) {
        self.status = JobStatus::Dead;
        self.completed_at = Some(chrono::Utc::now().timestamp());
    }

    /// Check if job can be retried
    pub fn can_retry(&self) -> bool {
        self.status.is_retryable() && self.attempt < self.max_attempts
    }

    /// Check if job has timed out (based on started_at)
    pub fn has_timed_out(&self) -> bool {
        if let Some(started_at) = self.started_at {
            let elapsed = chrono::Utc::now().timestamp() - started_at;
            elapsed > self.timeout_secs as i64
        } else {
            false // Not started yet
        }
    }

    /// Get job duration (if completed)
    pub fn duration(&self) -> Option<i64> {
        match (self.started_at, self.completed_at) {
            (Some(started), Some(completed)) => Some(completed - started),
            _ => None,
        }
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> serde_json::Result<serde_json::Value> {
        serde_json::to_value(self)
    }

    /// Deserialize from JSON
    pub fn from_json(value: serde_json::Value) -> serde_json::Result<Self> {
        serde_json::from_value(value)
    }
}

/// Job execution info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobExecutionInfo {
    /// Job ID
    pub job_id: JobId,

    /// Worker ID
    pub worker_id: Uuid,

    /// Started timestamp
    pub started_at: i64,

    /// Duration (if completed)
    pub duration: Option<i64>,

    /// Result
    pub result: Option<JobResult>,

    /// Error if failed
    pub error: Option<String>,
}

impl JobExecutionInfo {
    /// Create from job
    pub fn from_job(job: &Job, worker_id: Uuid) -> Self {
        Self {
            job_id: job.id,
            worker_id,
            started_at: job.started_at.unwrap_or(0),
            duration: job.duration(),
            result: None,
            error: None,
        }
    }

    /// Mark as completed with result
    pub fn complete(&mut self, result: JobResult) {
        self.result = Some(result);
        self.duration = Some(chrono::Utc::now().timestamp() - self.started_at);
    }

    /// Mark as failed with error
    pub fn fail(&mut self, error: String) {
        self.error = Some(error);
        self.duration = Some(chrono::Utc::now().timestamp() - self.started_at);
    }
}

/// Job query for filtering and searching
#[derive(Debug, Default, Clone)]
pub struct JobQuery {
    /// Filter by status
    pub status: Option<JobStatus>,

    /// Filter by queue
    pub queue: Option<String>,

    /// Filter by job type
    pub job_type: Option<String>,

    /// Filter by worker
    pub worker_id: Option<Uuid>,

    /// Filter by tags (all must match)
    pub tags: Vec<String>,

    /// Filter by created after
    pub created_after: Option<i64>,

    /// Filter by created before
    pub created_before: Option<i64>,

    /// Limit results
    pub limit: Option<usize>,

    /// Offset for pagination
    pub offset: Option<usize>,
}

impl JobQuery {
    /// Create a new query
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by status
    pub fn with_status(mut self, status: JobStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// Filter by queue
    pub fn with_queue(mut self, queue: impl Into<String>) -> Self {
        self.queue = Some(queue.into());
        self
    }

    /// Filter by job type
    pub fn with_job_type(mut self, job_type: impl Into<String>) -> Self {
        self.job_type = Some(job_type.into());
        self
    }

    /// Filter by tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Limit results
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Add offset
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = Some(offset);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_priority() {
        assert_eq!(JobPriority::High.weight(), 100);
        assert_eq!(JobPriority::Normal.weight(), 50);
        assert_eq!(JobPriority::Low.weight(), 10);

        assert!(JobPriority::High > JobPriority::Normal);
        assert!(JobPriority::Normal > JobPriority::Low);
    }

    #[test]
    fn test_job_status() {
        assert!(JobStatus::Completed.is_terminal());
        assert!(JobStatus::Failed.is_retryable());
        assert!(JobStatus::Running.is_active());
        assert!(!JobStatus::Pending.is_terminal());
    }

    #[test]
    fn test_job_creation() {
        let payload = serde_json::json!({"test": "data"});
        let job = Job::new("test_job", payload, "default");

        assert_eq!(job.job_type, "test_job");
        assert_eq!(job.queue, "default");
        assert_eq!(job.status, JobStatus::Pending);
        assert_eq!(job.attempt, 0);
    }

    #[test]
    fn test_job_with_options() {
        let payload = serde_json::json!({});
        let job = Job::new("test", payload, "default")
            .with_priority(JobPriority::High)
            .with_max_attempts(5)
            .with_tags(vec!["tag1".to_string(), "tag2".to_string()]);

        assert_eq!(job.priority, JobPriority::High);
        assert_eq!(job.max_attempts, 5);
        assert_eq!(job.tags.len(), 2);
    }

    #[test]
    fn test_job_scheduling() {
        let payload = serde_json::json!({});
        let scheduled_time = chrono::Utc::now().timestamp() + 3600;
        let job = Job::new("test", payload, "default").schedule_for(scheduled_time);

        assert!(job.is_scheduled());
        assert!(!job.should_execute_now());
    }

    #[test]
    fn test_job_lifecycle() {
        let mut job = Job::new("test", serde_json::json!({}), "default");
        let worker_id = Uuid::new_v4();

        // Start
        job.mark_started(worker_id);
        assert_eq!(job.status, JobStatus::Running);
        assert_eq!(job.worker_id, Some(worker_id));
        assert_eq!(job.attempt, 1);

        // Complete
        job.mark_completed();
        assert_eq!(job.status, JobStatus::Completed);
        assert!(job.completed_at.is_some());
    }

    #[test]
    fn test_job_retry() {
        let mut job = Job::new("test", serde_json::json!({}), "default").with_max_attempts(3);

        // Job must be started (which increments attempt) before it can fail
        job.mark_started(Uuid::new_v4());
        job.mark_failed();

        assert!(job.can_retry());
        assert_eq!(job.attempt, 1);
    }
}
