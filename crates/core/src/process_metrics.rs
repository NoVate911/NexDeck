//! Lightweight process memory reporting helpers for `nexdeck-agent`.

use std::fmt;

/// Snapshot of current process RAM consumption in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessMemoryStats {
    /// Current resident working set (`WorkingSetSize` on Windows, `VmRSS` on Linux).
    pub working_set_bytes: u64,
    /// Peak resident working set observed during process lifetime.
    pub peak_working_set_bytes: u64,
    /// Private committed/pagefile bytes when reported by the OS.
    pub private_bytes: u64,
}

impl fmt::Display for ProcessMemoryStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "working_set={} (peak={}, private={})",
            format_bytes_mib(self.working_set_bytes),
            format_bytes_mib(self.peak_working_set_bytes),
            format_bytes_mib(self.private_bytes)
        )
    }
}

/// Formats a byte count into a human-readable MiB string (e.g., `"2.45 MiB"`).
#[must_use]
pub fn format_bytes_mib(bytes: u64) -> String {
    let mib = (bytes as f64) / (1024.0 * 1024.0);
    format!("{mib:.2} MiB")
}

/// Queries the operating system for the current process's RAM footprint.
#[must_use]
pub fn current_process_memory() -> Option<ProcessMemoryStats> {
    platform::current_process_memory()
}

#[cfg(windows)]
mod platform {
    use super::ProcessMemoryStats;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    #[allow(unsafe_code)]
    pub(super) fn current_process_memory() -> Option<ProcessMemoryStats> {
        let mut counters = PROCESS_MEMORY_COUNTERS {
            cb: u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS>()).ok()?,
            ..Default::default()
        };

        // SAFETY: `GetCurrentProcess` returns the current-process pseudo-handle (no closing
        // required) and `counters` is uniquely owned stack memory sized to `counters.cb`.
        let ok = unsafe {
            GetProcessMemoryInfo(
                GetCurrentProcess(),
                std::ptr::addr_of_mut!(counters),
                counters.cb,
            )
        };
        if ok == 0 {
            return None;
        }

        Some(ProcessMemoryStats {
            working_set_bytes: counters.WorkingSetSize as u64,
            peak_working_set_bytes: counters.PeakWorkingSetSize as u64,
            private_bytes: counters.PagefileUsage as u64,
        })
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::ProcessMemoryStats;

    pub(super) fn current_process_memory() -> Option<ProcessMemoryStats> {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let working_set_bytes = parse_status_kb(&status, "VmRSS:")?;
        let peak_working_set_bytes =
            parse_status_kb(&status, "VmHWM:").unwrap_or(working_set_bytes);
        let private_bytes = parse_status_kb(&status, "RssAnon:")
            .or_else(|| parse_status_kb(&status, "VmData:"))
            .unwrap_or(working_set_bytes);

        Some(ProcessMemoryStats {
            working_set_bytes,
            peak_working_set_bytes,
            private_bytes,
        })
    }

    fn parse_status_kb(status: &str, key: &str) -> Option<u64> {
        let line = status.lines().find(|line| line.starts_with(key))?;
        let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
        Some(kb.saturating_mul(1024))
    }
}

#[cfg(all(not(windows), not(target_os = "linux")))]
mod platform {
    use super::ProcessMemoryStats;

    pub(super) fn current_process_memory() -> Option<ProcessMemoryStats> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_bytes_into_mib() {
        assert_eq!(format_bytes_mib(5 * 1024 * 1024), "5.00 MiB");
        assert_eq!(format_bytes_mib(2_621_440), "2.50 MiB");
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn queries_non_zero_working_set() {
        let stats = current_process_memory().expect("platform should report process memory");
        assert!(stats.working_set_bytes > 0);
        assert!(stats.peak_working_set_bytes >= stats.working_set_bytes);
    }
}
