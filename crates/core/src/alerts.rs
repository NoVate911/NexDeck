//! Stateful hardware threshold alert monitor emitting [`NotificationEvent`] alerts
//! with configurable cooldowns to prevent notification floods.

use std::collections::HashMap;

use nexdeck_monitoring::SystemSnapshot;
use nexdeck_notifications::{NotificationEvent, NotificationPriority, NotificationSource};
use serde::{Deserialize, Serialize};

/// Configurable hardware and power event notification thresholds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertThresholds {
    /// Master switch for automatic hardware threshold monitoring.
    pub enabled: bool,
    /// Alert when total CPU utilization meets or exceeds this percentage (`0..=100`).
    pub cpu_usage_pct: Option<u8>,
    /// Alert when physical RAM utilization meets or exceeds this percentage (`0..=100`).
    pub ram_usage_pct: Option<u8>,
    /// Alert when reported CPU package temperature meets or exceeds this value (°C).
    pub cpu_temp_c: Option<u16>,
    /// Alert when any GPU temperature meets or exceeds this value (°C).
    pub gpu_temp_c: Option<u16>,
    /// Alert when battery charge falls to or below this percentage while on battery power.
    pub battery_low_pct: Option<u8>,
    /// Minimum interval in seconds before repeating an alert of the same category.
    pub cooldown_secs: u64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            enabled: true,
            cpu_usage_pct: Some(95),
            ram_usage_pct: Some(95),
            cpu_temp_c: Some(90),
            gpu_temp_c: Some(90),
            battery_low_pct: Some(15),
            cooldown_secs: 120,
        }
    }
}

/// Evaluates successive [`SystemSnapshot`] frames against [`AlertThresholds`].
#[derive(Debug, Default)]
pub struct HardwareAlertMonitor {
    last_fired_ms: HashMap<&'static str, u64>,
    last_ac_connected: Option<bool>,
}

impl HardwareAlertMonitor {
    /// Creates a new [`HardwareAlertMonitor`] with empty cooldown state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluates `snapshot` under `thresholds` and produces newly triggered events.
    pub fn evaluate<F>(
        &mut self,
        snapshot: &SystemSnapshot,
        thresholds: &AlertThresholds,
        mut next_id: F,
    ) -> Vec<NotificationEvent>
    where
        F: FnMut() -> u64,
    {
        if !thresholds.enabled {
            return Vec::new();
        }

        let now_ms = snapshot.captured_at_epoch_ms;
        let cooldown_ms = thresholds.cooldown_secs.saturating_mul(1_000);
        let mut emitted = Vec::new();

        if let Some(limit) = thresholds.cpu_usage_pct
            && snapshot.cpu.total_usage_pct >= f32::from(limit)
            && self.cooldown_ready("cpu_usage", now_ms, cooldown_ms)
        {
            emitted.push(NotificationEvent {
                id: next_id(),
                source: NotificationSource::HardwareAlert,
                title: "Высокая нагрузка на CPU".to_owned(),
                body: format!(
                    "Загрузка процессора достигла {:.1}% (порог {}%).",
                    snapshot.cpu.total_usage_pct, limit
                ),
                priority: NotificationPriority::High,
                created_at_epoch_ms: now_ms,
            });
        }

        let ram_pct = snapshot.memory.usage_pct();
        if let Some(limit) = thresholds.ram_usage_pct
            && ram_pct >= f32::from(limit)
            && self.cooldown_ready("ram_usage", now_ms, cooldown_ms)
        {
            emitted.push(NotificationEvent {
                id: next_id(),
                source: NotificationSource::HardwareAlert,
                title: "Критическое заполнение RAM".to_owned(),
                body: format!(
                    "Занято {:.1}% оперативной памяти (порог {}%).",
                    ram_pct, limit
                ),
                priority: NotificationPriority::High,
                created_at_epoch_ms: now_ms,
            });
        }

        if let Some(limit) = thresholds.cpu_temp_c
            && let Some(temp) = snapshot.cpu.temperature_celsius
            && temp >= f32::from(limit)
            && self.cooldown_ready("cpu_temp", now_ms, cooldown_ms)
        {
            emitted.push(NotificationEvent {
                id: next_id(),
                source: NotificationSource::HardwareAlert,
                title: "Перегрев процессора".to_owned(),
                body: format!("Температура CPU: {:.1} °C (порог {} °C).", temp, limit),
                priority: NotificationPriority::Critical,
                created_at_epoch_ms: now_ms,
            });
        }

        if let Some(limit) = thresholds.gpu_temp_c
            && let Some((gpu_name, temp)) = snapshot
                .gpus
                .iter()
                .filter_map(|gpu| gpu.temperature_celsius.map(|t| (&gpu.name, t)))
                .max_by(|a, b| a.1.total_cmp(&b.1))
            && temp >= f32::from(limit)
            && self.cooldown_ready("gpu_temp", now_ms, cooldown_ms)
        {
            emitted.push(NotificationEvent {
                id: next_id(),
                source: NotificationSource::HardwareAlert,
                title: "Перегрев видеокарты".to_owned(),
                body: format!("GPU {gpu_name}: {temp:.1} °C (порог {limit} °C)."),
                priority: NotificationPriority::Critical,
                created_at_epoch_ms: now_ms,
            });
        }

        if let Some(battery) = &snapshot.battery {
            if let Some(prev_ac) = self.last_ac_connected
                && prev_ac != battery.ac_connected
            {
                emitted.push(NotificationEvent {
                    id: next_id(),
                    source: NotificationSource::WindowsSystem,
                    title: if battery.ac_connected {
                        "Питание от сети подключено".to_owned()
                    } else {
                        "ПК перешёл на питание от батареи".to_owned()
                    },
                    body: format!("Текущий заряд: {:.0}%.", battery.charge_pct),
                    priority: NotificationPriority::Normal,
                    created_at_epoch_ms: now_ms,
                });
            }
            self.last_ac_connected = Some(battery.ac_connected);

            if !battery.ac_connected
                && let Some(limit) = thresholds.battery_low_pct
                && battery.charge_pct <= f32::from(limit)
                && self.cooldown_ready("battery_low", now_ms, cooldown_ms)
            {
                emitted.push(NotificationEvent {
                    id: next_id(),
                    source: NotificationSource::HardwareAlert,
                    title: "Низкий заряд батареи".to_owned(),
                    body: format!(
                        "Осталось {:.0}% заряда (порог {}%).",
                        battery.charge_pct, limit
                    ),
                    priority: NotificationPriority::High,
                    created_at_epoch_ms: now_ms,
                });
            }
        }

        emitted
    }

    fn cooldown_ready(&mut self, key: &'static str, now_ms: u64, cooldown_ms: u64) -> bool {
        let ready = match self.last_fired_ms.get(key) {
            None => true,
            Some(&prev) => now_ms.saturating_sub(prev) >= cooldown_ms,
        };
        if ready {
            self.last_fired_ms.insert(key, now_ms);
        }
        ready
    }
}

#[cfg(test)]
mod tests {
    use nexdeck_monitoring::{BatteryMetrics, CpuMetrics, GpuMetrics, MemoryMetrics};

    use super::*;

    fn snapshot_at(
        epoch_ms: u64,
        cpu_pct: f32,
        ram_used_gb: u64,
        cpu_temp: Option<f32>,
        ac: bool,
        battery_pct: f32,
    ) -> SystemSnapshot {
        SystemSnapshot {
            captured_at_epoch_ms: epoch_ms,
            uptime_secs: 300,
            cpu: CpuMetrics {
                total_usage_pct: cpu_pct,
                per_core_usage_pct: vec![cpu_pct],
                frequency_mhz: Some(4_000),
                temperature_celsius: cpu_temp,
            },
            memory: MemoryMetrics {
                total_bytes: 16 * 1024 * 1024 * 1024,
                used_bytes: ram_used_gb * 1024 * 1024 * 1024,
            },
            gpus: vec![GpuMetrics {
                name: "ExampleGPU".to_owned(),
                usage_pct: Some(50.0),
                temperature_celsius: Some(60.0),
                clock_mhz: Some(1500),
                vram_used_bytes: Some(0),
                vram_total_bytes: Some(0),
            }],
            disks: Vec::new(),
            networks: Vec::new(),
            battery: Some(BatteryMetrics {
                charge_pct: battery_pct,
                ac_connected: ac,
            }),
        }
    }

    #[test]
    fn hardware_monitor_triggers_on_thresholds_power_change_and_honors_cooldown() {
        let thresholds = AlertThresholds {
            enabled: true,
            cpu_usage_pct: Some(90),
            ram_usage_pct: Some(90),
            cpu_temp_c: Some(85),
            gpu_temp_c: Some(90),
            battery_low_pct: Some(20),
            cooldown_secs: 60,
        };
        let mut monitor = HardwareAlertMonitor::new();
        let mut next_id = 1_u64;
        let mut alloc_id = || {
            let id = next_id;
            next_id += 1;
            id
        };

        // Normal baseline produces no alerts.
        let normal = snapshot_at(100_000, 20.0, 4, Some(50.0), true, 80.0);
        assert!(
            monitor
                .evaluate(&normal, &thresholds, &mut alloc_id)
                .is_empty()
        );

        // Unplug AC + overheat + high CPU + low battery fires alerts cleanly.
        let spike = snapshot_at(110_000, 96.0, 4, Some(88.0), false, 12.0);
        let first_wave = monitor.evaluate(&spike, &thresholds, &mut alloc_id);
        assert_eq!(first_wave.len(), 4);
        assert!(
            first_wave
                .iter()
                .any(|e| e.priority == NotificationPriority::Critical)
        );
        assert!(
            first_wave
                .iter()
                .any(|e| e.source == NotificationSource::WindowsSystem)
        );

        // Within cooldown (20s < 60s), repeated CPU/temp/battery spikes are suppressed.
        let repeat = snapshot_at(130_000, 98.0, 4, Some(89.0), false, 10.0);
        assert!(
            monitor
                .evaluate(&repeat, &thresholds, &mut alloc_id)
                .is_empty()
        );

        // After cooldown elapses (>= 60s), alerts may fire again.
        let later = snapshot_at(175_000, 95.0, 15, Some(86.0), false, 9.0);
        let second_wave = monitor.evaluate(&later, &thresholds, &mut alloc_id);
        assert_eq!(second_wave.len(), 4);
    }
}
