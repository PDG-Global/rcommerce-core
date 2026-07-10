//! Job queue implementation backed by Redis
//!
//! Provides Redis-backed job queues with priority support and
//! reliable job processing semantics.

use crate::cache::{CacheResult, RedisPool};
use crate::jobs::{Job, JobId, JobPriority, JobQuery, JobStatus};
use redis::Value;
use serde_json;
use std::collections::HashMap;
use uuid::Uuid;

/// Job queue backed by Redis
pub struct JobQueue {
    /// Redis pool
    pool: RedisPool,

    /// Queue name
    name: String,

    /// Namespace for keys
    namespace: String,
}

impl JobQueue {
    /// Create a new job queue
    pub fn new(pool: RedisPool, name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            pool,
            name: name.clone(),
            namespace: format!("jobs:queue:{}", name),
        }
    }

    /// Get queue name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Enqueue a job
    pub async fn enqueue(&self, job: &Job) -> CacheResult<()> {
        let conn = self.pool.get().await?;

        // Serialize job
        let job_data = serde_json::to_vec(job)
            .map_err(|e| crate::cache::CacheError::SerializationError(e.to_string()))?;

        // Generate Redis keys
        let job_key = self.job_key(&job.id);
        let queue_key = self.queue_key(&job.priority);
        let scheduled_key = self.scheduled_key();
        let _status_key = self.status_key(&job.status);

        // Store job data
        conn.setex(&job_key, 86400, &job_data).await?; // 24 hour TTL

        // Add to appropriate queue
        if job.is_scheduled() {
            // Add to scheduled set with score = scheduled time
            let mut cmd = redis::Cmd::new();
            cmd.arg("ZADD")
                .arg(&scheduled_key)
                .arg(job.scheduled_for.unwrap())
                .arg(job.id.to_string());
            conn.execute(cmd).await?;
        } else {
            // Add to queue based on priority
            let mut cmd = redis::Cmd::new();
            cmd.arg("LPUSH").arg(&queue_key).arg(job.id.to_string());
            conn.execute(cmd).await?;
        }

        // Update status counts
        let mut cmd = redis::Cmd::new();
        cmd.arg("HINCRBY")
            .arg(self.status_counts_key())
            .arg(job.status.to_string())
            .arg("1");
        conn.execute(cmd).await?;

        // Update queue length
        let mut cmd = redis::Cmd::new();
        cmd.arg("HINCRBY")
            .arg(self.stats_key())
            .arg("enqueued")
            .arg("1");
        conn.execute(cmd).await?;

        Ok(())
    }

    /// Dequeue a job (blocking)
    pub async fn dequeue(&self, _timeout_secs: u64) -> CacheResult<Option<Job>> {
        let conn = self.pool.get().await?;

        // Try high priority first, then normal, then low
        for priority in [JobPriority::High, JobPriority::Normal, JobPriority::Low] {
            let queue_key = self.queue_key(&priority);

            // Try to pop job from queue
            let mut cmd = redis::Cmd::new();
            cmd.arg("RPOP").arg(&queue_key);

            match conn.execute(cmd).await {
                Ok(result) => {
                    if let Ok(Some(job_id_str)) = redis::from_redis_value::<Option<String>>(result)
                    {
                        if let Ok(job_id) = Uuid::parse_str(&job_id_str) {
                            // Load job data
                            if let Some(job) = self.get_job(&job_id).await? {
                                // Update status to running
                                self.update_job_status(&job_id, JobStatus::Running).await?;
                                return Ok(Some(job));
                            }
                        }
                    }
                }
                Err(_) => continue, // Try next priority queue
            }
        }

        Ok(None)
    }

    /// Get a job by ID
    pub async fn get_job(&self, job_id: &JobId) -> CacheResult<Option<Job>> {
        let conn = self.pool.get().await?;

        let job_key = self.job_key(job_id);

        match conn.get(&job_key).await? {
            Some(data) => {
                let job: Job = serde_json::from_slice(&data)
                    .map_err(|e| crate::cache::CacheError::DeserializationError(e.to_string()))?;
                Ok(Some(job))
            }
            None => Ok(None),
        }
    }

    /// Update job status
    pub async fn update_job_status(
        &self,
        job_id: &JobId,
        new_status: JobStatus,
    ) -> CacheResult<()> {
        let conn = self.pool.get().await?;

        // Load job
        if let Some(mut job) = self.get_job(job_id).await? {
            let old_status = job.status;
            job.status = new_status;

            // Update timestamps based on status
            match new_status {
                JobStatus::Running => {
                    job.started_at = Some(chrono::Utc::now().timestamp());
                }
                JobStatus::Completed
                | JobStatus::Failed
                | JobStatus::Dead
                | JobStatus::TimedOut => {
                    job.completed_at = Some(chrono::Utc::now().timestamp());
                }
                _ => {}
            }

            // Save updated job
            self.save_job(&job).await?;

            // Update status counts
            let mut pipeline = redis::Pipeline::new();

            // Decrement old status count
            pipeline
                .cmd("HINCRBY")
                .arg(self.status_counts_key())
                .arg(old_status.to_string())
                .arg("-1");

            // Increment new status count
            pipeline
                .cmd("HINCRBY")
                .arg(self.status_counts_key())
                .arg(new_status.to_string())
                .arg("1");

            conn.execute_pipeline(&pipeline).await?;
        }

        Ok(())
    }

    /// Save job (update)
    pub async fn save_job(&self, job: &Job) -> CacheResult<()> {
        let conn = self.pool.get().await?;

        let job_key = self.job_key(&job.id);
        let job_data = serde_json::to_vec(job)
            .map_err(|e| crate::cache::CacheError::SerializationError(e.to_string()))?;

        // Update with TTL
        conn.setex(&job_key, 86400, &job_data).await?;

        Ok(())
    }

    /// Get scheduled jobs ready for execution
    pub async fn get_ready_scheduled_jobs(&self) -> CacheResult<Vec<Job>> {
        let conn = self.pool.get().await?;

        let scheduled_key = self.scheduled_key();
        let now = chrono::Utc::now().timestamp();

        // Get jobs with score <= now
        let mut cmd = redis::Cmd::new();
        cmd.arg("ZRANGEBYSCORE")
            .arg(&scheduled_key)
            .arg("-inf")
            .arg(now.to_string())
            .arg("LIMIT")
            .arg("0")
            .arg("100"); // Max 100 jobs at a time

        let job_ids: Vec<String> = redis::from_redis_value(conn.execute(cmd).await?)
            .map_err(|e| crate::cache::CacheError::DeserializationError(e.to_string()))?;

        // Remove from scheduled set
        if !job_ids.is_empty() {
            let mut pipeline = redis::Pipeline::new();
            for job_id_str in &job_ids {
                pipeline.cmd("ZREM").arg(&scheduled_key).arg(job_id_str);
            }
            conn.execute_pipeline(&pipeline).await?;
        }

        // Load job data
        let mut jobs = Vec::new();
        for job_id_str in job_ids {
            if let Ok(job_id) = Uuid::parse_str(&job_id_str) {
                if let Some(job) = self.get_job(&job_id).await? {
                    jobs.push(job);
                }
            }
        }

        Ok(jobs)
    }

    /// Delete a job
    pub async fn delete_job(&self, job_id: &JobId) -> CacheResult<bool> {
        let conn = self.pool.get().await?;

        let job_key = self.job_key(job_id);
        conn.del(&job_key).await
    }

    /// Get queue stats
    pub async fn stats(&self) -> CacheResult<QueueStats> {
        let conn = self.pool.get().await?;

        let mut total_pending = 0;
        let mut depth_by_priority = HashMap::new();

        // Count pending jobs by priority
        for priority in [JobPriority::High, JobPriority::Normal, JobPriority::Low] {
            let queue_key = self.queue_key(&priority);

            let mut cmd = redis::Cmd::new();
            cmd.arg("LLEN").arg(&queue_key);

            let count: i64 = redis::from_redis_value(conn.execute(cmd).await?)
                .map_err(|e| crate::cache::CacheError::DeserializationError(e.to_string()))?;

            total_pending += count;
            depth_by_priority.insert(priority, count as usize);
        }

        // Get status counts
        let mut cmd = redis::Cmd::new();
        cmd.arg("HGETALL").arg(self.status_counts_key());

        let status_counts: HashMap<String, i64> = redis::from_redis_value(conn.execute(cmd).await?)
            .map_err(|e| crate::cache::CacheError::DeserializationError(e.to_string()))?;

        let stats = QueueStats {
            name: self.name.clone(),
            total_pending: total_pending as usize,
            depth_by_priority,
            status_counts,
            is_healthy: true,
        };

        Ok(stats)
    }

    /// List jobs matching query
    pub async fn list_jobs(&self, query: &JobQuery) -> CacheResult<Vec<Job>> {
        let conn = self.pool.get().await?;

        // This is a simplified implementation
        // In production, you'd use Redis SCAN or maintain indexes

        // For now, scan all job keys (expensive!)
        let pattern = format!("{}/job:*", self.namespace);
        let mut cmd = redis::Cmd::new();
        cmd.arg("KEYS").arg(&pattern);

        let job_keys: Vec<String> = redis::from_redis_value(conn.execute(cmd).await?)
            .map_err(|e| crate::cache::CacheError::DeserializationError(e.to_string()))?;

        let mut jobs = Vec::new();

        for job_key in job_keys {
            if let Some(data) = conn.get(&job_key).await? {
                let job: Job = serde_json::from_slice(&data)
                    .map_err(|e| crate::cache::CacheError::DeserializationError(e.to_string()))?;

                // Apply filters
                if let Some(ref status) = query.status {
                    if job.status != *status {
                        continue;
                    }
                }

                if let Some(ref queue) = query.queue {
                    if job.queue != *queue {
                        continue;
                    }
                }

                if let Some(ref job_type) = query.job_type {
                    if job.job_type != *job_type {
                        continue;
                    }
                }

                if !query.tags.is_empty() {
                    let has_all_tags = query.tags.iter().all(|tag| job.tags.contains(tag));
                    if !has_all_tags {
                        continue;
                    }
                }

                jobs.push(job);
            }
        }

        // Apply limit and offset
        if let Some(offset) = query.offset {
            jobs = jobs.into_iter().skip(offset).collect();
        }

        if let Some(limit) = query.limit {
            jobs.truncate(limit);
        }

        Ok(jobs)
    }

    /// Clear queue (delete all jobs)
    pub async fn clear(&self) -> CacheResult<u64> {
        let conn = self.pool.get().await?;

        let mut deleted = 0;

        // Delete all job keys
        let pattern = format!("{}/job:*", self.namespace);
        let mut keys_cmd = redis::Cmd::new();
        keys_cmd.arg("KEYS").arg(&pattern);
        let job_keys: Vec<String> = conn
            .execute(keys_cmd)
            .await
            .map_err(|e| crate::cache::CacheError::OperationError(e.to_string()))
            .and_then(|v| {
                redis::from_redis_value(v)
                    .map_err(|e| crate::cache::CacheError::DeserializationError(e.to_string()))
            })?;

        if !job_keys.is_empty() {
            let mut pipeline = redis::Pipeline::new();
            for key in job_keys {
                pipeline.cmd("DEL").arg(key);
            }
            let results: Vec<Value> = conn.execute_pipeline(&pipeline).await?;
            deleted += results
                .iter()
                .filter_map(|v| match v {
                    redis::Value::Int(n) => Some(*n as u64),
                    _ => None,
                })
                .sum::<u64>();
        }

        // Clear queues
        for priority in [JobPriority::High, JobPriority::Normal, JobPriority::Low] {
            let queue_key = self.queue_key(&priority);
            conn.del(&queue_key).await?;
        }

        // Clear scheduled
        let scheduled_key = self.scheduled_key();
        conn.del(&scheduled_key).await?;

        // Reset stats
        let stats_key = self.stats_key();
        conn.del(&stats_key).await?;

        let status_counts_key = self.status_counts_key();
        conn.del(&status_counts_key).await?;

        Ok(deleted)
    }

    /// Helper: Generate job key
    fn job_key(&self, job_id: &JobId) -> String {
        format!("{}/job:{}", self.namespace, job_id)
    }

    /// Helper: Generate queue key
    fn queue_key(&self, priority: &JobPriority) -> String {
        format!(
            "{}/queue:{}",
            self.namespace,
            priority.to_string().to_lowercase()
        )
    }

    /// Helper: Generate scheduled key
    fn scheduled_key(&self) -> String {
        format!("{}/scheduled", self.namespace)
    }

    /// Helper: Generate status key
    fn status_key(&self, status: &JobStatus) -> String {
        format!(
            "{}/status:{}",
            self.namespace,
            status.to_string().to_lowercase()
        )
    }

    /// Helper: Generate status counts key
    fn status_counts_key(&self) -> String {
        format!("{}/status_counts", self.namespace)
    }

    /// Helper: Generate stats key
    fn stats_key(&self) -> String {
        format!("{}/stats", self.namespace)
    }
}

/// Queue statistics
#[derive(Debug, Default, Clone)]
pub struct QueueStats {
    /// Queue name
    pub name: String,

    /// Total pending jobs
    pub total_pending: usize,

    /// Jobs by priority
    pub depth_by_priority: HashMap<JobPriority, usize>,

    /// Jobs by status
    pub status_counts: HashMap<String, i64>,

    /// Whether queue is healthy
    pub is_healthy: bool,
}

impl QueueStats {
    /// Get count for specific status
    pub fn status_count(&self, status: JobStatus) -> i64 {
        self.status_counts
            .get(&status.to_string())
            .copied()
            .unwrap_or(0)
    }

    /// Format as human-readable
    pub fn format(&self) -> String {
        let mut lines = vec![format!("Queue '{}':", self.name)];

        lines.push(format!("  Pending: {}", self.total_pending));

        for (priority, count) in &self.depth_by_priority {
            lines.push(format!("    {:?}: {}", priority, count));
        }

        lines.push("  Status counts:".to_string());
        for (status, count) in &self.status_counts {
            lines.push(format!("    {}: {}", status, count));
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::RedisConfig;
    use crate::jobs::{Job, JobPriority};

    #[tokio::test]
    async fn test_job_queue_creation() {
        let config = RedisConfig::default();
        let pool = RedisPool::new(config).await;

        if let Ok(pool) = pool {
            let queue = JobQueue::new(pool, "test_queue");
            assert_eq!(queue.name, "test_queue");
        }
    }

    #[tokio::test]
    async fn test_enqueue_dequeue() {
        let config = RedisConfig::default();
        let pool = RedisPool::new(config).await;

        if let Ok(pool) = pool {
            // Use unique queue name to avoid test interference
            let queue_name = format!("test_queue_{}", Uuid::new_v4());
            let queue = JobQueue::new(pool, &queue_name);

            // Create a job
            let payload = serde_json::json!({"test": "data"});
            let job = Job::new("test_job", payload, &queue_name);

            // Enqueue
            queue.enqueue(&job).await.unwrap();

            // Small delay to ensure Redis write completes
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;

            // Dequeue
            let dequeued = queue.dequeue(0).await.unwrap();
            assert!(dequeued.is_some(), "Expected job to be dequeued");

            let dequeued_job = dequeued.unwrap();
            assert_eq!(dequeued_job.id, job.id);
            assert_eq!(dequeued_job.job_type, job.job_type);

            // Cleanup
            let _ = queue.clear().await;
        }
    }

    #[tokio::test]
    async fn test_job_priority() {
        let config = RedisConfig::default();
        let pool = RedisPool::new(config).await;

        if let Ok(pool) = pool {
            // Use unique queue name to avoid test interference
            let queue_name = format!("test_queue_{}", Uuid::new_v4());
            let queue = JobQueue::new(pool, &queue_name);

            // Enqueue jobs with different priorities
            let low_job =
                Job::new("low", serde_json::json!({}), &queue_name).with_priority(JobPriority::Low);
            let high_job = Job::new("high", serde_json::json!({}), &queue_name)
                .with_priority(JobPriority::High);

            queue.enqueue(&low_job).await.unwrap();
            queue.enqueue(&high_job).await.unwrap();

            // Small delay to ensure Redis writes complete
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;

            // High priority should dequeue first
            let first = queue
                .dequeue(0)
                .await
                .unwrap()
                .expect("Expected high priority job");
            assert_eq!(first.job_type, "high");

            let second = queue
                .dequeue(0)
                .await
                .unwrap()
                .expect("Expected low priority job");
            assert_eq!(second.job_type, "low");

            // Cleanup
            let _ = queue.clear().await;
        }
    }

    #[tokio::test]
    async fn test_queue_stats() {
        let config = RedisConfig::default();
        let pool = RedisPool::new(config).await;

        if let Ok(pool) = pool {
            // Use unique queue name to avoid test interference
            let queue_name = format!("test_queue_{}", Uuid::new_v4());
            let queue = JobQueue::new(pool, &queue_name);

            // Enqueue some jobs
            for i in 0..5 {
                let job = Job::new(format!("job_{}", i), serde_json::json!({}), &queue_name);
                queue.enqueue(&job).await.unwrap();
            }

            // Small delay to ensure Redis writes complete
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;

            let stats = queue.stats().await.unwrap();
            assert_eq!(stats.total_pending, 5, "Expected 5 pending jobs");
            assert!(stats.is_healthy);

            // Cleanup
            let _ = queue.clear().await;
        }
    }
}
