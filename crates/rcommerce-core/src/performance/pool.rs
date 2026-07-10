//! Connection pooling optimization and monitoring

use crate::cache::{CacheResult, RedisPool};

/// Database connection pool optimizer
pub struct PoolOptimizer {
    /// Redis pool (for monitoring, if needed)
    #[allow(dead_code)]
    redis_pool: Option<RedisPool>,

    /// Optimization statistics
    #[allow(dead_code)]
    stats: PoolOptimizationStats,
}

impl PoolOptimizer {
    /// Create new pool optimizer
    pub fn new(redis_pool: Option<RedisPool>) -> Self {
        Self {
            redis_pool,
            stats: PoolOptimizationStats::default(),
        }
    }

    /// Analyze pool performance and recommend optimizations
    pub async fn analyze_pool(&mut self, pool_stats: PoolStats) -> CacheResult<PoolOptimization> {
        let mut recommendations = Vec::new();

        // Check pool utilization
        let utilization_rate =
            pool_stats.active_connections as f64 / pool_stats.max_connections as f64;

        if utilization_rate > 0.9 {
            recommendations.push(PoolRecommendation::IncreasePoolSize {
                current: pool_stats.max_connections,
                recommended: (pool_stats.max_connections as f64 * 1.5) as usize,
                reason: "High pool utilization".to_string(),
            });
        } else if utilization_rate < 0.3 {
            recommendations.push(PoolRecommendation::DecreasePoolSize {
                current: pool_stats.max_connections,
                recommended: (pool_stats.max_connections as f64 * 0.7) as usize,
                reason: "Low pool utilization".to_string(),
            });
        }

        // Check wait time
        if let Some(avg_wait_ms) = pool_stats.avg_wait_time_ms {
            if avg_wait_ms > 100 {
                recommendations.push(PoolRecommendation::IncreasePoolSize {
                    current: pool_stats.max_connections,
                    recommended: pool_stats.max_connections + 10,
                    reason: format!("High wait time: {}ms", avg_wait_ms),
                });
            }
        }

        // Check idle connections
        if pool_stats.idle_connections > pool_stats.max_connections / 2 {
            recommendations.push(PoolRecommendation::DecreasePoolSize {
                current: pool_stats.max_connections,
                recommended: pool_stats.max_connections - (pool_stats.idle_connections / 2),
                reason: "Too many idle connections".to_string(),
            });
        }

        Ok(PoolOptimization {
            recommendations,
            current_stats: pool_stats,
        })
    }

    /// Optimize pool configuration
    pub fn optimize_config(&self, current_config: PoolConfig) -> PoolConfig {
        let mut optimized = current_config;

        // Increase pool size for high-traffic scenarios
        if optimized.pool_size < 20 {
            optimized.pool_size = (optimized.pool_size * 2).min(50);
        }

        // Reduce connection timeout for faster failure detection
        if optimized.connect_timeout_ms > 5000 {
            optimized.connect_timeout_ms = 3000;
        }

        // Enable connection keepalive
        optimized.enable_keepalive = true;
        optimized.keepalive_interval_ms = Some(30000);

        optimized
    }
}

/// Pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Pool name
    pub name: String,

    /// Active connections
    pub active_connections: usize,

    /// Idle connections
    pub idle_connections: usize,

    /// Max connections
    pub max_connections: usize,

    /// Average wait time for connection (ms)
    pub avg_wait_time_ms: Option<u64>,

    /// Total connections created
    pub total_connections: u64,

    /// Total connection errors
    pub total_errors: u64,
}

impl PoolStats {
    /// Calculate utilization rate
    pub fn utilization_rate(&self) -> f64 {
        self.active_connections as f64 / self.max_connections as f64
    }

    /// Calculate error rate
    pub fn error_rate(&self) -> f64 {
        if self.total_connections == 0 {
            0.0
        } else {
            self.total_errors as f64 / self.total_connections as f64
        }
    }
}

impl Default for PoolStats {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            active_connections: 0,
            idle_connections: 0,
            max_connections: 10,
            avg_wait_time_ms: None,
            total_connections: 0,
            total_errors: 0,
        }
    }
}

/// Pool configuration
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Pool size
    pub pool_size: usize,

    /// Connection timeout (ms)
    pub connect_timeout_ms: u64,

    /// Max lifetime (ms)
    pub max_lifetime_ms: Option<u64>,

    /// Enable keepalive
    pub enable_keepalive: bool,

    /// Keepalive interval (ms)
    pub keepalive_interval_ms: Option<u64>,

    /// Retry on failure
    pub retry_on_failure: bool,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            pool_size: 20,
            connect_timeout_ms: 5000,
            max_lifetime_ms: Some(3600000), // 1 hour
            enable_keepalive: true,
            keepalive_interval_ms: Some(30000),
            retry_on_failure: true,
        }
    }
}

/// Pool optimization recommendations
#[derive(Debug, Clone)]
pub struct PoolOptimization {
    /// Recommendations
    pub recommendations: Vec<PoolRecommendation>,

    /// Current statistics
    pub current_stats: PoolStats,
}

/// Pool optimization recommendations
#[derive(Debug, Clone)]
pub enum PoolRecommendation {
    /// Increase pool size
    IncreasePoolSize {
        current: usize,
        recommended: usize,
        reason: String,
    },

    /// Decrease pool size
    DecreasePoolSize {
        current: usize,
        recommended: usize,
        reason: String,
    },

    /// Enable connection keepalive
    EnableKeepalive { interval_ms: u64, reason: String },

    /// Adjust connection timeout
    AdjustTimeout {
        current_ms: u64,
        recommended_ms: u64,
        reason: String,
    },

    /// Enable retry on failure
    EnableRetry { reason: String },
}

/// Pool optimization statistics
#[derive(Debug, Default)]
pub struct PoolOptimizationStats {
    /// Total optimizations applied
    pub optimizations_applied: u64,

    /// Average pool utilization improvement
    pub avg_utilization_improvement: f64,

    /// Wait time reductions (ms)
    pub wait_time_reductions_ms: Vec<u64>,
}
