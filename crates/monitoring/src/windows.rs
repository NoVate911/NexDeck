//! Native Windows 11 hardware telemetry collector built directly on `windows-sys`.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::ptr::{null, null_mut};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetIfTable2, IF_TYPE_SOFTWARE_LOOPBACK, MIB_IF_TABLE2,
};
use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows_sys::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW,
};
use windows_sys::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PdhAddEnglishCounterW,
    PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterValue, PdhOpenQueryW,
};
use windows_sys::Win32::System::Power::{
    CallNtPowerInformation, GetSystemPowerStatus, PROCESSOR_POWER_INFORMATION,
    ProcessorInformation, SYSTEM_POWER_STATUS,
};
use windows_sys::Win32::System::SystemInformation::{
    GetTickCount64, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use windows_sys::Win32::System::Threading::GetSystemTimes;

use crate::{
    BatteryMetrics, CpuMetrics, CpuTimesSample, DiskMetrics, MemoryMetrics, MetricCollector,
    MonitoringError, NetworkMetrics, SystemSnapshot, clamp_percentage, compute_cpu_usage_pct,
    compute_throughput_bps,
};

const DRIVE_FIXED: u32 = 3;

/// RAII wrapper ensuring `PdhCloseQuery` always releases the PDH handle.
struct PdhQueryGuard(PDH_HQUERY);

// SAFETY: PDH query handles are used exclusively under `&mut WindowsCollector`.
unsafe impl Send for PdhQueryGuard {}

impl Drop for PdhQueryGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = PdhCloseQuery(self.0);
            }
        }
    }
}

/// RAII wrapper ensuring `FreeMibTable` always releases `GetIfTable2` allocations.
struct MibTableGuard(*mut MIB_IF_TABLE2);

impl Drop for MibTableGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                FreeMibTable(self.0.cast());
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DiskPdhCounters {
    read: PDH_HCOUNTER,
    write: PDH_HCOUNTER,
}

// SAFETY: `PDH_HCOUNTER` handles belong to `PdhQueryGuard` and are only used under `&mut self`.
unsafe impl Send for DiskPdhCounters {}

#[derive(Debug, Clone, Copy)]
struct CorePdhCounter(PDH_HCOUNTER);

// SAFETY: `PDH_HCOUNTER` handles belong to `PdhQueryGuard` and are only used under `&mut self`.
unsafe impl Send for CorePdhCounter {}

/// Native Windows 11 hardware telemetry collector using Win32 and PDH APIs.
pub struct WindowsCollector {
    pdh_query: PdhQueryGuard,
    core_counters: Vec<CorePdhCounter>,
    disk_counters: HashMap<String, DiskPdhCounters>,
    pdh_warmed_up: bool,
    prev_cpu_times: Option<CpuTimesSample>,
    prev_net_bytes: HashMap<u64, (u64, u64)>,
    prev_net_at: Option<Instant>,
    logical_cores: usize,
}

impl WindowsCollector {
    /// Initializes Win32/PDH baseline counters for subsequent [`Self::collect`] samples.
    ///
    /// # Errors
    ///
    /// Returns [`MonitoringError::CollectionFailed`] if opening the PDH query fails.
    pub fn new() -> Result<Self, MonitoringError> {
        let mut query: PDH_HQUERY = null_mut();
        let status = unsafe { PdhOpenQueryW(null(), 0, &mut query) };
        if status != 0 || query.is_null() {
            return Err(MonitoringError::CollectionFailed(format!(
                "PdhOpenQueryW failed with status 0x{status:08X}"
            )));
        }

        let pdh_query = PdhQueryGuard(query);
        let logical_cores = std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1);

        let mut core_counters = Vec::with_capacity(logical_cores);
        for core_idx in 0..logical_cores {
            let path = to_wide(&format!(r"\Processor({core_idx})\% Processor Time"));
            let mut counter: PDH_HCOUNTER = null_mut();
            let add_status =
                unsafe { PdhAddEnglishCounterW(pdh_query.0, path.as_ptr(), 0, &mut counter) };
            if add_status == 0 && !counter.is_null() {
                core_counters.push(CorePdhCounter(counter));
            }
        }

        let mut disk_counters = HashMap::new();
        for drive in fixed_drive_roots() {
            let instance = drive.trim_end_matches('\\');
            let read_path = to_wide(&format!(r"\LogicalDisk({instance})\Disk Read Bytes/sec"));
            let write_path = to_wide(&format!(r"\LogicalDisk({instance})\Disk Write Bytes/sec"));
            let mut read_counter: PDH_HCOUNTER = null_mut();
            let mut write_counter: PDH_HCOUNTER = null_mut();

            let r_ok = unsafe {
                PdhAddEnglishCounterW(pdh_query.0, read_path.as_ptr(), 0, &mut read_counter)
            };
            let w_ok = unsafe {
                PdhAddEnglishCounterW(pdh_query.0, write_path.as_ptr(), 0, &mut write_counter)
            };
            if r_ok == 0 && w_ok == 0 && !read_counter.is_null() && !write_counter.is_null() {
                disk_counters.insert(
                    drive,
                    DiskPdhCounters {
                        read: read_counter,
                        write: write_counter,
                    },
                );
            }
        }

        let pdh_warmed_up = unsafe { PdhCollectQueryData(pdh_query.0) == 0 };
        let prev_cpu_times = read_system_times();

        Ok(Self {
            pdh_query,
            core_counters,
            disk_counters,
            pdh_warmed_up,
            prev_cpu_times,
            prev_net_bytes: HashMap::new(),
            prev_net_at: None,
            logical_cores,
        })
    }

    fn collect_cpu(&mut self, pdh_ready: bool) -> CpuMetrics {
        let current_times = read_system_times();
        let total_usage_pct = match (self.prev_cpu_times, current_times) {
            (Some(prev), Some(curr)) => compute_cpu_usage_pct(prev, curr),
            _ => 0.0,
        };
        if current_times.is_some() {
            self.prev_cpu_times = current_times;
        }

        let mut per_core_usage_pct = Vec::with_capacity(self.core_counters.len());
        if pdh_ready {
            for counter in &self.core_counters {
                let pct = read_pdh_double(counter.0).map_or(0.0, clamp_percentage);
                per_core_usage_pct.push(pct);
            }
        } else {
            per_core_usage_pct.resize(self.core_counters.len(), 0.0);
        }

        let frequency_mhz = read_average_cpu_mhz(self.logical_cores);

        CpuMetrics {
            total_usage_pct,
            per_core_usage_pct,
            frequency_mhz,
            temperature_celsius: None,
        }
    }

    fn collect_disks(&self, pdh_ready: bool) -> Vec<DiskMetrics> {
        let drives = fixed_drive_roots();
        let mut metrics = Vec::with_capacity(drives.len());

        for mount_point in drives {
            let wide_root = to_wide(&mount_point);
            let mut free_to_caller: u64 = 0;
            let mut total_bytes: u64 = 0;
            let mut total_free: u64 = 0;

            let ok = unsafe {
                GetDiskFreeSpaceExW(
                    wide_root.as_ptr(),
                    &mut free_to_caller,
                    &mut total_bytes,
                    &mut total_free,
                )
            };
            if ok == 0 {
                continue;
            }

            let (read_bytes_per_sec, write_bytes_per_sec) = if pdh_ready {
                self.disk_counters.get(&mount_point).map_or((0, 0), |c| {
                    let r = read_pdh_double(c.read).map_or(0, non_negative_u64);
                    let w = read_pdh_double(c.write).map_or(0, non_negative_u64);
                    (r, w)
                })
            } else {
                (0, 0)
            };

            metrics.push(DiskMetrics {
                mount_point,
                total_bytes,
                free_bytes: free_to_caller,
                read_bytes_per_sec,
                write_bytes_per_sec,
            });
        }

        metrics
    }

    fn collect_networks(&mut self) -> Result<Vec<NetworkMetrics>, MonitoringError> {
        let mut table_ptr: *mut MIB_IF_TABLE2 = null_mut();
        let status = unsafe { GetIfTable2(&mut table_ptr) };
        if status != 0 || table_ptr.is_null() {
            return Err(MonitoringError::CollectionFailed(format!(
                "GetIfTable2 failed with error {status}"
            )));
        }
        let _guard = MibTableGuard(table_ptr);

        let now = Instant::now();
        let elapsed_secs = self
            .prev_net_at
            .map_or(0.0, |prev| now.duration_since(prev).as_secs_f64());

        let count = unsafe { (*table_ptr).NumEntries as usize };
        let rows = unsafe { std::slice::from_raw_parts((*table_ptr).Table.as_ptr(), count) };

        let mut next_bytes = HashMap::with_capacity(count);
        let mut interfaces = Vec::new();

        for row in rows {
            if row.Type == IF_TYPE_SOFTWARE_LOOPBACK || row.OperStatus != IfOperStatusUp {
                continue;
            }

            let luid = unsafe { row.InterfaceLuid.Value };
            let rx_total = row.InOctets;
            let tx_total = row.OutOctets;
            next_bytes.insert(luid, (tx_total, rx_total));

            let (tx_bytes_per_sec, rx_bytes_per_sec) = match self.prev_net_bytes.get(&luid) {
                Some(&(prev_tx, prev_rx)) => (
                    compute_throughput_bps(prev_tx, tx_total, elapsed_secs),
                    compute_throughput_bps(prev_rx, rx_total, elapsed_secs),
                ),
                None => (0, 0),
            };

            let alias = utf16_z_to_string(&row.Alias);
            let interface_name = if alias.is_empty() {
                utf16_z_to_string(&row.Description)
            } else {
                alias
            };

            interfaces.push(NetworkMetrics {
                interface_name,
                tx_bytes_per_sec,
                rx_bytes_per_sec,
                total_tx_bytes: tx_total,
                total_rx_bytes: rx_total,
            });
        }

        interfaces.sort_by(|a, b| a.interface_name.cmp(&b.interface_name));
        self.prev_net_bytes = next_bytes;
        self.prev_net_at = Some(now);
        Ok(interfaces)
    }
}

impl MetricCollector for WindowsCollector {
    fn collect(&mut self) -> Result<SystemSnapshot, MonitoringError> {
        let pdh_collect_ok = unsafe { PdhCollectQueryData(self.pdh_query.0) == 0 };
        let pdh_ready = self.pdh_warmed_up && pdh_collect_ok;
        if pdh_collect_ok {
            self.pdh_warmed_up = true;
        }

        let memory = read_memory_status()?;
        let cpu = self.collect_cpu(pdh_ready);
        let disks = self.collect_disks(pdh_ready);
        let networks = self.collect_networks()?;
        let battery = read_battery_status();
        let uptime_secs = unsafe { GetTickCount64() } / 1_000;
        let captured_at_epoch_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);

        Ok(SystemSnapshot {
            captured_at_epoch_ms,
            uptime_secs,
            cpu,
            memory,
            gpus: Vec::new(),
            disks,
            networks,
            battery,
        })
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn utf16_z_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end]).trim().to_owned()
}

fn filetime_to_u64(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64)
}

fn non_negative_u64(val: f64) -> u64 {
    if !val.is_finite() || val <= 0.0 {
        return 0;
    }
    val.round() as u64
}

fn read_system_times() -> Option<CpuTimesSample> {
    let mut idle = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let ok = unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) };
    if ok == 0 {
        return None;
    }
    Some(CpuTimesSample {
        idle: filetime_to_u64(idle),
        kernel: filetime_to_u64(kernel),
        user: filetime_to_u64(user),
    })
}

fn read_pdh_double(counter: PDH_HCOUNTER) -> Option<f64> {
    let mut val_type: u32 = 0;
    let mut fmt = PDH_FMT_COUNTERVALUE::default();
    let status =
        unsafe { PdhGetFormattedCounterValue(counter, PDH_FMT_DOUBLE, &mut val_type, &mut fmt) };
    if status != 0 {
        return None;
    }
    Some(unsafe { fmt.Anonymous.doubleValue })
}

fn read_memory_status() -> Result<MemoryMetrics, MonitoringError> {
    let mut mem = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    let ok = unsafe { GlobalMemoryStatusEx(&mut mem) };
    if ok == 0 {
        return Err(MonitoringError::CollectionFailed(
            "GlobalMemoryStatusEx failed".to_owned(),
        ));
    }
    Ok(MemoryMetrics {
        total_bytes: mem.ullTotalPhys,
        used_bytes: mem.ullTotalPhys.saturating_sub(mem.ullAvailPhys),
    })
}

fn read_average_cpu_mhz(logical_cores: usize) -> Option<u32> {
    let count = logical_cores.max(1);
    let mut info = vec![PROCESSOR_POWER_INFORMATION::default(); count];
    let byte_len = (count * size_of::<PROCESSOR_POWER_INFORMATION>()) as u32;
    let status = unsafe {
        CallNtPowerInformation(
            ProcessorInformation,
            null(),
            0,
            info.as_mut_ptr().cast(),
            byte_len,
        )
    };
    if status != 0 {
        return None;
    }
    let sum: u64 = info.iter().map(|p| u64::from(p.CurrentMhz)).sum();
    let avg = sum / (count as u64);
    u32::try_from(avg).ok().filter(|&mhz| mhz > 0)
}

fn fixed_drive_roots() -> Vec<String> {
    let required = unsafe { GetLogicalDriveStringsW(0, null_mut()) };
    if required == 0 {
        return Vec::new();
    }
    let mut buf = vec![0u16; required as usize + 1];
    let written = unsafe { GetLogicalDriveStringsW(buf.len() as u32, buf.as_mut_ptr()) };
    if written == 0 {
        return Vec::new();
    }

    let mut roots = Vec::new();
    let mut idx = 0;
    while idx < written as usize {
        let rest = &buf[idx..];
        let Some(end) = rest.iter().position(|&c| c == 0) else {
            break;
        };
        if end == 0 {
            break;
        }
        let slice = &rest[..=end];
        let drive_type = unsafe { GetDriveTypeW(slice.as_ptr()) };
        if drive_type == DRIVE_FIXED {
            roots.push(String::from_utf16_lossy(&rest[..end]));
        }
        idx += end + 1;
    }
    roots
}

fn read_battery_status() -> Option<BatteryMetrics> {
    let mut status = SYSTEM_POWER_STATUS::default();
    let ok = unsafe { GetSystemPowerStatus(&mut status) };
    if ok == 0 {
        return None;
    }
    // Bit 7 (128) indicates no system battery; 255 indicates unknown status.
    if (status.BatteryFlag & 128) != 0 || status.BatteryLifePercent > 100 {
        return None;
    }
    Some(BatteryMetrics {
        charge_pct: f32::from(status.BatteryLifePercent),
        ac_connected: status.ACLineStatus == 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn windows_collector_samples_real_system_metrics() {
        let mut collector = WindowsCollector::new().expect("WindowsCollector should initialize");
        thread::sleep(Duration::from_millis(80));
        let snap = collector
            .collect()
            .expect("WindowsCollector should collect snapshot");

        assert!(snap.memory.total_bytes > 0);
        assert!(snap.memory.used_bytes <= snap.memory.total_bytes);
        assert!(snap.uptime_secs > 0);
        assert!((0.0..=100.0).contains(&snap.cpu.total_usage_pct));
        assert!(!snap.disks.is_empty());
    }
}
