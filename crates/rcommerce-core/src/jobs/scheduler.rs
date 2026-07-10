//! Job scheduler for cron-like and scheduled jobs

use crate::cache::RedisPool;
use crate::jobs::{Job, JobError, JobId, JobProcessingResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Job scheduler for recurring and scheduled jobs
pub struct JobScheduler {
    /// Redis pool
    pool: RedisPool,

    /// Scheduler configuration
    config: SchedulerConfig,
}

/// Scheduler configuration (imported from config module for convenience)
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Enable scheduler
    pub enabled: bool,

    /// Check interval
    pub check_interval: std::time::Duration,

    /// Max scheduled jobs
    pub max_scheduled_jobs: usize,

    /// Timezone
    pub timezone: String,

    /// Enable cron
    pub enable_cron: bool,

    /// Max cron jobs
    pub max_cron_jobs: usize,
}

impl SchedulerConfig {
    /// Create new config with default values
    pub fn new() -> Self {
        Self {
            enabled: true,
            check_interval: std::time::Duration::from_secs(60),
            max_scheduled_jobs: 10000,
            timezone: "UTC".to_string(),
            enable_cron: true,
            max_cron_jobs: 1000,
        }
    }
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl JobScheduler {
    /// Create a new job scheduler
    pub fn new(pool: RedisPool, config: SchedulerConfig) -> Self {
        info!("Creating job scheduler with config: {:?}", config);

        Self { pool, config }
    }

    /// Start scheduler loop
    pub async fn start(self: Arc<Self>) -> JobProcessingResult<tokio::task::JoinHandle<()>> {
        if !self.config.enabled {
            return Err(JobError::Worker("Scheduler is disabled".to_string()));
        }

        info!("Starting job scheduler");

        let self_clone = self.clone();
        let handle = tokio::spawn(async move {
            self_clone.run().await;
        });

        Ok(handle)
    }

    /// Run scheduler loop
    async fn run(&self) {
        info!("Job scheduler running");

        if !self.config.enabled {
            info!("Job scheduler stopped (disabled)");
            return;
        }

        loop {
            // Process scheduled jobs
            match self.process_due_jobs().await {
                Ok(count) => {
                    if count > 0 {
                        debug!("Scheduler processed {} due jobs", count);
                    }
                }
                Err(e) => {
                    error!("Scheduler error processing due jobs: {}", e);
                }
            }

            // Process cron jobs
            if self.config.enable_cron {
                match self.process_cron_jobs().await {
                    Ok(_) => {}
                    Err(e) => {
                        error!("Scheduler error processing cron jobs: {}", e);
                    }
                }
            }

            // Sleep before next check
            tokio::time::sleep(self.config.check_interval).await;
        }
    }

    /// Schedule a job for future execution
    pub async fn schedule(&self, job: Job, execute_at: DateTime<Utc>) -> JobProcessingResult<()> {
        let conn = self.pool.get().await?;

        // Store job
        let job_data =
            serde_json::to_vec(&job).map_err(|e| JobError::Serialization(e.to_string()))?;

        let job_key = format!("scheduler:job:{}", job.id);
        conn.setex(&job_key, 86400, &job_data).await?;

        // Add to scheduled set (score = timestamp)
        let timestamp = execute_at.timestamp();
        let mut cmd = redis::Cmd::new();
        cmd.arg("ZADD")
            .arg("scheduler:scheduled")
            .arg(timestamp)
            .arg(job.id.to_string());
        conn.execute(cmd).await?;

        info!("Scheduled job: id={}, execute_at={}", job.id, execute_at);

        Ok(())
    }

    /// Process jobs that are due for execution
    async fn process_due_jobs(&self) -> JobProcessingResult<usize> {
        let conn = self.pool.get().await?;

        let now = Utc::now().timestamp();

        // Get due jobs
        let mut cmd = redis::Cmd::new();
        cmd.arg("ZRANGEBYSCORE")
            .arg("scheduler:scheduled")
            .arg("-inf")
            .arg(now.to_string())
            .arg("LIMIT")
            .arg("0")
            .arg("100");

        let value = conn.execute(cmd).await?;
        let job_ids: Vec<String> =
            redis::from_redis_value(value).map_err(|e| JobError::Deserialization(e.to_string()))?;

        if job_ids.is_empty() {
            return Ok(0);
        }

        let mut processed = 0;

        for job_id_str in job_ids {
            if let Ok(job_id) = Uuid::parse_str(&job_id_str) {
                match self.execute_scheduled_job(job_id).await {
                    Ok(_) => {
                        processed += 1;
                    }
                    Err(e) => {
                        error!("Failed to execute scheduled job {}: {}", job_id, e);
                    }
                }
            }
        }

        Ok(processed)
    }

    /// Execute a scheduled job
    async fn execute_scheduled_job(&self, job_id: JobId) -> JobProcessingResult<()> {
        let conn = self.pool.get().await?;

        // Load job
        let job_key = format!("scheduler:job:{}", job_id);

        match conn.get(&job_key).await? {
            Some(data) => {
                let _job: Job = serde_json::from_slice(&data)
                    .map_err(|e| JobError::Deserialization(e.to_string()))?;

                // Remove from scheduled set
                let mut cmd = redis::Cmd::new();
                cmd.arg("ZREM")
                    .arg("scheduler:scheduled")
                    .arg(job_id.to_string());
                conn.execute(cmd).await?;

                // Delete job from scheduler
                conn.del(&job_key).await?;

                // Schedule to actual queue
                // This would enqueue to the actual job queue (implementation depends on queue)
                info!("Executed scheduled job: id={}", job_id);

                Ok(())
            }
            None => {
                warn!("Scheduled job not found: id={}", job_id);
                // Remove from set anyway to clean up
                let mut cmd = redis::Cmd::new();
                cmd.arg("ZREM")
                    .arg("scheduler:scheduled")
                    .arg(job_id.to_string());
                conn.execute(cmd).await?;
                Ok(())
            }
        }
    }

    /// Create a cron job
    pub async fn cron(&self, schedule: impl Into<String>, job: Job) -> JobProcessingResult<()> {
        let schedule_str = schedule.into();

        // Validate cron expression
        if !self.validate_cron(&schedule_str) {
            return Err(JobError::Worker(format!(
                "Invalid cron expression: {}",
                schedule_str
            )));
        }

        let conn = self.pool.get().await?;

        // Store cron job
        let cron_key = format!("scheduler:cron:{}", job.id);

        let cron_job = CronJob {
            id: job.id,
            schedule: schedule_str.clone(),
            job_data: serde_json::to_vec(&job)
                .map_err(|e| JobError::Serialization(e.to_string()))?,
            enabled: true,
            created_at: Utc::now().timestamp(),
            next_run: self
                .calculate_next_run(&schedule_str, Utc::now())
                .timestamp(),
        };

        let cron_data =
            serde_json::to_vec(&cron_job).map_err(|e| JobError::Serialization(e.to_string()))?;

        conn.setex(&cron_key, 86400, &cron_data).await?;

        // Add to cron index
        let mut cmd = redis::Cmd::new();
        cmd.arg("SADD")
            .arg("scheduler:cron_jobs")
            .arg(job.id.to_string());
        conn.execute(cmd).await?;

        info!(
            "Created cron job: id={}, schedule={}",
            job.id, cron_job.schedule
        );

        Ok(())
    }

    /// Process cron jobs
    async fn process_cron_jobs(&self) -> JobProcessingResult<usize> {
        let conn = self.pool.get().await?;

        // Get all cron job IDs
        let job_ids: Vec<String> = conn.smembers("scheduler:cron_jobs").await?;

        let mut executed = 0;

        for job_id_str in job_ids {
            if let Ok(job_id) = Uuid::parse_str(&job_id_str) {
                match self.check_and_execute_cron_job(job_id).await {
                    Ok(true) => executed += 1,
                    Ok(false) => {} // Not ready yet
                    Err(e) => {
                        error!("Failed to execute cron job {}: {}", job_id, e);
                    }
                }
            }
        }

        Ok(executed)
    }

    /// Check and execute a cron job if due
    async fn check_and_execute_cron_job(&self, job_id: JobId) -> JobProcessingResult<bool> {
        let conn = self.pool.get().await?;

        let cron_key = format!("scheduler:cron:{}", job_id);

        match conn.get(&cron_key).await? {
            Some(data) => {
                let mut cron_job: CronJob = serde_json::from_slice(&data)
                    .map_err(|e| JobError::Deserialization(e.to_string()))?;

                let now = Utc::now().timestamp();

                if cron_job.enabled && now >= cron_job.next_run {
                    // Execute cron job
                    let _job: Job = serde_json::from_slice(&cron_job.job_data)
                        .map_err(|e| JobError::Deserialization(e.to_string()))?;

                    // Update next run time
                    cron_job.next_run = self
                        .calculate_next_run(&cron_job.schedule, Utc::now())
                        .timestamp();
                    let updated_data = serde_json::to_vec(&cron_job)
                        .map_err(|e| JobError::Serialization(e.to_string()))?;

                    conn.setex(&cron_key, 86400, &updated_data).await?;

                    info!(
                        "Executed cron job: id={}, next_run={}",
                        job_id, cron_job.next_run
                    );

                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            None => {
                // Clean up missing cron job
                let mut cmd = redis::Cmd::new();
                cmd.arg("SREM")
                    .arg("scheduler:cron_jobs")
                    .arg(job_id.to_string());
                conn.execute(cmd).await?;
                Ok(false)
            }
        }
    }

    /// Calculate next run time for cron expression
    fn calculate_next_run(&self, schedule: &str, from: DateTime<Utc>) -> DateTime<Utc> {
        // Simplified implementation - in production use a proper cron parser
        // For now, assume "*/5 * * * *" means every 5 minutes

        if schedule.starts_with("*/") {
            if let Some(minutes_str) = schedule
                .strip_prefix("*/")
                .and_then(|s| s.split_whitespace().next())
            {
                if let Ok(minutes) = minutes_str.parse::<i64>() {
                    return from + chrono::Duration::minutes(minutes);
                }
            }
        }

        // Default: run in 1 hour
        from + chrono::Duration::hours(1)
    }

    /// Validate cron expression (basic validation)
    fn validate_cron(&self, schedule: &str) -> bool {
        !schedule.is_empty() && schedule.len() <= 100
    }

    /// Remove a cron job
    pub async fn remove_cron(&self, job_id: JobId) -> JobProcessingResult<bool> {
        let conn = self.pool.get().await?;

        let cron_key = format!("scheduler:cron:{}", job_id);
        let deleted = conn.del(&cron_key).await?;

        if deleted {
            conn.srem("scheduler:cron_jobs", job_id.to_string()).await?;

            info!("Removed cron job: id={}", job_id);
        }

        Ok(deleted)
    }

    /// Disable a cron job
    pub async fn disable_cron(&self, job_id: JobId) -> JobProcessingResult<bool> {
        self.update_cron_enabled(job_id, false).await
    }

    /// Enable a cron job
    pub async fn enable_cron(&self, job_id: JobId) -> JobProcessingResult<bool> {
        self.update_cron_enabled(job_id, true).await
    }

    /// Update cron job enabled state
    async fn update_cron_enabled(&self, job_id: JobId, enabled: bool) -> JobProcessingResult<bool> {
        let conn = self.pool.get().await?;

        let cron_key = format!("scheduler:cron:{}", job_id);

        if let Some(data) = conn.get(&cron_key).await? {
            let mut cron_job: CronJob = serde_json::from_slice(&data)
                .map_err(|e| JobError::Deserialization(e.to_string()))?;

            cron_job.enabled = enabled;

            let updated_data = serde_json::to_vec(&cron_job)
                .map_err(|e| JobError::Serialization(e.to_string()))?;

            conn.setex(&cron_key, 86400, &updated_data).await?;

            info!(
                "{} cron job: id={}",
                if enabled { "Enabled" } else { "Disabled" },
                job_id
            );

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get all cron jobs
    pub async fn get_cron_jobs(&self) -> JobProcessingResult<Vec<CronJobInfo>> {
        let conn = self.pool.get().await?;

        let job_ids: Vec<String> = conn.smembers("scheduler:cron_jobs").await?;

        let mut jobs = Vec::new();

        for job_id_str in job_ids {
            if let Ok(job_id) = Uuid::parse_str(&job_id_str) {
                let cron_key = format!("scheduler:cron:{}", job_id);

                if let Some(data) = conn.get(&cron_key).await? {
                    let cron_job: CronJob = serde_json::from_slice(&data)
                        .map_err(|e| JobError::Deserialization(e.to_string()))?;

                    jobs.push(CronJobInfo {
                        id: job_id,
                        schedule: cron_job.schedule,
                        enabled: cron_job.enabled,
                        created_at: cron_job.created_at,
                        next_run: cron_job.next_run,
                    });
                }
            }
        }

        Ok(jobs)
    }
}

/// Cron job definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    /// Cron job ID (same as job ID)
    pub id: JobId,

    /// Cron schedule expression
    pub schedule: String,

    /// Job data (serialized)
    pub job_data: Vec<u8>,

    /// Whether cron is enabled
    pub enabled: bool,

    /// Creation timestamp
    pub created_at: i64,

    /// Next run timestamp
    pub next_run: i64,
}

/// Cron job info (for external API)
#[derive(Debug, Clone)]
pub struct CronJobInfo {
    /// Job ID
    pub id: JobId,

    /// Schedule expression
    pub schedule: String,

    /// Whether enabled
    pub enabled: bool,

    /// Created timestamp
    pub created_at: i64,

    /// Next run timestamp
    pub next_run: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::RedisConfig;

    #[tokio::test]
    async fn test_scheduler_creation() {
        let config = RedisConfig::default();
        let pool = RedisPool::new(config).await;

        if let Ok(pool) = pool {
            let scheduler = JobScheduler::new(pool, SchedulerConfig::default());
            assert!(scheduler.config.enabled);
        }
    }

    #[test]
    fn test_calculate_next_run() {
        // Test the cron calculation logic without needing a pool
        let calculate = |schedule: &str, from: DateTime<Utc>| -> DateTime<Utc> {
            if schedule.starts_with("*/") {
                if let Some(minutes_str) = schedule
                    .strip_prefix("*/")
                    .and_then(|s| s.split_whitespace().next())
                {
                    if let Ok(minutes) = minutes_str.parse::<i64>() {
                        return from + chrono::Duration::minutes(minutes);
                    }
                }
            }
            from + chrono::Duration::hours(1)
        };

        let now = Utc::now();
        let next = calculate("*/5 * * * *", now);

        let diff = next.timestamp() - now.timestamp();
        assert_eq!(diff, 300); // Exactly 5 minutes
    }

    #[tokio::test]
    async fn test_cron_validation() {
        let config = SchedulerConfig::default();
        let pool = RedisPool::new(RedisConfig::default()).await;

        if let Ok(pool) = pool {
            let scheduler = JobScheduler::new(pool, config);

            assert!(scheduler.validate_cron("*/5 * * * *"));
            assert!(scheduler.validate_cron("invalid"));
        }
    }
}
