//! Performance benchmarking utilities

use crate::performance::PerformanceResult;
use std::time::Instant;

/// Benchmark runner
pub struct Benchmark {
    /// Name
    name: String,

    /// Number of iterations
    iterations: usize,

    /// Warmup iterations
    warmup_iters: usize,

    /// Results
    results: Vec<BenchmarkIteration>,
}

impl Benchmark {
    /// Create new benchmark
    pub fn new(name: impl Into<String>, iterations: usize) -> Self {
        Self {
            name: name.into(),
            iterations,
            warmup_iters: 10,
            results: Vec::new(),
        }
    }

    /// Set warmup iterations
    pub fn with_warmup(mut self, warmup_iters: usize) -> Self {
        self.warmup_iters = warmup_iters;
        self
    }

    /// Run benchmark
    pub async fn run<F, Fut>(&mut self, mut func: F) -> PerformanceResult<BenchmarkResult>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = PerformanceResult<()>>,
    {
        // Warmup phase
        for i in 0..self.warmup_iters {
            func().await?;
            if i % 10 == 0 {
                println!("Warmup {}/{} complete", i + 1, self.warmup_iters);
            }
        }

        // Benchmark phase
        self.results.reserve(self.iterations);

        for i in 0..self.iterations {
            let start = Instant::now();
            func().await?;
            let duration = start.elapsed();

            self.results.push(BenchmarkIteration {
                iteration: i,
                duration_ms: duration.as_millis(),
                duration_nanos: duration.as_nanos(),
            });

            if i % 100 == 0 && i > 0 {
                println!("Iteration {}/{} complete", i + 1, self.iterations);
            }
        }

        Ok(self.analyze())
    }

    /// Run concurrent benchmark
    pub async fn run_concurrent<F, Fut>(
        &mut self,
        func: F,
        concurrency: usize,
    ) -> PerformanceResult<BenchmarkResult>
    where
        F: Fn() -> Fut + Clone + Send + 'static,
        Fut: std::future::Future<Output = PerformanceResult<()>> + Send,
    {
        use tokio::task::JoinSet;

        // Warmup
        for i in 0..self.warmup_iters {
            func().await?;
            if i % 10 == 0 {
                println!("Warmup {}/{} complete", i + 1, self.warmup_iters);
            }
        }

        // Concurrent benchmark
        self.results.reserve(self.iterations);

        for batch in 0..(self.iterations / concurrency).max(1) {
            let mut join_set = JoinSet::new();

            for _ in 0..concurrency {
                if self.results.len() >= self.iterations {
                    break;
                }

                let func_clone = func.clone();
                join_set.spawn(async move {
                    let start = Instant::now();
                    func_clone().await?;
                    let duration = start.elapsed();

                    PerformanceResult::Ok(BenchmarkIteration {
                        iteration: 0, // Will be set later
                        duration_ms: duration.as_millis(),
                        duration_nanos: duration.as_nanos(),
                    })
                });
            }

            while let Some(result) = join_set.join_next().await {
                match result {
                    Ok(Ok(iteration)) => {
                        self.results.push(BenchmarkIteration {
                            iteration: self.results.len(),
                            ..iteration
                        });
                    }
                    Ok(Err(e)) => return Err(e),
                    Err(_) => {
                        return Err(crate::performance::PerformanceError::BenchmarkError(
                            "Task failed".to_string(),
                        ))
                    }
                }
            }

            if batch % 10 == 0 {
                println!(
                    "Batch {}/{} complete",
                    batch + 1,
                    self.iterations / concurrency
                );
            }
        }

        Ok(self.analyze())
    }

    /// Analyze results
    fn analyze(&self) -> BenchmarkResult {
        let durations_ms: Vec<u128> = self.results.iter().map(|r| r.duration_ms).collect();

        let sum: u128 = durations_ms.iter().sum();
        let avg = sum / durations_ms.len() as u128;

        let mut sorted = durations_ms.clone();
        sorted.sort();

        let min = sorted.first().copied().unwrap_or(0);
        let max = sorted.last().copied().unwrap_or(0);
        let p50 = sorted[sorted.len() / 2];
        let p95 = sorted[(sorted.len() as f64 * 0.95) as usize];
        let p99 = sorted[(sorted.len() as f64 * 0.99) as usize];

        let throughput = 1000.0 / (avg as f64);

        BenchmarkResult {
            name: self.name.clone(),
            iterations: self.results.len(),
            total_duration_ms: sum,
            avg_duration_ms: avg,
            min_duration_ms: min,
            max_duration_ms: max,
            p50_duration_ms: p50,
            p95_duration_ms: p95,
            p99_duration_ms: p99,
            throughput_per_sec: throughput,
        }
    }
}

/// Benchmark iteration result
#[derive(Debug, Clone)]
pub struct BenchmarkIteration {
    /// Iteration number
    pub iteration: usize,

    /// Duration in milliseconds
    pub duration_ms: u128,

    /// Duration in nanoseconds
    pub duration_nanos: u128,
}

/// Benchmark result
#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    /// Benchmark name
    pub name: String,

    /// Number of iterations
    pub iterations: usize,

    /// Total duration (ms)
    pub total_duration_ms: u128,

    /// Average duration (ms)
    pub avg_duration_ms: u128,

    /// Minimum duration (ms)
    pub min_duration_ms: u128,

    /// Maximum duration (ms)
    pub max_duration_ms: u128,

    /// 50th percentile (ms)
    pub p50_duration_ms: u128,

    /// 95th percentile (ms)
    pub p95_duration_ms: u128,

    /// 99th percentile (ms)
    pub p99_duration_ms: u128,

    /// Throughput (operations/sec)
    pub throughput_per_sec: f64,
}

impl BenchmarkResult {
    /// Format as human-readable
    pub fn format(&self) -> String {
        format!(
            "Benchmark '{}' ({} iterations):\n\
             Total: {}ms, Avg: {}ms\n\
             Min: {}ms, Max: {}ms\n\
             P50: {}ms, P95: {}ms, P99: {}ms\n\
             Throughput: {:.2} ops/sec",
            self.name,
            self.iterations,
            self.total_duration_ms,
            self.avg_duration_ms,
            self.min_duration_ms,
            self.max_duration_ms,
            self.p50_duration_ms,
            self.p95_duration_ms,
            self.p99_duration_ms,
            self.throughput_per_sec
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_benchmark_creation() {
        let bench = Benchmark::new("test_bench", 100);
        assert_eq!(bench.iterations, 100);
    }

    #[tokio::test]
    async fn test_benchmark_run() {
        let mut bench = Benchmark::new("test", 10);

        let result = bench
            .run(|| async {
                sleep(Duration::from_millis(1)).await;
                Ok(())
            })
            .await
            .unwrap();

        assert_eq!(result.iterations, 10);
        assert!(result.avg_duration_ms > 0);
    }
}
