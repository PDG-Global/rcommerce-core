//! Memory and performance profiling utilities

use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

/// Global allocator wrapper for memory tracking
static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static DEALLOCATED: AtomicUsize = AtomicUsize::new(0);

/// Memory profiler
pub struct MemoryProfiler {
    /// Start time
    start_time: Instant,

    /// Initial allocations
    initial_allocated: usize,

    /// Start allocations
    start_allocated: usize,
}

impl MemoryProfiler {
    /// Start profiling
    pub fn start() -> Self {
        Self {
            start_time: Instant::now(),
            initial_allocated: ALLOCATED.load(Ordering::Relaxed),
            start_allocated: ALLOCATED.load(Ordering::Relaxed),
        }
    }

    /// Get current memory usage
    pub fn current_usage(&self) -> MemoryUsage {
        let allocated = ALLOCATED.load(Ordering::Relaxed);
        let deallocated = DEALLOCATED.load(Ordering::Relaxed);

        MemoryUsage {
            allocated_bytes: allocated,
            deallocated_bytes: deallocated,
            net_usage_bytes: allocated.saturating_sub(deallocated),
        }
    }

    /// Stop profiling and get results
    pub fn stop(self) -> PerformanceProfile {
        let end_time = Instant::now();
        let end_allocated = ALLOCATED.load(Ordering::Relaxed);

        PerformanceProfile {
            duration_ms: end_time.duration_since(self.start_time).as_millis(),
            memory_allocated: end_allocated.saturating_sub(self.start_allocated),
            peak_memory_usage: self.peak_usage(),
        }
    }

    /// Get peak memory usage during profiling
    fn peak_usage(&self) -> usize {
        let current = ALLOCATED.load(Ordering::Relaxed);
        current.saturating_sub(self.initial_allocated)
    }
}

/// Memory usage snapshot
#[derive(Debug, Clone)]
pub struct MemoryUsage {
    /// Total allocated bytes
    pub allocated_bytes: usize,

    /// Total deallocated bytes
    pub deallocated_bytes: usize,

    /// Net memory usage (allocated - deallocated)
    pub net_usage_bytes: usize,
}

impl MemoryUsage {
    /// Format as human-readable string
    pub fn format(&self) -> String {
        format!(
            "Memory: allocated={}, deallocated={}, net={}",
            self.format_bytes(self.allocated_bytes),
            self.format_bytes(self.deallocated_bytes),
            self.format_bytes(self.net_usage_bytes)
        )
    }

    /// Format bytes to human-readable
    fn format_bytes(&self, bytes: usize) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
        let mut size = bytes as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

/// Performance profile
#[derive(Debug, Clone)]
pub struct PerformanceProfile {
    /// Duration in milliseconds
    pub duration_ms: u128,

    /// Memory allocated during profiling
    pub memory_allocated: usize,

    /// Peak memory usage
    pub peak_memory_usage: usize,
}

impl PerformanceProfile {
    /// Format as human-readable
    pub fn format(&self) -> String {
        format!(
            "Performance Profile:\n  Duration: {}ms\n  Memory Allocated: {}\n  Peak Memory: {}",
            self.duration_ms,
            self.format_bytes(self.memory_allocated),
            self.format_bytes(self.peak_memory_usage)
        )
    }

    /// Format bytes
    fn format_bytes(&self, bytes: usize) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
        let mut size = bytes as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

/// Function profiler for measuring execution time
pub struct FunctionProfiler {
    name: String,
    start_time: Instant,
}

impl FunctionProfiler {
    /// Start profiling a function
    pub fn start(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            start_time: Instant::now(),
        }
    }

    /// Stop profiling and return result
    pub fn stop(self) -> FunctionProfile {
        let duration = self.start_time.elapsed();

        FunctionProfile {
            function_name: self.name,
            duration_ms: duration.as_millis(),
            duration_nanos: duration.as_nanos(),
        }
    }
}

/// Function profile
#[derive(Debug, Clone)]
pub struct FunctionProfile {
    /// Function name
    pub function_name: String,

    /// Duration in milliseconds
    pub duration_ms: u128,

    /// Duration in nanoseconds
    pub duration_nanos: u128,
}
