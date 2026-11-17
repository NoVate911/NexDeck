//! Hardware resource monitoring models and native Windows 11 collectors.
//!
//! Platform-independent snapshot structures and rate calculations live in this
//! module, while Win32 / PDH FFI bindings are isolated inside the `windows`
//! submodule ([ADR-0006](../../docs/architecture/adr/0006-monitoring-win32-pdh.md)).

use std::fmt;

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows;

#[cfg(windows)]
pub use windows::WindowsCollector;

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

/// Platform collector interface implemented by telemetry backends.
pub trait MetricCollector {
    /// Samples current hardware metrics into a [`SystemSnapshot`].
    ///
    /// # Errors
    ///
    /// Returns [`MonitoringError`] if reading system counters fails.
    fn collect(&mut self) -> Result<SystemSnapshot, MonitoringError>;
}

/// Raw cumulative CPU times (`100ns` ticks on Windows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTimesSample {
    /// Cumulative idle ticks across logical processors.
    pub idle: u64,
    /// Cumulative kernel ticks (including idle ticks on Windows).
    pub kernel: u64,
    /// Cumulative user-space ticks.
    pub user: u64,
}

/// Clamps a floating-point percentage into `[0.0, 100.0]`.
#[must_use]
pub fn clamp_percentage(value: f64) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    value.clamp(0.0, 100.0) as f32
}

/// Computes total CPU utilization percentage from two cumulative [`CpuTimesSample`] reads.
///
/// Matches Windows `GetSystemTimes` semantics where `kernel` includes `idle`.
#[must_use]
pub fn compute_cpu_usage_pct(prev: CpuTimesSample, curr: CpuTimesSample) -> f32 {
    let idle_delta = curr.idle.saturating_sub(prev.idle);
    let kernel_delta = curr.kernel.saturating_sub(prev.kernel);
    let user_delta = curr.user.saturating_sub(prev.user);
    let total = kernel_delta.saturating_add(user_delta);
    if total == 0 {
        return 0.0;
    }
    let busy = total.saturating_sub(idle_delta);
    clamp_percentage((busy as f64 / total as f64) * 100.0)
}

/// Computes byte throughput per second from monotonic byte counters and elapsed time.
#[must_use]
pub fn compute_throughput_bps(prev_bytes: u64, curr_bytes: u64, elapsed_secs: f64) -> u64 {
    if !elapsed_secs.is_finite() || elapsed_secs <= 0.001 {
        return 0;
    }
    let delta = curr_bytes.saturating_sub(prev_bytes);
    ((delta as f64) / elapsed_secs).round().max(0.0) as u64
}

/// Deterministic collector returning a preconfigured [`SystemSnapshot`] for tests.
#[derive(Debug, Clone)]
pub struct StaticCollector {
    snapshot: SystemSnapshot,
}

impl StaticCollector {
    /// Creates a [`StaticCollector`] returning clones of `snapshot`.
    #[must_use]
    pub const fn new(snapshot: SystemSnapshot) -> Self {
        Self { snapshot }
    }
}

impl MetricCollector for StaticCollector {
    fn collect(&mut self) -> Result<SystemSnapshot, MonitoringError> {
        Ok(self.snapshot.clone())
    }
}

/// Fallback collector used on non-Windows targets where native Win32/PDH APIs are unavailable.
#[cfg(not(windows))]
#[derive(Debug, Default)]
pub struct UnsupportedCollector;

#[cfg(not(windows))]
impl MetricCollector for UnsupportedCollector {
    fn collect(&mut self) -> Result<SystemSnapshot, MonitoringError> {
        Err(MonitoringError::UnsupportedSource(
            "native Windows collector requires Windows 11".to_owned(),
        ))
    }
}

/// Creates the default platform hardware metric collector.
///
/// # Errors
///
/// Returns [`MonitoringError`] if platform counter initialization fails.
pub fn default_collector() -> Result<Box<dyn MetricCollector + Send>, MonitoringError> {
    #[cfg(windows)]
    {
        Ok(Box::new(WindowsCollector::new()?))
    }
    #[cfg(not(windows))]
    {
        Ok(Box::new(UnsupportedCollector))
    }
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

    #[test]
    fn cpu_delta_computes_expected_utilization() {
        let prev = CpuTimesSample {
            idle: 1_000,
            kernel: 1_500,
            user: 500,
        };
        let curr = CpuTimesSample {
            idle: 1_600,
            kernel: 2_300,
            user: 700,
        };
        // kernel_delta = 800, user_delta = 200 -> total = 1000; idle_delta = 600 -> busy = 400 -> 40%
        let usage = compute_cpu_usage_pct(prev, curr);
        assert!((usage - 40.0).abs() < 0.01);
    }

    #[test]
    fn cpu_delta_handles_zero_and_non_monotonic_inputs() {
        let same = CpuTimesSample {
            idle: 500,
            kernel: 700,
            user: 200,
        };
        assert_eq!(compute_cpu_usage_pct(same, same), 0.0);
        assert_eq!(clamp_percentage(f64::NAN), 0.0);
        assert_eq!(clamp_percentage(150.0), 100.0);
    }

    #[test]
    fn throughput_computation_guards_small_elapsed_and_counters() {
        assert_eq!(compute_throughput_bps(1_000, 3_000, 2.0), 1_000);
        assert_eq!(compute_throughput_bps(3_000, 1_000, 2.0), 0);
        assert_eq!(compute_throughput_bps(1_000, 5_000, 0.0), 0);
    }

    #[test]
    fn static_collector_returns_snapshot() {
        let snapshot = SystemSnapshot {
            captured_at_epoch_ms: 1_000,
            uptime_secs: 42,
            cpu: CpuMetrics {
                total_usage_pct: 12.5,
                per_core_usage_pct: vec![10.0, 15.0],
                frequency_mhz: Some(3_600),
                temperature_celsius: None,
            },
            memory: MemoryMetrics {
                total_bytes: 16_000_000_000,
                used_bytes: 4_000_000_000,
            },
            gpus: Vec::new(),
            disks: Vec::new(),
            networks: Vec::new(),
            battery: None,
        };
        let mut collector = StaticCollector::new(snapshot.clone());
        assert_eq!(collector.collect(), Ok(snapshot));
    }
}
