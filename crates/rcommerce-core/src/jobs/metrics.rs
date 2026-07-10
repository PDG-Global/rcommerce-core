//! Job processing metrics and monitoring

use crate::cache::{CacheResult, RedisPool};
use crate::jobs::{JobId, JobStatus};
use std::collections::HashMap;
use tracing::info;

/// Job metrics collector
pub struct JobMetrics {
    /// Redis pool
    pool: RedisPool,
}

impl JobMetrics {
    /// Create new metrics collector
    pub fn new(pool: RedisPool) -> Self {
        Self { pool }
    }

    /// Record job completion
    pub async fn record_completion(
        &self,
        job_id: JobId,
        status: JobStatus,
        duration_ms: u64,
    ) -> CacheResult<()> {
        let conn = self.pool.get().await?;

        // Increment counter
        let key = format!("metrics:jobs:{}", status.to_string().to_lowercase());
        conn.incr(&key).await?;

        // Record latency
        let lat_key = format!("metrics:latency:{}", status.to_string().to_lowercase());
        conn.lpush(&lat_key, duration_ms.to_string()).await?;

        info!(
            "Recorded job completion: id={}, status={}, duration={}ms",
            job_id, status, duration_ms
        );

        Ok(())
    }

    /// Get metrics summary
    pub async fn get_summary(&self) -> CacheResult<MetricsSummary> {
        let conn = self.pool.get().await?;

        // Get status counts
        let mut counts = HashMap::new();
        for status in [
            JobStatus::Pending,
            JobStatus::Running,
            JobStatus::Completed,
            JobStatus::Failed,
            JobStatus::Dead,
        ] {
            let key = format!("metrics:jobs:{}", status.to_string().to_lowercase());
            let count: i64 = if let Some(data) = conn.get(&key).await? {
                String::from_utf8_lossy(&data).parse().unwrap_or(0)
            } else {
                0
            };
            counts.insert(status, count);
        }

        // Get latency stats
        let lat_key = "metrics:latency:completed";
        let lat_data: Vec<u8> = conn.get(lat_key).await?.unwrap_or_default();

        let latencies: Vec<u64> = String::from_utf8_lossy(&lat_data)
            .split(',')
            .filter_map(|s| s.parse().ok())
            .collect();

        let avg_latency = if !latencies.is_empty() {
            latencies.iter().sum::<u64>() / latencies.len() as u64
        } else {
            0
        };

        // Calculate total processed before moving counts
        let total_processed = counts.get(&JobStatus::Completed).copied().unwrap_or(0)
            + counts.get(&JobStatus::Failed).copied().unwrap_or(0)
            + counts.get(&JobStatus::Dead).copied().unwrap_or(0);

        Ok(MetricsSummary {
            job_counts: counts,
            average_latency_ms: avg_latency,
            total_processed,
        })
    }
}

/// Metrics summary
#[derive(Debug, Default)]
pub struct MetricsSummary {
    /// Job counts by status
    pub job_counts: HashMap<JobStatus, i64>,

    /// Average latency in milliseconds
    pub average_latency_ms: u64,

    /// Total jobs processed
    pub total_processed: i64,
}
