//! Performance optimizer with automatic recommendations

use crate::performance::{CacheStats, PerformanceMetrics, PoolStats};

/// Performance optimizer
pub struct PerformanceOptimizer {
    /// Optimization thresholds
    thresholds: OptimizationThresholds,
}

impl PerformanceOptimizer {
    /// Create new optimizer with default thresholds
    pub fn new() -> Self {
        Self {
            thresholds: OptimizationThresholds::default(),
        }
    }

    /// Analyze performance and generate recommendations
    pub fn analyze(&self, metrics: &PerformanceMetrics) -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();

        // Check latency
        if metrics.avg_latency_ms > self.thresholds.max_latency_ms {
            recommendations.push(OptimizationRecommendation::ReduceLatency {
                current_ms: metrics.avg_latency_ms,
                target_ms: self.thresholds.target_latency_ms,
                actions: vec![
                    "Implement query result caching".to_string(),
                    "Optimize database queries".to_string(),
                    "Enable connection pooling".to_string(),
                ],
            });
        }

        // Check cache hit rate
        if metrics.cache_hit_rate < self.thresholds.min_cache_hit_rate {
            recommendations.push(OptimizationRecommendation::ImproveCacheHitRate {
                current_rate: metrics.cache_hit_rate,
                target_rate: self.thresholds.target_cache_hit_rate,
                actions: vec![
                    "Increase cache size".to_string(),
                    "Optimize cache TTL".to_string(),
                    "Implement cache warming".to_string(),
                ],
            });
        }

        // Check CPU usage
        if metrics.cpu_usage_percent > self.thresholds.max_cpu_percent {
            recommendations.push(OptimizationRecommendation::ReduceCpuUsage {
                current_percent: metrics.cpu_usage_percent,
                target_percent: self.thresholds.target_cpu_percent,
                actions: vec![
                    "Optimize hot code paths".to_string(),
                    "Implement result caching".to_string(),
                    "Add worker pool scaling".to_string(),
                ],
            });
        }

        // Check memory usage
        if metrics.memory_usage_mb > self.thresholds.max_memory_mb {
            recommendations.push(OptimizationRecommendation::ReduceMemoryUsage {
                current_mb: metrics.memory_usage_mb,
                target_mb: self.thresholds.target_memory_mb,
                actions: vec![
                    "Optimize data structures".to_string(),
                    "Implement streaming responses".to_string(),
                    "Add connection pooling limits".to_string(),
                ],
            });
        }

        recommendations
    }

    /// Optimize cache configuration
    pub fn optimize_cache(&self, stats: &CacheStats) -> CacheOptimization {
        let mut config_changes = Vec::new();

        if stats.hit_rate < 0.5 {
            config_changes.push("Increase cache size by 50%".to_string());
        }

        if stats.size > 1000 {
            config_changes.push("Implement cache sharding".to_string());
        }

        CacheOptimization {
            current_stats: stats.clone(),
            recommended_changes: config_changes,
            expected_improvement: "15-25% hit rate improvement".to_string(),
        }
    }

    /// Optimize pool configuration
    pub fn optimize_pool(&self, stats: &PoolStats) -> PoolOptimization {
        let mut config_changes = Vec::new();

        if stats.utilization_rate() > 0.9 {
            config_changes.push(format!(
                "Increase pool size from {} to {}",
                stats.max_connections,
                stats.max_connections * 2
            ));
        }

        if stats.active_connections < 5 {
            config_changes.push("Decrease pool size to save resources".to_string());
        }

        PoolOptimization {
            current_stats: stats.clone(),
            recommended_changes: config_changes,
            expected_improvement: "Better resource utilization".to_string(),
        }
    }

    /// Generate comprehensive optimization report
    pub fn generate_report(&self, metrics: &PerformanceMetrics) -> OptimizationReport {
        let recommendations = self.analyze(metrics);
        let score = metrics.calculate_score();
        let health = metrics.is_healthy();

        OptimizationReport {
            performance_score: score,
            is_healthy: health,
            priority_actions: self.get_priority_actions(&recommendations),
            recommendations,
        }
    }

    /// Get priority actions based on recommendations
    fn get_priority_actions(&self, recommendations: &[OptimizationRecommendation]) -> Vec<String> {
        let mut result_actions = Vec::new();

        for rec in recommendations {
            match rec {
                OptimizationRecommendation::ReduceLatency {
                    actions: rec_actions,
                    ..
                } => {
                    result_actions.extend(rec_actions.iter().take(2).cloned());
                }
                OptimizationRecommendation::ImproveCacheHitRate {
                    actions: rec_actions,
                    ..
                } => {
                    result_actions.extend(rec_actions.iter().take(1).cloned());
                }
                _ => {}
            }
        }

        result_actions
    }
}

impl Default for PerformanceOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Optimization thresholds
#[derive(Debug, Clone)]
pub struct OptimizationThresholds {
    /// Maximum acceptable latency (ms)
    pub max_latency_ms: f64,

    /// Target latency (ms)
    pub target_latency_ms: f64,

    /// Minimum acceptable cache hit rate
    pub min_cache_hit_rate: f64,

    /// Target cache hit rate
    pub target_cache_hit_rate: f64,

    /// Maximum acceptable CPU usage (%)
    pub max_cpu_percent: f64,

    /// Target CPU usage (%)
    pub target_cpu_percent: f64,

    /// Maximum acceptable memory (MB)
    pub max_memory_mb: f64,

    /// Target memory usage (MB)
    pub target_memory_mb: f64,
}

impl Default for OptimizationThresholds {
    fn default() -> Self {
        Self {
            max_latency_ms: 100.0,
            target_latency_ms: 50.0,
            min_cache_hit_rate: 0.8,
            target_cache_hit_rate: 0.95,
            max_cpu_percent: 80.0,
            target_cpu_percent: 60.0,
            max_memory_mb: 1024.0,
            target_memory_mb: 512.0,
        }
    }
}

/// Optimization recommendation
#[derive(Debug, Clone)]
pub enum OptimizationRecommendation {
    /// Reduce latency
    ReduceLatency {
        current_ms: f64,
        target_ms: f64,
        actions: Vec<String>,
    },

    /// Improve cache hit rate
    ImproveCacheHitRate {
        current_rate: f64,
        target_rate: f64,
        actions: Vec<String>,
    },

    /// Reduce CPU usage
    ReduceCpuUsage {
        current_percent: f64,
        target_percent: f64,
        actions: Vec<String>,
    },

    /// Reduce memory usage
    ReduceMemoryUsage {
        current_mb: f64,
        target_mb: f64,
        actions: Vec<String>,
    },
}

/// Cache optimization result
#[derive(Debug, Clone)]
pub struct CacheOptimization {
    /// Current statistics
    pub current_stats: CacheStats,

    /// Recommended configuration changes
    pub recommended_changes: Vec<String>,

    /// Expected improvement
    pub expected_improvement: String,
}

/// Pool optimization result
#[derive(Debug, Clone)]
pub struct PoolOptimization {
    /// Current statistics
    pub current_stats: PoolStats,

    /// Recommended configuration changes
    pub recommended_changes: Vec<String>,

    /// Expected improvement
    pub expected_improvement: String,
}

/// Comprehensive optimization report
#[derive(Debug, Clone)]
pub struct OptimizationReport {
    /// Performance score (0-100)
    pub performance_score: f64,

    /// Whether system is healthy
    pub is_healthy: bool,

    /// List of recommendations
    pub recommendations: Vec<OptimizationRecommendation>,

    /// Priority actions to take
    pub priority_actions: Vec<String>,
}
