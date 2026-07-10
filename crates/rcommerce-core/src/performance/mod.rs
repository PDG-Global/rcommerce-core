//! Performance optimization and monitoring module
//!
//! This module provides performance optimization strategies including:
//! - Caching strategies (LRU, TTL-based)
//! - Query optimization and result caching
//! - Connection pooling optimization
//! - Memory usage profiling
//! - Performance benchmarking
//! - Load testing utilities
//! - Resource monitoring

pub mod benchmark;
pub mod cache;
pub mod monitor;
pub mod optimizer;
pub mod pool;
pub mod profiler;
pub mod query;

// Re-export main types
pub use benchmark::{Benchmark, BenchmarkResult};
pub use cache::{CacheStats, CacheStrategy, LruCache, TtlCache};
pub use monitor::{ResourceMonitor, SystemMetrics};
pub use optimizer::{OptimizationRecommendation, PerformanceOptimizer};
pub use pool::{PoolOptimizer, PoolStats};
pub use profiler::{MemoryProfiler, PerformanceProfile};
pub use query::{CachedQueryResult, QueryCache};

/// Performance result type
pub type PerformanceResult<T> = Result<T, PerformanceError>;

/// Performance-related errors
#[derive(Debug, thiserror::Error)]
pub enum PerformanceError {
    #[error("Cache error: {0}")]
    CacheError(String),

    #[error("Profiler error: {0}")]
    ProfilerError(String),

    #[error("Benchmark error: {0}")]
    BenchmarkError(String),

    #[error("Monitoring error: {0}")]
    MonitorError(String),
}

impl From<crate::cache::CacheError> for PerformanceError {
    fn from(err: crate::cache::CacheError) -> Self {
        PerformanceError::CacheError(err.to_string())
    }
}

/// Performance metrics
#[derive(Debug, Clone, Default)]
pub struct PerformanceMetrics {
    /// Requests per second
    pub requests_per_second: f64,

    /// Average latency (ms)
    pub avg_latency_ms: f64,

    /// Memory usage (MB)
    pub memory_usage_mb: f64,

    /// CPU usage (%)
    pub cpu_usage_percent: f64,

    /// Cache hit rate
    pub cache_hit_rate: f64,

    /// Database query time (ms)
    pub db_query_time_ms: f64,
}

impl PerformanceMetrics {
    /// Calculate performance score (0-100)
    pub fn calculate_score(&self) -> f64 {
        let latency_score = (100.0 - self.avg_latency_ms).max(0.0);
        let throughput_score = self.requests_per_second.min(100.0);
        let cache_score = self.cache_hit_rate * 100.0;

        (latency_score + throughput_score + cache_score) / 3.0
    }

    /// Check if metrics are acceptable
    pub fn is_healthy(&self) -> bool {
        self.avg_latency_ms < 100.0 && self.cache_hit_rate > 0.8 && self.cpu_usage_percent < 80.0
    }
}
