//! System resource monitoring

use std::time::Duration;
use sysinfo::System;

/// System resource monitor
pub struct ResourceMonitor {
    /// System info
    system: System,

    /// Process ID
    pid: usize,
}

impl Default for ResourceMonitor {
    fn default() -> Self {
        let pid = sysinfo::get_current_pid()
            .map(|p| p.as_u32() as usize)
            .unwrap_or(0);
        let mut system = System::new_all();
        system.refresh_all();

        Self { system, pid }
    }
}

impl ResourceMonitor {
    /// Create new resource monitor
    pub fn new() -> Self {
        Self::default()
    }

    /// Get current system metrics
    pub fn get_metrics(&mut self) -> SystemMetrics {
        self.system.refresh_all();

        let process = self.system.process(sysinfo::Pid::from(self.pid)).unwrap();

        SystemMetrics {
            cpu_usage_percent: process.cpu_usage() as f64,
            memory_usage_bytes: process.memory(),
            virtual_memory_bytes: process.virtual_memory(),
            total_memory_bytes: self.system.total_memory(),
            used_memory_bytes: self.system.used_memory(),
            load_average: 0.0, // Load average not available in current sysinfo version
            process_count: self.system.processes().len(),
        }
    }

    /// Monitor for duration and return average
    pub fn monitor_average(&mut self, duration: Duration) -> SystemMetrics {
        let start = std::time::Instant::now();
        let mut samples = Vec::new();

        while start.elapsed() < duration {
            samples.push(self.get_metrics());
            std::thread::sleep(Duration::from_secs(1));
        }

        self.average_metrics(&samples)
    }

    /// Calculate average metrics
    fn average_metrics(&self, samples: &[SystemMetrics]) -> SystemMetrics {
        let count = samples.len() as f64;

        SystemMetrics {
            cpu_usage_percent: samples.iter().map(|m| m.cpu_usage_percent).sum::<f64>() / count,
            memory_usage_bytes: (samples.iter().map(|m| m.memory_usage_bytes).sum::<u64>() as f64
                / count) as u64,
            virtual_memory_bytes: (samples.iter().map(|m| m.virtual_memory_bytes).sum::<u64>()
                as f64
                / count) as u64,
            total_memory_bytes: samples.first().map(|m| m.total_memory_bytes).unwrap_or(0),
            used_memory_bytes: (samples.iter().map(|m| m.used_memory_bytes).sum::<u64>() as f64
                / count) as u64,
            load_average: samples.iter().map(|m| m.load_average).sum::<f64>() / count,
            process_count: (samples.iter().map(|m| m.process_count).sum::<usize>() as f64 / count)
                as usize,
        }
    }
}

/// System metrics
#[derive(Debug, Clone)]
pub struct SystemMetrics {
    /// CPU usage (%)
    pub cpu_usage_percent: f64,

    /// Memory usage (bytes)
    pub memory_usage_bytes: u64,

    /// Virtual memory (bytes)
    pub virtual_memory_bytes: u64,

    /// Total system memory (bytes)
    pub total_memory_bytes: u64,

    /// Used system memory (bytes)
    pub used_memory_bytes: u64,

    /// Load average
    pub load_average: f64,

    /// Number of processes
    pub process_count: usize,
}

impl SystemMetrics {
    /// Format as human-readable
    pub fn format(&self) -> String {
        format!(
            "System Metrics:\n\
             CPU: {:.1}% | Memory: {} / {} | Load: {:.2}\n\
             Processes: {}",
            self.cpu_usage_percent,
            self.format_bytes(self.memory_usage_bytes),
            self.format_bytes(self.total_memory_bytes),
            self.load_average,
            self.process_count
        )
    }

    /// Format bytes
    fn format_bytes(&self, bytes: u64) -> String {
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
