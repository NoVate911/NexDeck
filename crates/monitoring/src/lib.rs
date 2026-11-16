//! Hardware resource monitoring models and collector traits for NexDeck.
//!
//! Concrete Windows 11 collectors (`PDH`, `Win32`, optional vendor GPU APIs)
//! will implement [`MetricCollector`] in Stage 1 while preserving the
//! minimal-footprint target of the Core Agent.

use std::fmt;

/// Processor telemetry snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct CpuMetrics {
    /// Total package utilization percentage (`0.0..=100.0`).
    pub total_usage_pct: f32,
    /// Per-logical-core utilization percentages.
    pub per_core_usage_pct: Vec<f32>,
    /// Current clock frequency in MHz, if available from hardware.
    pub frequency_mhz: Option<u32>,
    /// Package temperature in Celsius, if exposed by sensors.
    pub temperature_celsius: Option<f32>,
}

/// System RAM telemetry snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryMetrics {
    /// Total physical RAM in bytes.
    pub total_bytes: u64,
    /// Currently used RAM in bytes.
    pub used_bytes: u64,
}

impl MemoryMetrics {
    /// Computes available RAM in bytes using saturating subtraction.
    #[must_use]
    pub const fn free_bytes(&self) -> u64 {
        self.total_bytes.saturating_sub(self.used_bytes)
    }

    /// Computes memory utilization percentage (`0.0..=100.0`).
    #[must_use]
    pub fn usage_pct(&self) -> f32 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        ((self.used_bytes.min(self.total_bytes) as f64 / self.total_bytes as f64) * 100.0) as f32
    }
}

/// Graphics adapter telemetry snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuMetrics {
    /// Adapter display name.
    pub name: String,
    /// Core load percentage (`0.0..=100.0`), if available.
    pub usage_pct: Option<f32>,
    /// Core temperature in Celsius, if available.
    pub temperature_celsius: Option<f32>,
    /// Core clock in MHz, if available.
    pub clock_mhz: Option<u32>,
    /// Used VRAM in bytes, if available.
    pub vram_used_bytes: Option<u64>,
    /// Total VRAM in bytes, if available.
    pub vram_total_bytes: Option<u64>,
}

/// Storage volume telemetry snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskMetrics {
    /// Drive letter or mount point (e.g., `C:\`).
    pub mount_point: String,
    /// Total capacity in bytes.
    pub total_bytes: u64,
    /// Free capacity in bytes.
    pub free_bytes: u64,
    /// Current read throughput in bytes/sec.
    pub read_bytes_per_sec: u64,
    /// Current write throughput in bytes/sec.
    pub write_bytes_per_sec: u64,
}

/// Network interface telemetry snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkMetrics {
    /// Interface name.
    pub interface_name: String,
    /// Upload throughput in bytes/sec.
    pub tx_bytes_per_sec: u64,
    /// Download throughput in bytes/sec.
    pub rx_bytes_per_sec: u64,
    /// Cumulative transmitted bytes since boot/reset.
    pub total_tx_bytes: u64,
    /// Cumulative received bytes since boot/reset.
    pub total_rx_bytes: u64,
}

/// Battery telemetry snapshot for laptops/UPS devices.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BatteryMetrics {
    /// Charge percentage (`0.0..=100.0`).
    pub charge_pct: f32,
    /// Whether AC power is connected.
    pub ac_connected: bool,
}

/// Aggregated hardware telemetry sample captured at a point in time.
#[derive(Debug, Clone, PartialEq)]
pub struct SystemSnapshot {
    /// Capture timestamp in Unix epoch milliseconds.
    pub captured_at_epoch_ms: u64,
    /// System uptime in seconds.
    pub uptime_secs: u64,
    /// CPU metrics.
    pub cpu: CpuMetrics,
    /// Physical RAM metrics.
    pub memory: MemoryMetrics,
    /// Detected GPU metrics.
    pub gpus: Vec<GpuMetrics>,
    /// Detected disk metrics.
    pub disks: Vec<DiskMetrics>,
    /// Active network interface metrics.
    pub networks: Vec<NetworkMetrics>,
    /// Battery telemetry, when present.
    pub battery: Option<BatteryMetrics>,
}

/// Errors produced by metric collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitoringError {
    /// Telemetry source is unsupported on the current hardware or OS.
    UnsupportedSource(String),
    /// Reading hardware counters failed.
    CollectionFailed(String),
}

impl fmt::Display for MonitoringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSource(src) => write!(f, "unsupported monitoring source: {src}"),
            Self::CollectionFailed(reason) => write!(f, "monitoring collection failed: {reason}"),
        }
    }
}

impl std::error::Error for MonitoringError {}

/// Platform collector interface implemented by Stage 1+ telemetry backends.
pub trait MetricCollector {
    /// Samples current hardware metrics into a [`SystemSnapshot`].
    ///
    /// # Errors
    ///
    /// Returns [`MonitoringError`] if reading system counters fails.
    fn collect(&mut self) -> Result<SystemSnapshot, MonitoringError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_metrics_computes_free_and_percentage() {
        let mem = MemoryMetrics {
            total_bytes: 16_000_000_000,
            used_bytes: 4_000_000_000,
        };
        assert_eq!(mem.free_bytes(), 12_000_000_000);
        assert!((mem.usage_pct() - 25.0).abs() < 0.01);
    }

    #[test]
    fn zero_total_memory_returns_zero_usage() {
        let mem = MemoryMetrics {
            total_bytes: 0,
            used_bytes: 1024,
        };
        assert_eq!(mem.free_bytes(), 0);
        assert_eq!(mem.usage_pct(), 0.0);
    }
}
