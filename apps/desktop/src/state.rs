//! Headless application state, persistence orchestration, and panel builder logic.
//!
//! Kept independent of `egui`/`eframe` so all configuration edits, device
//! revocations, and grid collision rules can be unit-tested without a display server.

use std::collections::VecDeque;
use std::fs;
use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use nexdeck_api::ApiPermission;
use nexdeck_api::clients::{ClientRecord, ClientRegistry, IssuedClientCredentials};
use nexdeck_api::panels::{
    DeckConfig, MAX_PANEL_COLUMNS, MAX_REFRESH_INTERVAL_MS, MIN_PANEL_COLUMNS,
    MIN_REFRESH_INTERVAL_MS, PanelConfig, TelemetryBinding, WidgetAction, WidgetIcon, WidgetKind,
    WidgetSpec, WidgetStyle,
};
use nexdeck_core::AGENT_VERSION;
use nexdeck_core::alerts::AlertThresholds;
use nexdeck_core::autostart::{
    AutostartError, format_autostart_command, install_autostart, query_autostart,
    uninstall_autostart,
};
use nexdeck_core::storage::{AgentSettings, StoragePaths};
use nexdeck_notifications::{NotificationEvent, NotificationFilter, NotificationPriority};
use nexdeck_transport::control::ControlAction;
use nexdeck_transport::pairing::{DeviceRecord, PairingQrPayload};
use nexdeck_transport::{PROTOCOL_VERSION, TransportKind};

const MAX_LOG_LINES: usize = 120;
const GRID_SEARCH_MAX_ROWS: u16 = 128;

/// Primary navigation sections displayed in the Desktop Manager shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopTab {
    /// Visual panel and widget layout editor (`panels.json`).
    Panels,
    /// Core Agent runtime launcher, autostart, and `config.json` editor.
    AgentSettings,
    /// Paired device revocation (`devices.json`) and TLS fingerprint tools.
    DevicesAndTls,
}

/// Captures streamed stdout/stderr lines and any parsed pairing QR/PIN banner.
#[derive(Debug, Default)]
struct AgentOutputState {
    lines: VecDeque<String>,
    json_accumulator: String,
    capturing_pairing_json: bool,
    latest_pin: Option<String>,
    latest_pairing_json: Option<String>,
}

impl AgentOutputState {
    fn push_line(&mut self, prefix: &str, raw_line: &str) {
        let trimmed = raw_line.trim_end();
        if self.lines.len() >= MAX_LOG_LINES {
            self.lines.pop_front();
        }
        if prefix.is_empty() {
            self.lines.push_back(trimmed.to_owned());
        } else {
            self.lines.push_back(format!("[{prefix}] {trimmed}"));
        }

        if let Some(pin) = extract_pin_from_banner(trimmed) {
            self.latest_pin = Some(pin);
            self.json_accumulator.clear();
            self.capturing_pairing_json = true;
            return;
        }

        if self.capturing_pairing_json {
            self.json_accumulator.push_str(trimmed);
            self.json_accumulator.push('\n');
            if trimmed == "}" {
                if serde_json::from_str::<PairingQrPayload>(&self.json_accumulator).is_ok() {
                    self.latest_pairing_json = Some(self.json_accumulator.trim().to_owned());
                }
                self.capturing_pairing_json = false;
                self.json_accumulator.clear();
            }
        }
    }
}

/// Manages an optional supervised `nexdeck-agent` child process launched by the UI.
#[derive(Debug)]
pub struct AgentProcessController {
    child: Option<Child>,
    output: Arc<Mutex<AgentOutputState>>,
    pair_mode: bool,
    last_exit_note: Option<String>,
}

impl Default for AgentProcessController {
    fn default() -> Self {
        Self {
            child: None,
            output: Arc::new(Mutex::new(AgentOutputState::default())),
            pair_mode: false,
            last_exit_note: None,
        }
    }
}

impl Drop for AgentProcessController {
    fn drop(&mut self) {
        self.stop();
    }
}

impl AgentProcessController {
    /// Returns `true` if a child `nexdeck-agent` process is currently running.
    pub fn is_running(&mut self) -> bool {
        self.poll();
        self.child.is_some()
    }

    /// Returns whether the active/last child process was launched with `--pair`.
    #[must_use]
    pub const fn is_pair_mode(&self) -> bool {
        self.pair_mode
    }

    /// Returns the most recent child exit status description, if it exited.
    #[must_use]
    pub fn last_exit_note(&self) -> Option<&str> {
        self.last_exit_note.as_deref()
    }

    /// Returns buffered log lines emitted by the child agent process.
    #[must_use]
    pub fn log_lines(&self) -> Vec<String> {
        match self.output.lock() {
            Ok(guard) => guard.lines.iter().cloned().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Returns the latest pairing PIN and QR JSON parsed from `--pair` output.
    #[must_use]
    pub fn latest_pairing_info(&self) -> (Option<String>, Option<String>) {
        match self.output.lock() {
            Ok(guard) => (guard.latest_pin.clone(), guard.latest_pairing_json.clone()),
            Err(_) => (None, None),
        }
    }

    /// Checks whether the managed child has exited and updates internal state.
    pub fn poll(&mut self) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        if let Ok(Some(status)) = child.try_wait() {
            self.last_exit_note = Some(format!("Процесс агента завершился ({status})"));
            self.child = None;
        }
    }

    /// Spawns `nexdeck-agent` with `--daemon`, `--config-dir`, and optional `--pair`.
    ///
    /// # Errors
    ///
    /// Returns a Russian error description if an agent child is already running, the binary
    /// cannot be located, or spawning fails.
    pub fn start(
        &mut self,
        agent_exe: &Path,
        config_dir: &Path,
        bind_addr: SocketAddr,
        pair_mode: bool,
        pair_ttl_secs: u64,
    ) -> Result<(), String> {
        self.poll();
        if self.child.is_some() {
            return Err("Агент уже запущен из Desktop Manager.".to_owned());
        }

        if let Ok(mut guard) = self.output.lock() {
            guard.lines.clear();
            guard.latest_pin = None;
            guard.latest_pairing_json = None;
            guard.capturing_pairing_json = false;
            guard.json_accumulator.clear();
        }

        let mut cmd = Command::new(agent_exe);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.arg("--daemon")
            .arg("--config-dir")
            .arg(config_dir)
            .arg("--bind")
            .arg(bind_addr.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if pair_mode {
            cmd.arg("--pair")
                .arg("--ttl")
                .arg(pair_ttl_secs.max(15).to_string());
        }

        let mut child = cmd
            .spawn()
            .map_err(|err| format!("Не удалось запустить {}: {err}", agent_exe.display()))?;

        if let Some(stdout) = child.stdout.take() {
            spawn_output_reader(stdout, Arc::clone(&self.output), "");
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_output_reader(stderr, Arc::clone(&self.output), "stderr");
        }

        self.pair_mode = pair_mode;
        self.last_exit_note = None;
        self.child = Some(child);
        Ok(())
    }

    /// Terminates and reaps the managed `nexdeck-agent` child process if running.
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
            self.last_exit_note = Some("Агент остановлен пользователем.".to_owned());
        }
    }
}

fn spawn_output_reader<R: std::io::Read + Send + 'static>(
    reader: R,
    output: Arc<Mutex<AgentOutputState>>,
    prefix: &'static str,
) {
    thread::spawn(move || {
        let buffered = BufReader::new(reader);
        for line in buffered.lines().map_while(Result::ok) {
            if let Ok(mut guard) = output.lock() {
                guard.push_line(prefix, &line);
            }
        }
    });
}

/// Draft text inputs backing the `config.json` settings editor form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsDraft {
    /// Socket address (`ip:port`) string.
    pub bind_addr: String,
    /// Comma-separated TLS Subject Alternative Name hosts.
    pub san_hosts_csv: String,
    /// Telemetry polling cadence in milliseconds.
    pub poll_interval_ms: String,
    /// Notification buffer capacity.
    pub notification_queue_capacity: String,
    /// Whether the isolated WASM plugin engine is enabled.
    pub enable_plugins: bool,
    /// Minimum priority forwarded/buffered for notifications.
    pub min_notification_priority: NotificationPriority,
    /// Comma-separated muted application/plugin names.
    pub muted_origins_csv: String,
    /// Whether automated hardware threshold monitoring is enabled.
    pub alerts_enabled: bool,
    /// CPU load alert threshold (`0..=100`, empty = disabled).
    pub alert_cpu_pct: String,
    /// RAM utilization alert threshold (`0..=100`, empty = disabled).
    pub alert_ram_pct: String,
    /// CPU temperature alert threshold in °C (empty = disabled).
    pub alert_cpu_temp_c: String,
    /// GPU temperature alert threshold in °C (empty = disabled).
    pub alert_gpu_temp_c: String,
    /// Low battery alert threshold (`0..=100`, empty = disabled).
    pub alert_battery_low_pct: String,
    /// Cooldown in seconds between repeated hardware alerts of the same kind.
    pub alert_cooldown_secs: String,
}

impl SettingsDraft {
    /// Creates an editable draft from validated [`AgentSettings`].
    #[must_use]
    pub fn from_settings(settings: &AgentSettings) -> Self {
        Self {
            bind_addr: settings.bind_addr.clone(),
            san_hosts_csv: settings.san_hosts.join(", "),
            poll_interval_ms: settings.agent.poll_interval_ms.to_string(),
            notification_queue_capacity: settings.agent.notification_queue_capacity.to_string(),
            enable_plugins: settings.agent.enable_plugins,
            min_notification_priority: settings.agent.notification_filter.min_priority,
            muted_origins_csv: settings.agent.notification_filter.muted_origins.join(", "),
            alerts_enabled: settings.agent.alert_thresholds.enabled,
            alert_cpu_pct: format_opt_num(settings.agent.alert_thresholds.cpu_usage_pct),
            alert_ram_pct: format_opt_num(settings.agent.alert_thresholds.ram_usage_pct),
            alert_cpu_temp_c: format_opt_num(settings.agent.alert_thresholds.cpu_temp_c),
            alert_gpu_temp_c: format_opt_num(settings.agent.alert_thresholds.gpu_temp_c),
            alert_battery_low_pct: format_opt_num(settings.agent.alert_thresholds.battery_low_pct),
            alert_cooldown_secs: settings.agent.alert_thresholds.cooldown_secs.to_string(),
        }
    }

    /// Parses and validates all draft fields into [`AgentSettings`].
    ///
    /// # Errors
    ///
    /// Returns a readable validation error message if any field is malformed.
    pub fn build_validated_settings(
        &self,
        template: &AgentSettings,
    ) -> Result<AgentSettings, String> {
        let poll_interval_ms: u64 = self
            .poll_interval_ms
            .trim()
            .parse()
            .map_err(|_| "Интервал опроса должен быть целым числом >= 100 мс.".to_owned())?;
        let queue_capacity: usize = self
            .notification_queue_capacity
            .trim()
            .parse()
            .map_err(|_| "Ёмкость очереди уведомлений должна быть целым числом > 0.".to_owned())?;

        let san_hosts: Vec<String> = self
            .san_hosts_csv
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        if san_hosts.is_empty() {
            return Err(
                "Укажите хотя бы один SAN-хост (например: localhost, 127.0.0.1).".to_owned(),
            );
        }

        let mut notification_filter = NotificationFilter {
            min_priority: self.min_notification_priority,
            muted_origins: self
                .muted_origins_csv
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
        };
        notification_filter.normalize();

        let cooldown_secs: u64 = self
            .alert_cooldown_secs
            .trim()
            .parse()
            .map_err(|_| "Пауза повтора тревоги должна быть целым числом (сек).".to_owned())?;

        let alert_thresholds = AlertThresholds {
            enabled: self.alerts_enabled,
            cpu_usage_pct: parse_opt_pct(&self.alert_cpu_pct, "Порог CPU (%)")?,
            ram_usage_pct: parse_opt_pct(&self.alert_ram_pct, "Порог RAM (%)")?,
            cpu_temp_c: parse_opt_temp(&self.alert_cpu_temp_c, "Порог темп. CPU (°C)")?,
            gpu_temp_c: parse_opt_temp(&self.alert_gpu_temp_c, "Порог темп. GPU (°C)")?,
            battery_low_pct: parse_opt_pct(
                &self.alert_battery_low_pct,
                "Низкий заряд батареи (%)",
            )?,
            cooldown_secs,
        };

        let mut next = template.clone();
        next.bind_addr = self.bind_addr.trim().to_owned();
        next.san_hosts = san_hosts;
        next.agent.primary_transport = TransportKind::WifiWebSocket;
        next.agent.poll_interval_ms = poll_interval_ms;
        next.agent.notification_queue_capacity = queue_capacity;
        next.agent.enable_plugins = self.enable_plugins;
        next.agent.notification_filter = notification_filter;
        next.agent.alert_thresholds = alert_thresholds;

        let _ = next
            .validate()
            .map_err(|err| format!("Ошибка проверки настроек: {err}"))?;
        Ok(next)
    }
}

fn format_opt_num<T: ToString>(value: Option<T>) -> String {
    value.map_or_else(String::new, |num| num.to_string())
}

fn parse_opt_pct(raw: &str, label: &str) -> Result<Option<u8>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let val: u8 = trimmed
        .parse()
        .map_err(|_| format!("{label}: введите целое число от 1 до 100 или оставьте пустым."))?;
    if val > 100 || val == 0 {
        return Err(format!("{label}: допустимое значение от 1 до 100%."));
    }
    Ok(Some(val))
}

fn parse_opt_temp(raw: &str, label: &str) -> Result<Option<u16>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let val: u16 = trimmed
        .parse()
        .map_err(|_| format!("{label}: введите целую температуру (°C) или оставьте пустым."))?;
    if !(20..=150).contains(&val) {
        return Err(format!("{label}: допустимое значение от 20 до 150 °C."));
    }
    Ok(Some(val))
}

fn load_sorted_notifications(paths: &StoragePaths) -> Result<Vec<NotificationEvent>, String> {
    let mut events = paths
        .load_notifications()
        .map_err(|err| format!("Ошибка чтения notifications.json: {err}"))?;
    events.sort_by_key(|event| std::cmp::Reverse(event.id));
    Ok(events)
}

/// Complete Desktop Manager state persisted across [`StoragePaths`].
#[derive(Debug)]
#[allow(dead_code)]
pub struct DesktopManagerState {
    /// Active storage layout (`config.json`, `devices.json`, `panels.json`, TLS PEM/sha256).
    pub paths: StoragePaths,
    /// Active UI navigation tab.
    pub active_tab: DesktopTab,
    /// Last saved agent configuration.
    pub settings: AgentSettings,
    /// Editable draft of `config.json` fields.
    pub settings_draft: SettingsDraft,
    /// Registered mobile devices (token hashes only).
    pub devices: Vec<DeviceRecord>,
    /// Registered Universal API applications (`clients.json`, token hashes only).
    pub api_clients: Vec<ClientRecord>,
    /// Buffered notifications queued across sleep/disconnect cycles (`notifications.json`).
    pub buffered_notifications: Vec<NotificationEvent>,
    /// Draft application ID for registering a new API client.
    pub new_api_client_id: String,
    /// Draft application display name for registering a new API client.
    pub new_api_client_name: String,
    /// Draft permission checkboxes aligned with [`ApiPermission::ALL`].
    pub new_api_client_permissions: [bool; 5],
    /// Most recently issued one-time API client credentials (shown until dismissed).
    pub latest_issued_api_client: Option<IssuedClientCredentials>,
    /// Current host TLS certificate SHA-256 fingerprint (64 hex chars).
    pub tls_fingerprint_sha256: String,
    /// Persisted deck containing customizable grid panels and widgets.
    pub deck: DeckConfig,
    /// Currently selected panel index inside `deck.panels`.
    pub selected_panel_idx: usize,
    /// Currently selected widget identifier inside the active panel.
    pub selected_widget_id: Option<String>,
    /// Draft buffer for creating a new panel.
    pub new_panel_name: String,
    /// Draft column count for new panels (`2..=8`).
    pub new_panel_columns: u16,
    /// Draft refresh interval for new panels (`100..=60000` ms).
    pub new_panel_refresh_ms: u64,
    /// Multi-line JSON editor buffer for panel/deck import & export.
    pub json_buffer: String,
    /// Cached Windows autostart registry command, if configured.
    pub autostart_command: Option<String>,
    /// Human-readable platform note if autostart querying is unsupported.
    pub autostart_note: Option<String>,
    /// Pairing TTL used when launching `nexdeck-agent --daemon --pair`.
    pub pair_ttl_secs: u64,
    /// Managed background `nexdeck-agent` process supervisor.
    pub agent_process: AgentProcessController,
    /// Positive confirmation banner shown in the status bar.
    pub status_message: Option<String>,
    /// Validation/I/O error banner shown in the status bar.
    pub error_message: Option<String>,
}

#[allow(dead_code)]
impl DesktopManagerState {
    /// Loads settings, devices, TLS identity, panel layout, and autostart state from `paths`.
    ///
    /// # Errors
    ///
    /// Returns a readable message if initialization or validation of persisted files fails.
    pub fn load(paths: StoragePaths) -> Result<Self, String> {
        let settings = paths
            .load_or_init_settings()
            .map_err(|err| format!("Не удалось загрузить config.json: {err}"))?;
        let devices = paths
            .load_devices()
            .map_err(|err| format!("Не удалось загрузить devices.json: {err}"))?;
        let api_clients = paths
            .load_clients()
            .map_err(|err| format!("Не удалось загрузить clients.json: {err}"))?;
        let buffered_notifications = load_sorted_notifications(&paths)?;
        let tls = paths
            .load_or_create_tls_identity(&settings.san_hosts)
            .map_err(|err| format!("Не удалось загрузить TLS-идентичность: {err}"))?;
        let deck = paths
            .load_or_init_panels()
            .map_err(|err| format!("Не удалось загрузить panels.json: {err}"))?;

        let selected_panel_idx = deck
            .panels
            .iter()
            .position(|panel| panel.id == deck.active_panel_id)
            .unwrap_or(0);
        let selected_widget_id = deck
            .panels
            .get(selected_panel_idx)
            .and_then(|panel| panel.widgets.first())
            .map(|widget| widget.id.clone());

        let (autostart_command, autostart_note) = read_autostart_status();
        let settings_draft = SettingsDraft::from_settings(&settings);

        Ok(Self {
            paths,
            active_tab: DesktopTab::Panels,
            settings,
            settings_draft,
            devices,
            api_clients,
            buffered_notifications,
            new_api_client_id: "org.example.client".to_owned(),
            new_api_client_name: "ExampleApp".to_owned(),
            new_api_client_permissions: [true, true, true, false, false],
            latest_issued_api_client: None,
            tls_fingerprint_sha256: tls.fingerprint_sha256,
            deck,
            selected_panel_idx,
            selected_widget_id,
            new_panel_name: "Новая панель".to_owned(),
            new_panel_columns: 4,
            new_panel_refresh_ms: 1_000,
            json_buffer: String::new(),
            autostart_command,
            autostart_note,
            pair_ttl_secs: 300,
            agent_process: AgentProcessController::default(),
            status_message: Some(format!(
                "Готово (агент v{AGENT_VERSION}, протокол v{PROTOCOL_VERSION})."
            )),
            error_message: None,
        })
    }

    /// Reloads paired devices, TLS fingerprint, settings, and autostart state from disk.
    pub fn refresh_from_disk(&mut self) {
        self.clear_alerts();
        match self.paths.load_or_init_settings() {
            Ok(settings) => {
                self.settings = settings;
                self.settings_draft = SettingsDraft::from_settings(&self.settings);
            }
            Err(err) => {
                self.error_message = Some(format!("Ошибка чтения config.json: {err}"));
                return;
            }
        }
        match self.paths.load_devices() {
            Ok(devices) => self.devices = devices,
            Err(err) => {
                self.error_message = Some(format!("Ошибка чтения devices.json: {err}"));
                return;
            }
        }
        match self.paths.load_clients() {
            Ok(clients) => self.api_clients = clients,
            Err(err) => {
                self.error_message = Some(format!("Ошибка чтения clients.json: {err}"));
                return;
            }
        }
        match load_sorted_notifications(&self.paths) {
            Ok(events) => self.buffered_notifications = events,
            Err(msg) => {
                self.error_message = Some(msg);
                return;
            }
        }
        match self
            .paths
            .load_or_create_tls_identity(&self.settings.san_hosts)
        {
            Ok(tls) => self.tls_fingerprint_sha256 = tls.fingerprint_sha256,
            Err(err) => {
                self.error_message = Some(format!("Ошибка чтения TLS-сертификата: {err}"));
                return;
            }
        }
        let (autostart_command, autostart_note) = read_autostart_status();
        self.autostart_command = autostart_command;
        self.autostart_note = autostart_note;
        self.status_message = Some("Данные обновлены с диска.".to_owned());
    }

    /// Validates and saves edited `config.json` fields.
    pub fn save_settings_draft(&mut self) {
        self.clear_alerts();
        match self.settings_draft.build_validated_settings(&self.settings) {
            Ok(next) => match self.paths.save_settings(&next) {
                Ok(()) => {
                    self.settings = next;
                    self.settings_draft = SettingsDraft::from_settings(&self.settings);
                    self.status_message =
                        Some("Настройки агента сохранены в config.json.".to_owned());
                }
                Err(err) => {
                    self.error_message = Some(format!("Не удалось записать config.json: {err}"));
                }
            },
            Err(msg) => self.error_message = Some(msg),
        }
    }

    /// Discards unsaved form edits and restores values from `self.settings`.
    pub fn reset_settings_draft(&mut self) {
        self.clear_alerts();
        self.settings_draft = SettingsDraft::from_settings(&self.settings);
        self.status_message = Some("Изменения формы настроек сброшены.".to_owned());
    }

    /// Reloads persisted sleep/disconnect notifications from `notifications.json`.
    pub fn reload_notifications_buffer(&mut self) {
        self.clear_alerts();
        match load_sorted_notifications(&self.paths) {
            Ok(events) => {
                self.buffered_notifications = events;
                self.status_message = Some(format!(
                    "Обновлён буфер уведомлений ({} записей).",
                    self.buffered_notifications.len()
                ));
            }
            Err(msg) => {
                self.error_message = Some(msg);
            }
        }
    }

    /// Clears all persisted sleep/disconnect notifications from `notifications.json`.
    pub fn clear_notifications_buffer(&mut self) {
        self.clear_alerts();
        match self.paths.save_notifications(&[]) {
            Ok(()) => {
                self.buffered_notifications.clear();
                self.status_message =
                    Some("Буфер уведомлений очищен (notifications.json).".to_owned());
            }
            Err(err) => {
                self.error_message = Some(format!("Не удалось очистить notifications.json: {err}"));
            }
        }
    }

    /// Installs or removes Windows current-user autostart targeting `nexdeck-agent`.
    pub fn set_autostart_enabled(&mut self, enable: bool) {
        self.clear_alerts();
        if !enable {
            match uninstall_autostart() {
                Ok(true) => {
                    self.autostart_command = None;
                    self.status_message = Some("Автозапуск NexDeckAgent удалён.".to_owned());
                }
                Ok(false) => {
                    self.autostart_command = None;
                    self.status_message = Some("Запись автозапуска уже отсутствовала.".to_owned());
                }
                Err(err) => {
                    self.error_message = Some(format!("Не удалось отключить автозапуск: {err}"));
                }
            }
            return;
        }

        let agent_exe = match resolve_agent_binary_path() {
            Ok(path) => path,
            Err(msg) => {
                self.error_message = Some(msg);
                return;
            }
        };
        let bind_addr = match self.settings.validate() {
            Ok(addr) => addr,
            Err(err) => {
                self.error_message = Some(format!("Некорректный адрес привязки: {err}"));
                return;
            }
        };
        let command = match format_autostart_command(&agent_exe, Some(bind_addr)) {
            Ok(cmd) => cmd,
            Err(err) => {
                self.error_message = Some(format!("Некорректный путь агента: {err}"));
                return;
            }
        };

        match install_autostart(&command) {
            Ok(()) => {
                self.autostart_command = Some(command);
                self.status_message =
                    Some("Автозапуск NexDeckAgent включён в HKCU\\...\\Run.".to_owned());
            }
            Err(err) => {
                self.error_message = Some(format!("Не удалось включить автозапуск: {err}"));
            }
        }
    }

    /// Starts the local `nexdeck-agent` binary using current `StoragePaths` and bind address.
    pub fn start_agent(&mut self, pair_mode: bool) {
        self.clear_alerts();
        let bind_addr = match self.settings.validate() {
            Ok(addr) => addr,
            Err(err) => {
                self.error_message = Some(format!("Невозможно запустить агента: {err}"));
                return;
            }
        };
        let agent_exe = resolve_agent_binary_command_target();
        match self.agent_process.start(
            &agent_exe,
            self.paths.root_dir(),
            bind_addr,
            pair_mode,
            self.pair_ttl_secs,
        ) {
            Ok(()) => {
                self.status_message = Some(if pair_mode {
                    "Агент запущен в режиме сопряжения (--pair).".to_owned()
                } else {
                    "Агент запущен в фоновом режиме (--daemon).".to_owned()
                });
            }
            Err(msg) => self.error_message = Some(msg),
        }
    }

    /// Stops the supervised `nexdeck-agent` child process if running.
    pub fn stop_agent(&mut self) {
        self.clear_alerts();
        self.agent_process.stop();
        self.status_message = Some("Команда остановки отправлена агенту.".to_owned());
    }

    /// Starts or restarts `nexdeck-agent` in pairing mode (`--pair`) in one click,
    /// switching `127.0.0.1` to `0.0.0.0` if a LAN IP is detected so Wi-Fi phones can reach the PC.
    pub fn connect_phone_one_click(&mut self) {
        self.clear_alerts();
        if self.settings.bind_addr.starts_with("127.0.0.1:")
            && detect_lan_ipv4().is_some()
            && let Some(port) = self.settings.bind_addr.strip_prefix("127.0.0.1:")
        {
            let updated_bind = format!("0.0.0.0:{port}");
            self.settings.bind_addr.clone_from(&updated_bind);
            self.settings_draft.bind_addr = updated_bind;
            let _ = self.paths.save_settings(&self.settings);
        }

        if self.agent_process.is_running() {
            if self.agent_process.is_pair_mode()
                && self.agent_process.latest_pairing_info().0.is_some()
            {
                return;
            }
            self.agent_process.stop();
        }
        self.pair_ttl_secs = self.pair_ttl_secs.max(300);
        self.start_agent(true);
    }

    /// Returns `(pin, display_host_port, qr_json)` with the PC's LAN IP substituted when available.
    #[must_use]
    pub fn resolved_pairing_qr_payload(&self) -> Option<(String, String, String)> {
        let (pin, raw_json) = self.agent_process.latest_pairing_info();
        let pin = pin?;
        let raw_json = raw_json?;
        let Ok(mut payload) = serde_json::from_str::<PairingQrPayload>(&raw_json) else {
            return Some((pin, self.settings.bind_addr.clone(), raw_json));
        };
        if (payload.host == "127.0.0.1" || payload.host == "0.0.0.0" || payload.host == "localhost")
            && let Some(lan_ip) = detect_lan_ipv4()
        {
            payload.host = lan_ip;
        }
        let host_port = format!("{}:{}", payload.host, payload.port);
        let final_json = serde_json::to_string(&payload).unwrap_or(raw_json);
        Some((pin, host_port, final_json))
    }

    /// Revokes a paired mobile device by identifier and updates `devices.json`.
    pub fn revoke_device(&mut self, device_id: &str) {
        self.clear_alerts();
        let before = self.devices.len();
        let mut next: Vec<DeviceRecord> = self
            .devices
            .iter()
            .filter(|record| record.device_id != device_id)
            .cloned()
            .collect();

        if next.len() == before {
            self.error_message = Some(format!("Устройство '{device_id}' не найдено."));
            return;
        }

        match self.paths.save_devices(&next) {
            Ok(()) => {
                next.sort_by(|left, right| left.device_id.cmp(&right.device_id));
                self.devices = next;
                self.status_message = Some(format!(
                    "Доступ устройства '{device_id}' отозван (требуется повторное сопряжение)."
                ));
            }
            Err(err) => {
                self.error_message = Some(format!("Не удалось обновить devices.json: {err}"));
            }
        }
    }

    /// Registers a new Universal API client application and persists its token hash to `clients.json`.
    pub fn register_api_client(&mut self) {
        self.clear_alerts();
        let mut reg = ClientRegistry::from_records(self.api_clients.clone());
        let perms: Vec<ApiPermission> = ApiPermission::ALL
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(idx, perm)| self.new_api_client_permissions[idx].then_some(perm))
            .collect();
        let now_epoch_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));

        match reg.register_client(
            self.new_api_client_id.trim().to_owned(),
            self.new_api_client_name.trim().to_owned(),
            perms,
            now_epoch_ms,
        ) {
            Ok(issued) => {
                let records = reg.export_records();
                match self.paths.save_clients(&records) {
                    Ok(()) => {
                        self.api_clients = records;
                        self.status_message = Some(format!(
                            "Приложение '{}' зарегистрировано в clients.json.",
                            issued.client_id
                        ));
                        self.latest_issued_api_client = Some(issued);
                    }
                    Err(err) => {
                        self.error_message =
                            Some(format!("Не удалось записать clients.json: {err}"));
                    }
                }
            }
            Err(err) => {
                self.error_message = Some(format!("Ошибка регистрации API-клиента: {err}"));
            }
        }
    }

    /// Toggles the `revoked` flag for an existing API client in `clients.json`.
    pub fn set_api_client_revoked(&mut self, client_id: &str, revoked: bool) {
        self.clear_alerts();
        let mut reg = ClientRegistry::from_records(self.api_clients.clone());
        if let Err(err) = reg.set_revoked(client_id, revoked) {
            self.error_message = Some(format!("Ошибка обновления клиента '{client_id}': {err}"));
            return;
        }
        let records = reg.export_records();
        match self.paths.save_clients(&records) {
            Ok(()) => {
                self.api_clients = records;
                self.status_message = Some(if revoked {
                    format!("Доступ API-клиента '{client_id}' отозван.")
                } else {
                    format!("Доступ API-клиента '{client_id}' восстановлен.")
                });
            }
            Err(err) => {
                self.error_message = Some(format!("Не удалось записать clients.json: {err}"));
            }
        }
    }

    /// Permanently removes a registered API client from `clients.json`.
    pub fn delete_api_client(&mut self, client_id: &str) {
        self.clear_alerts();
        let mut reg = ClientRegistry::from_records(self.api_clients.clone());
        if !reg.remove_client(client_id) {
            self.error_message = Some(format!("API-клиент '{client_id}' не найден."));
            return;
        }
        let records = reg.export_records();
        match self.paths.save_clients(&records) {
            Ok(()) => {
                self.api_clients = records;
                self.status_message = Some(format!("API-клиент '{client_id}' удалён."));
            }
            Err(err) => {
                self.error_message = Some(format!("Не удалось записать clients.json: {err}"));
            }
        }
    }

    /// Regenerates host TLS certificate/key pair using configured SAN hosts.
    pub fn rotate_tls_identity(&mut self) {
        self.clear_alerts();
        for path in [
            self.paths.tls_cert_path(),
            self.paths.tls_key_path(),
            self.paths.tls_fingerprint_path(),
        ] {
            if path.exists()
                && let Err(err) = fs::remove_file(&path)
            {
                self.error_message = Some(format!(
                    "Не удалось удалить старый TLS-файл {}: {err}",
                    path.display()
                ));
                return;
            }
        }

        match self
            .paths
            .load_or_create_tls_identity(&self.settings.san_hosts)
        {
            Ok(tls) => {
                self.tls_fingerprint_sha256 = tls.fingerprint_sha256;
                self.status_message = Some(
                    "TLS-сертификат перевыпущен. Подключённые мобильные клиенты потребуют повторного сопряжения."
                        .to_owned(),
                );
            }
            Err(err) => {
                self.error_message = Some(format!("Ошибка генерации TLS-сертификата: {err}"));
            }
        }
    }

    /// Returns a shared reference to the currently selected [`PanelConfig`].
    #[must_use]
    pub fn selected_panel(&self) -> Option<&PanelConfig> {
        self.deck.panels.get(self.selected_panel_idx)
    }

    /// Returns a cloned copy of the currently selected [`WidgetSpec`], if any.
    #[must_use]
    pub fn selected_widget(&self) -> Option<WidgetSpec> {
        let target_id = self.selected_widget_id.as_deref()?;
        self.selected_panel()?
            .widgets
            .iter()
            .find(|widget| widget.id == target_id)
            .cloned()
    }

    /// Switches selection to panel `idx` and updates `active_panel_id` in `panels.json`.
    pub fn select_panel(&mut self, idx: usize) {
        self.clear_alerts();
        let Some(panel_id) = self.deck.panels.get(idx).map(|panel| panel.id.clone()) else {
            return;
        };

        let mut candidate = self.deck.clone();
        candidate.active_panel_id = panel_id;
        if let Err(err) = self.commit_deck(candidate, None) {
            self.error_message = Some(err);
            return;
        }

        self.selected_panel_idx = idx;
        self.selected_widget_id = self
            .deck
            .panels
            .get(idx)
            .and_then(|panel| panel.widgets.first())
            .map(|widget| widget.id.clone());
    }

    /// Selects a widget inside the currently active panel.
    pub fn select_widget(&mut self, widget_id: &str) {
        self.clear_alerts();
        if let Some(panel) = self.selected_panel()
            && panel.widgets.iter().any(|widget| widget.id == widget_id)
        {
            self.selected_widget_id = Some(widget_id.to_owned());
        }
    }

    /// Creates a new validated panel and selects it cleanly.
    pub fn add_panel(&mut self) {
        self.clear_alerts();
        let trimmed_name = self.new_panel_name.trim();
        let name = if trimmed_name.is_empty() {
            "ExamplePanel"
        } else {
            trimmed_name
        };

        let existing_ids: Vec<&str> = self.deck.panels.iter().map(|p| p.id.as_str()).collect();
        let new_id = next_unique_safe_id("panel", name, &existing_ids);
        let columns = self
            .new_panel_columns
            .clamp(MIN_PANEL_COLUMNS, MAX_PANEL_COLUMNS);
        let refresh_interval_ms = self
            .new_panel_refresh_ms
            .clamp(MIN_REFRESH_INTERVAL_MS, MAX_REFRESH_INTERVAL_MS);

        let panel = PanelConfig {
            id: new_id.clone(),
            name: name.chars().take(64).collect(),
            columns,
            refresh_interval_ms,
            widgets: Vec::new(),
        };

        let mut candidate = self.deck.clone();
        candidate.panels.push(panel);
        candidate.active_panel_id = new_id;
        let new_idx = candidate.panels.len().saturating_sub(1);

        if let Err(msg) =
            self.commit_deck(candidate, Some("Панель создана и сохранена в panels.json."))
        {
            self.error_message = Some(msg);
            return;
        }
        self.selected_panel_idx = new_idx;
        self.selected_widget_id = None;
    }

    /// Updates metadata (name, columns, refresh interval) of the currently selected panel.
    pub fn update_selected_panel_meta(
        &mut self,
        name: String,
        columns: u16,
        refresh_interval_ms: u64,
    ) {
        self.clear_alerts();
        let mut candidate = self.deck.clone();
        let Some(panel) = candidate.panels.get_mut(self.selected_panel_idx) else {
            return;
        };
        panel.name = name;
        panel.columns = columns;
        panel.refresh_interval_ms = refresh_interval_ms;

        if let Err(msg) = self.commit_deck(candidate, Some("Параметры панели обновлены."))
        {
            self.error_message = Some(msg);
        }
    }

    /// Deletes the active panel as long as at least one panel remains in the deck.
    pub fn delete_selected_panel(&mut self) {
        self.clear_alerts();
        if self.deck.panels.len() <= 1 {
            self.error_message =
                Some("В конфигурации должна оставаться хотя бы одна панель.".to_owned());
            return;
        }

        let mut candidate = self.deck.clone();
        let removed_name = candidate.panels.remove(self.selected_panel_idx).name;
        let next_idx = self
            .selected_panel_idx
            .min(candidate.panels.len().saturating_sub(1));
        candidate.active_panel_id = candidate.panels[next_idx].id.clone();

        if let Err(msg) = self.commit_deck(
            candidate,
            Some(&format!("Панель «{removed_name}» удалена.")),
        ) {
            self.error_message = Some(msg);
            return;
        }
        self.selected_panel_idx = next_idx;
        self.selected_widget_id = self
            .deck
            .panels
            .get(next_idx)
            .and_then(|panel| panel.widgets.first())
            .map(|widget| widget.id.clone());
    }

    /// Places a newly constructed widget of `kind` into the first non-overlapping cell.
    pub fn add_widget(&mut self, kind: WidgetKind) {
        self.add_widget_at(kind, None);
    }

    /// Places a newly constructed widget of `kind` at `target_cell` (or the first free cell).
    pub fn add_widget_at(&mut self, kind: WidgetKind, target_cell: Option<(u16, u16)>) {
        self.clear_alerts();
        let mut candidate = self.deck.clone();
        let Some(panel) = candidate.panels.get_mut(self.selected_panel_idx) else {
            return;
        };

        let (default_col_span, default_row_span) = default_spans_for_kind(kind, panel.columns);
        let placement = target_cell
            .and_then(|(c, r)| {
                let clamped_c = c.min(panel.columns.saturating_sub(default_col_span));
                is_region_free(
                    panel,
                    None,
                    clamped_c,
                    r,
                    default_col_span,
                    default_row_span,
                )
                .then_some((clamped_c, r))
            })
            .or_else(|| find_free_cell(panel, default_col_span, default_row_span));

        let Some((col, row)) = placement else {
            self.error_message = Some("Не найдена свободная ячейка в сетке панели.".to_owned());
            return;
        };

        let existing_ids: Vec<&str> = panel.widgets.iter().map(|w| w.id.as_str()).collect();
        let id = next_unique_safe_id("widget", widget_kind_slug(kind), &existing_ids);
        let spec = default_widget_spec(
            id.clone(),
            kind,
            col,
            row,
            default_col_span,
            default_row_span,
        );
        panel.widgets.push(spec);

        if let Err(msg) = self.commit_deck(candidate, Some("Виджет добавлен на панель."))
        {
            self.error_message = Some(msg);
            return;
        }
        self.selected_widget_id = Some(id);
    }

    /// Places a ready-to-use [`WidgetPreset`] onto `target_cell` (from drag-and-drop) or the first free cell.
    pub fn add_preset_at(&mut self, preset: WidgetPreset, target_cell: Option<(u16, u16)>) {
        self.clear_alerts();
        let mut candidate = self.deck.clone();
        let Some(panel) = candidate.panels.get_mut(self.selected_panel_idx) else {
            return;
        };

        let (raw_w, raw_h) = preset.default_spans();
        let col_span = raw_w.min(panel.columns).max(1);
        let row_span = raw_h.max(1);

        let placement = target_cell
            .and_then(|(c, r)| {
                let clamped_c = c.min(panel.columns.saturating_sub(col_span));
                is_region_free(panel, None, clamped_c, r, col_span, row_span)
                    .then_some((clamped_c, r))
            })
            .or_else(|| find_free_cell(panel, col_span, row_span));

        let Some((col, row)) = placement else {
            self.error_message = Some("На панели нет свободного места для виджета.".to_owned());
            return;
        };

        let existing_ids: Vec<&str> = panel.widgets.iter().map(|w| w.id.as_str()).collect();
        let id = next_unique_safe_id("widget", preset.slug(), &existing_ids);
        let spec = preset.build_spec(id.clone(), col, row, col_span, row_span);
        panel.widgets.push(spec);

        if let Err(msg) = self.commit_deck(
            candidate,
            Some(&format!("Добавлен виджет «{}».", preset.title())),
        ) {
            self.error_message = Some(msg);
            return;
        }
        self.selected_widget_id = Some(id);
    }

    /// Moves `widget_id` directly to `(target_col, target_row)` (via mouse drag-and-drop),
    /// automatically swapping positions if dropped onto another widget of identical size.
    pub fn move_widget_to_cell(&mut self, widget_id: &str, target_col: u16, target_row: u16) {
        self.clear_alerts();
        let mut candidate = self.deck.clone();
        let Some(panel) = candidate.panels.get_mut(self.selected_panel_idx) else {
            return;
        };
        let Some(src_idx) = panel.widgets.iter().position(|w| w.id == widget_id) else {
            return;
        };

        let src = panel.widgets[src_idx].clone();
        let clamped_col = target_col.min(panel.columns.saturating_sub(src.col_span));
        if clamped_col == src.col && target_row == src.row {
            self.selected_widget_id = Some(widget_id.to_owned());
            return;
        }

        if is_region_free(
            panel,
            Some(widget_id),
            clamped_col,
            target_row,
            src.col_span,
            src.row_span,
        ) {
            panel.widgets[src_idx].col = clamped_col;
            panel.widgets[src_idx].row = target_row;
            if let Err(msg) = self.commit_deck(candidate, None) {
                self.error_message = Some(msg);
                return;
            }
            self.selected_widget_id = Some(widget_id.to_owned());
            return;
        }

        // Check if there is a single target widget occupying that cell that we can swap with
        let probe = WidgetSpec {
            col: clamped_col,
            row: target_row,
            ..src.clone()
        };
        let colliding: Vec<usize> = panel
            .widgets
            .iter()
            .enumerate()
            .filter(|(idx, w)| *idx != src_idx && probe.overlaps(w))
            .map(|(idx, _)| idx)
            .collect();

        if colliding.len() == 1 {
            let dst_idx = colliding[0];
            let dst = panel.widgets[dst_idx].clone();
            if dst.col_span == src.col_span && dst.row_span == src.row_span {
                panel.widgets[src_idx].col = dst.col;
                panel.widgets[src_idx].row = dst.row;
                panel.widgets[dst_idx].col = src.col;
                panel.widgets[dst_idx].row = src.row;
                if self.commit_deck(candidate, None).is_ok() {
                    self.selected_widget_id = Some(widget_id.to_owned());
                }
            }
        }
    }

    /// Resizes `widget_id` directly to `(col_span, row_span)` when dragging its corner handle.
    pub fn resize_widget_to_spans(&mut self, widget_id: &str, col_span: u16, row_span: u16) {
        self.clear_alerts();
        let mut candidate = self.deck.clone();
        let Some(panel) = candidate.panels.get_mut(self.selected_panel_idx) else {
            return;
        };
        let Some(slot) = panel.widgets.iter_mut().find(|w| w.id == widget_id) else {
            return;
        };
        let max_w = panel.columns.saturating_sub(slot.col).max(1);
        let next_w = col_span.clamp(1, max_w);
        let next_h = row_span.clamp(1, 6);
        if slot.col_span == next_w && slot.row_span == next_h {
            return;
        }
        slot.col_span = next_w;
        slot.row_span = next_h;
        if let Err(msg) = self.commit_deck(candidate, None) {
            self.error_message = Some(msg);
            return;
        }
        self.selected_widget_id = Some(widget_id.to_owned());
    }

    /// Returns `true` if a widget (or new preset) can be dropped or swapped at `(col, row)`.
    #[must_use]
    pub fn can_drop_widget_at(
        &self,
        moving_widget_id: Option<&str>,
        col: u16,
        row: u16,
        col_span: u16,
        row_span: u16,
    ) -> bool {
        let Some(panel) = self.selected_panel() else {
            return false;
        };
        if col_span == 0 || row_span == 0 || col.saturating_add(col_span) > panel.columns {
            return false;
        }
        if is_region_free(panel, moving_widget_id, col, row, col_span, row_span) {
            return true;
        }
        if let Some(moving_id) = moving_widget_id {
            let probe = WidgetSpec {
                id: "__probe__".to_owned(),
                title: "probe".to_owned(),
                kind: WidgetKind::Button,
                col,
                row,
                col_span,
                row_span,
                telemetry: None,
                action: WidgetAction::None,
                text: None,
                icon: None,
                asset_key: None,
                custom_component: None,
                style: WidgetStyle::default(),
            };
            let colliding: Vec<&WidgetSpec> = panel
                .widgets
                .iter()
                .filter(|w| w.id != moving_id && probe.overlaps(w))
                .collect();
            if colliding.len() == 1 {
                let other = colliding[0];
                return other.col_span == col_span && other.row_span == row_span;
            }
        }
        false
    }

    /// Validates and saves edits to the currently selected widget.
    pub fn update_selected_widget(&mut self, updated: WidgetSpec) {
        self.clear_alerts();
        let Some(target_id) = self.selected_widget_id.clone() else {
            return;
        };

        let mut candidate = self.deck.clone();
        let Some(panel) = candidate.panels.get_mut(self.selected_panel_idx) else {
            return;
        };
        let Some(slot) = panel.widgets.iter_mut().find(|w| w.id == target_id) else {
            self.error_message = Some("Выбранный виджет не найден.".to_owned());
            return;
        };

        let next_id = updated.id.clone();
        *slot = updated;

        if let Err(msg) = self.commit_deck(candidate, Some("Свойства виджета сохранены."))
        {
            self.error_message = Some(msg);
            return;
        }
        self.selected_widget_id = Some(next_id);
    }

    /// Moves the selected widget along the grid, rejecting boundary overflow or overlap.
    pub fn move_selected_widget(&mut self, delta_col: i32, delta_row: i32) {
        let Some(mut widget) = self.selected_widget() else {
            return;
        };

        let next_col = i32::from(widget.col).saturating_add(delta_col);
        let next_row = i32::from(widget.row).saturating_add(delta_row);
        if next_col < 0 || next_row < 0 {
            self.error_message =
                Some("Нельзя переместить виджет за левый или верхний край.".to_owned());
            return;
        }

        widget.col = u16::try_from(next_col).unwrap_or(u16::MAX);
        widget.row = u16::try_from(next_row).unwrap_or(u16::MAX);
        self.update_selected_widget(widget);
    }

    /// Adjusts width/height spans of the selected widget (`>= 1`), rejecting overlap.
    pub fn resize_selected_widget(&mut self, delta_col_span: i32, delta_row_span: i32) {
        let Some(mut widget) = self.selected_widget() else {
            return;
        };

        let next_col_span = i32::from(widget.col_span).saturating_add(delta_col_span);
        let next_row_span = i32::from(widget.row_span).saturating_add(delta_row_span);
        if next_col_span < 1 || next_row_span < 1 {
            self.error_message = Some("Размер виджета не может быть меньше 1×1.".to_owned());
            return;
        }

        widget.col_span = u16::try_from(next_col_span).unwrap_or(1);
        widget.row_span = u16::try_from(next_row_span).unwrap_or(1);
        self.update_selected_widget(widget);
    }

    /// Removes the selected widget from the active panel.
    pub fn delete_selected_widget(&mut self) {
        self.clear_alerts();
        let Some(target_id) = self.selected_widget_id.clone() else {
            return;
        };

        let mut candidate = self.deck.clone();
        let Some(panel) = candidate.panels.get_mut(self.selected_panel_idx) else {
            return;
        };
        let before = panel.widgets.len();
        panel.widgets.retain(|widget| widget.id != target_id);
        if panel.widgets.len() == before {
            return;
        }

        if let Err(msg) = self.commit_deck(candidate, Some("Виджет удалён с панели."))
        {
            self.error_message = Some(msg);
            return;
        }
        self.selected_widget_id = self
            .selected_panel()
            .and_then(|panel| panel.widgets.first())
            .map(|widget| widget.id.clone());
    }

    /// Restores the starter deck layout (`DeckConfig::default_starter`).
    pub fn reset_deck_to_starter(&mut self) {
        self.clear_alerts();
        let starter = DeckConfig::default_starter();
        if let Err(msg) = self.commit_deck(
            starter,
            Some("Стартовая конфигурация панелей восстановлена."),
        ) {
            self.error_message = Some(msg);
            return;
        }
        self.selected_panel_idx = 0;
        self.selected_widget_id = self
            .deck
            .panels
            .first()
            .and_then(|panel| panel.widgets.first())
            .map(|widget| widget.id.clone());
    }

    /// Serializes the entire deck into `json_buffer` for export/copy.
    pub fn export_deck_to_buffer(&mut self) {
        self.clear_alerts();
        match self.deck.export_json() {
            Ok(json) => {
                self.json_buffer = json;
                self.status_message =
                    Some("Вся конфигурация панелей экспортирована в JSON.".to_owned());
            }
            Err(err) => {
                self.error_message = Some(format!("Ошибка экспорта JSON: {err}"));
            }
        }
    }

    /// Serializes only the selected panel into `json_buffer`.
    pub fn export_selected_panel_to_buffer(&mut self) {
        self.clear_alerts();
        let Some(panel) = self.selected_panel() else {
            return;
        };
        match panel.export_json() {
            Ok(json) => {
                self.json_buffer = json;
                self.status_message = Some("Выбранная панель экспортирована в JSON.".to_owned());
            }
            Err(err) => {
                self.error_message = Some(format!("Ошибка экспорта панели: {err}"));
            }
        }
    }

    /// Imports and validates a full [`DeckConfig`] from `json_buffer`.
    pub fn import_deck_from_buffer(&mut self) {
        self.clear_alerts();
        match DeckConfig::import_json(&self.json_buffer) {
            Ok(deck) => {
                let next_idx = deck
                    .panels
                    .iter()
                    .position(|panel| panel.id == deck.active_panel_id)
                    .unwrap_or(0);
                if let Err(msg) =
                    self.commit_deck(deck, Some("Конфигурация Deck импортирована и сохранена."))
                {
                    self.error_message = Some(msg);
                    return;
                }
                self.selected_panel_idx = next_idx;
                self.selected_widget_id = self
                    .deck
                    .panels
                    .get(next_idx)
                    .and_then(|panel| panel.widgets.first())
                    .map(|widget| widget.id.clone());
            }
            Err(err) => {
                self.error_message = Some(format!("Некорректный JSON конфигурации Deck: {err}"));
            }
        }
    }

    /// Imports a single [`PanelConfig`] from `json_buffer`, assigning a unique ID if needed.
    pub fn import_panel_from_buffer(&mut self) {
        self.clear_alerts();
        match PanelConfig::import_json(&self.json_buffer) {
            Ok(mut panel) => {
                let mut candidate = self.deck.clone();
                if candidate
                    .panels
                    .iter()
                    .any(|existing| existing.id == panel.id)
                {
                    let existing_ids: Vec<&str> =
                        candidate.panels.iter().map(|p| p.id.as_str()).collect();
                    panel.id = next_unique_safe_id("panel", &panel.id, &existing_ids);
                }
                let imported_id = panel.id.clone();
                candidate.panels.push(panel);
                candidate.active_panel_id = imported_id;
                let next_idx = candidate.panels.len().saturating_sub(1);

                if let Err(msg) = self.commit_deck(
                    candidate,
                    Some("Панель импортирована в текущую конфигурацию."),
                ) {
                    self.error_message = Some(msg);
                    return;
                }
                self.selected_panel_idx = next_idx;
                self.selected_widget_id = self
                    .deck
                    .panels
                    .get(next_idx)
                    .and_then(|p| p.widgets.first())
                    .map(|w| w.id.clone());
            }
            Err(err) => {
                self.error_message = Some(format!("Некорректный JSON панели: {err}"));
            }
        }
    }

    fn commit_deck(&mut self, next: DeckConfig, success_note: Option<&str>) -> Result<(), String> {
        next.validate()
            .map_err(|err| format!("Ошибка макета: {err}"))?;
        self.paths
            .save_panels(&next)
            .map_err(|err| format!("Не удалось записать panels.json: {err}"))?;
        self.deck = next;
        if let Some(note) = success_note {
            self.status_message = Some(note.to_owned());
        }
        Ok(())
    }

    fn clear_alerts(&mut self) {
        self.status_message = None;
        self.error_message = None;
    }
}

/// Ready-to-use consumer widget templates for 1-click and drag-and-drop placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetPreset {
    /// CPU utilization percentage (`1×1`).
    CpuUsage,
    /// Real-time CPU sparkline chart (`2×2`).
    CpuChart,
    /// RAM utilization gauge (`2×1`).
    RamUsage,
    /// GPU utilization percentage (`1×1`).
    GpuUsage,
    /// Network download throughput (`1×1`).
    NetworkSpeed,
    /// Battery charge gauge (`1×1`).
    BatteryLevel,
    /// Media Play / Pause button (`1×1`).
    MediaPlayPause,
    /// Media Next Track button (`1×1`).
    MediaNext,
    /// Media Previous Track button (`1×1`).
    MediaPrev,
    /// System Volume Up button (`1×1`).
    VolumeUp,
    /// System Volume Down button (`1×1`).
    VolumeDown,
    /// System Mute toggle (`1×1`).
    VolumeMute,
    /// Text note (`2×1`).
    QuickNote,
}

impl WidgetPreset {
    /// Returns all ready-made presets in display order.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::CpuUsage,
            Self::CpuChart,
            Self::RamUsage,
            Self::GpuUsage,
            Self::NetworkSpeed,
            Self::BatteryLevel,
            Self::MediaPlayPause,
            Self::MediaPrev,
            Self::MediaNext,
            Self::VolumeUp,
            Self::VolumeDown,
            Self::VolumeMute,
            Self::QuickNote,
        ]
    }

    /// Short Russian title displayed on the template card and created widget.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::CpuUsage => "Процессор %",
            Self::CpuChart => "График CPU",
            Self::RamUsage => "Память RAM",
            Self::GpuUsage => "Видеокарта %",
            Self::NetworkSpeed => "Скорость сети",
            Self::BatteryLevel => "Заряд батареи",
            Self::MediaPlayPause => "Старт / Пауза",
            Self::MediaNext => "След. трек",
            Self::MediaPrev => "Пред. трек",
            Self::VolumeUp => "Громкость +",
            Self::VolumeDown => "Громкость −",
            Self::VolumeMute => "Без звука",
            Self::QuickNote => "Заметка",
        }
    }

    /// Subtitle describing the template's function.
    #[must_use]
    pub const fn subtitle(self) -> &'static str {
        match self {
            Self::CpuUsage => "Датчик нагрузки CPU (1×1)",
            Self::CpuChart => "Живой график нагрузки (2×2)",
            Self::RamUsage => "Шкала занятой памяти (2×1)",
            Self::GpuUsage => "Нагрузка видеокарты (1×1)",
            Self::NetworkSpeed => "Входящий трафик (1×1)",
            Self::BatteryLevel => "Шкала заряда ноутбука (1×1)",
            Self::MediaPlayPause => "Управление плеером (1×1)",
            Self::MediaNext => "Переключить вперёд (1×1)",
            Self::MediaPrev => "Переключить назад (1×1)",
            Self::VolumeUp => "Прибавить звук ПК (1×1)",
            Self::VolumeDown => "Убавить звук ПК (1×1)",
            Self::VolumeMute => "Вкл / Выкл звук ПК (1×1)",
            Self::QuickNote => "Произвольный текст (2×1)",
        }
    }

    /// Default `(col_span, row_span)` for this preset.
    #[must_use]
    pub const fn default_spans(self) -> (u16, u16) {
        match self {
            Self::CpuChart => (2, 2),
            Self::RamUsage | Self::QuickNote => (2, 1),
            _ => (1, 1),
        }
    }

    /// Curated vector icon associated with this preset.
    #[must_use]
    pub const fn icon(self) -> WidgetIcon {
        match self {
            Self::CpuUsage | Self::CpuChart => WidgetIcon::Cpu,
            Self::RamUsage => WidgetIcon::Memory,
            Self::GpuUsage => WidgetIcon::Gpu,
            Self::NetworkSpeed => WidgetIcon::Network,
            Self::BatteryLevel => WidgetIcon::Battery,
            Self::MediaPlayPause => WidgetIcon::PlayPause,
            Self::MediaNext => WidgetIcon::NextTrack,
            Self::MediaPrev => WidgetIcon::PreviousTrack,
            Self::VolumeUp => WidgetIcon::VolumeUp,
            Self::VolumeDown => WidgetIcon::VolumeDown,
            Self::VolumeMute => WidgetIcon::VolumeMute,
            Self::QuickNote => WidgetIcon::Terminal,
        }
    }

    const fn slug(self) -> &'static str {
        match self {
            Self::CpuUsage => "cpu",
            Self::CpuChart => "cpu-chart",
            Self::RamUsage => "ram",
            Self::GpuUsage => "gpu",
            Self::NetworkSpeed => "net",
            Self::BatteryLevel => "battery",
            Self::MediaPlayPause => "play",
            Self::MediaNext => "next",
            Self::MediaPrev => "prev",
            Self::VolumeUp => "vol-up",
            Self::VolumeDown => "vol-down",
            Self::VolumeMute => "mute",
            Self::QuickNote => "note",
        }
    }

    fn build_spec(
        self,
        id: String,
        col: u16,
        row: u16,
        col_span: u16,
        row_span: u16,
    ) -> WidgetSpec {
        let (kind, telemetry, action, text, bg_hex, accent_hex) = match self {
            Self::CpuUsage => (
                WidgetKind::MetricNumber,
                Some(TelemetryBinding::CpuUsagePct),
                WidgetAction::None,
                None,
                "#162238",
                "#38BDF8",
            ),
            Self::CpuChart => (
                WidgetKind::Chart,
                Some(TelemetryBinding::CpuUsagePct),
                WidgetAction::None,
                None,
                "#131E32",
                "#3B82F6",
            ),
            Self::RamUsage => (
                WidgetKind::Gauge,
                Some(TelemetryBinding::RamUsagePct),
                WidgetAction::None,
                None,
                "#112A26",
                "#10B981",
            ),
            Self::GpuUsage => (
                WidgetKind::MetricNumber,
                Some(TelemetryBinding::GpuUsagePct),
                WidgetAction::None,
                None,
                "#251838",
                "#A855F7",
            ),
            Self::NetworkSpeed => (
                WidgetKind::MetricNumber,
                Some(TelemetryBinding::NetworkRxBps),
                WidgetAction::None,
                None,
                "#162436",
                "#06B6D4",
            ),
            Self::BatteryLevel => (
                WidgetKind::Gauge,
                Some(TelemetryBinding::BatteryPct),
                WidgetAction::None,
                None,
                "#14291E",
                "#22C55E",
            ),
            Self::MediaPlayPause => (
                WidgetKind::Button,
                None,
                WidgetAction::Control(ControlAction::MediaPlayPause),
                None,
                "#1D2D50",
                "#60A5FA",
            ),
            Self::MediaNext => (
                WidgetKind::Button,
                None,
                WidgetAction::Control(ControlAction::MediaNextTrack),
                None,
                "#1B2538",
                "#93C5FD",
            ),
            Self::MediaPrev => (
                WidgetKind::Button,
                None,
                WidgetAction::Control(ControlAction::MediaPreviousTrack),
                None,
                "#1B2538",
                "#93C5FD",
            ),
            Self::VolumeUp => (
                WidgetKind::Button,
                None,
                WidgetAction::Control(ControlAction::VolumeUp),
                None,
                "#132A2E",
                "#2DD4BF",
            ),
            Self::VolumeDown => (
                WidgetKind::Button,
                None,
                WidgetAction::Control(ControlAction::VolumeDown),
                None,
                "#132A2E",
                "#2DD4BF",
            ),
            Self::VolumeMute => (
                WidgetKind::Toggle,
                None,
                WidgetAction::Control(ControlAction::VolumeMuteToggle),
                None,
                "#311820",
                "#F87171",
            ),
            Self::QuickNote => (
                WidgetKind::Text,
                None,
                WidgetAction::None,
                Some("Быстрая заметка на экране телефона".to_owned()),
                "#1E2636",
                "#94A3B8",
            ),
        };

        WidgetSpec {
            id,
            title: self.title().to_owned(),
            kind,
            col,
            row,
            col_span,
            row_span,
            telemetry,
            action,
            text,
            icon: Some(self.icon()),
            asset_key: None,
            custom_component: None,
            style: WidgetStyle {
                background_hex: bg_hex.to_owned(),
                foreground_hex: "#F8FAFC".to_owned(),
                accent_hex: accent_hex.to_owned(),
                font_size_sp: 14,
                corner_radius: 14,
            },
        }
    }
}

fn is_region_free(
    panel: &PanelConfig,
    ignore_widget_id: Option<&str>,
    col: u16,
    row: u16,
    col_span: u16,
    row_span: u16,
) -> bool {
    if col_span == 0 || row_span == 0 || col.saturating_add(col_span) > panel.columns {
        return false;
    }
    let probe = WidgetSpec {
        id: "__probe__".to_owned(),
        title: "probe".to_owned(),
        kind: WidgetKind::Text,
        col,
        row,
        col_span,
        row_span,
        telemetry: None,
        action: WidgetAction::None,
        text: None,
        icon: None,
        asset_key: None,
        custom_component: None,
        style: WidgetStyle::default(),
    };
    panel.widgets.iter().all(|existing| {
        if ignore_widget_id.is_some_and(|ignore| existing.id == ignore) {
            true
        } else {
            !probe.overlaps(existing)
        }
    })
}

/// Detects the primary local LAN IPv4 address via routing table lookup without sending traffic.
#[must_use]
pub fn detect_lan_ipv4() -> Option<String> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("192.0.2.1:80").ok()?;
    let addr = sock.local_addr().ok()?;
    let ip = addr.ip();
    if ip.is_loopback() || ip.is_unspecified() {
        None
    } else {
        Some(ip.to_string())
    }
}

/// Scans `panel` rows top-to-bottom, left-to-right for the earliest `(col, row)` placement
/// with size `(col_span, row_span)` that neither overflows `panel.columns` nor overlaps widgets.
#[must_use]
pub fn find_free_cell(panel: &PanelConfig, col_span: u16, row_span: u16) -> Option<(u16, u16)> {
    if col_span == 0 || row_span == 0 || col_span > panel.columns {
        return None;
    }

    let max_start_col = panel.columns.saturating_sub(col_span);
    for row in 0..GRID_SEARCH_MAX_ROWS {
        for col in 0..=max_start_col {
            let probe = WidgetSpec {
                id: "__probe__".to_owned(),
                title: "probe".to_owned(),
                kind: WidgetKind::Text,
                col,
                row,
                col_span,
                row_span,
                telemetry: None,
                action: WidgetAction::None,
                text: None,
                icon: None,
                asset_key: None,
                custom_component: None,
                style: WidgetStyle::default(),
            };
            if panel
                .widgets
                .iter()
                .all(|existing| !probe.overlaps(existing))
            {
                return Some((col, row));
            }
        }
    }
    None
}

/// Resolves a sibling `nexdeck-agent` binary path in `dir` (`nexdeck-agent.exe` on Windows).
#[must_use]
pub fn resolve_agent_exe_in_dir(dir: &Path) -> Option<PathBuf> {
    let binary_name = if cfg!(windows) {
        "nexdeck-agent.exe"
    } else {
        "nexdeck-agent"
    };
    let candidate = dir.join(binary_name);
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

/// Returns human-readable Russian labels for widget kinds.
#[must_use]
pub const fn widget_kind_label(kind: WidgetKind) -> &'static str {
    match kind {
        WidgetKind::Text => "Текст",
        WidgetKind::MetricNumber => "Метрика (число)",
        WidgetKind::Button => "Кнопка",
        WidgetKind::Toggle => "Переключатель",
        WidgetKind::Chart => "График",
        WidgetKind::Gauge => "Индикатор (%)",
        WidgetKind::Icon => "Иконка",
        WidgetKind::Image => "Изображение",
        WidgetKind::Custom => "Плагин (Custom)",
    }
}

/// Returns all selectable widget kinds in display order.
#[must_use]
pub const fn all_widget_kinds() -> &'static [WidgetKind] {
    &[
        WidgetKind::Chart,
        WidgetKind::MetricNumber,
        WidgetKind::Gauge,
        WidgetKind::Button,
        WidgetKind::Toggle,
        WidgetKind::Text,
        WidgetKind::Icon,
        WidgetKind::Image,
        WidgetKind::Custom,
    ]
}

/// Returns human-readable labels for telemetry bindings.
#[must_use]
pub const fn telemetry_label(binding: TelemetryBinding) -> &'static str {
    match binding {
        TelemetryBinding::CpuUsagePct => "CPU нагрузка (%)",
        TelemetryBinding::CpuFrequencyMhz => "CPU частота (МГц)",
        TelemetryBinding::CpuTemperatureC => "CPU температура (°C)",
        TelemetryBinding::RamUsagePct => "RAM занято (%)",
        TelemetryBinding::RamUsedBytes => "RAM объём (байты)",
        TelemetryBinding::GpuUsagePct => "GPU нагрузка (%)",
        TelemetryBinding::GpuVramPct => "GPU VRAM (%)",
        TelemetryBinding::NetworkRxBps => "Сеть входящая (B/s)",
        TelemetryBinding::NetworkTxBps => "Сеть исходящая (B/s)",
        TelemetryBinding::BatteryPct => "Батарея (%)",
        TelemetryBinding::UptimeSecs => "Время работы (сек)",
    }
}

/// Returns all telemetry metrics in display order.
#[must_use]
pub const fn all_telemetry_bindings() -> &'static [TelemetryBinding] {
    &[
        TelemetryBinding::CpuUsagePct,
        TelemetryBinding::CpuFrequencyMhz,
        TelemetryBinding::CpuTemperatureC,
        TelemetryBinding::RamUsagePct,
        TelemetryBinding::RamUsedBytes,
        TelemetryBinding::GpuUsagePct,
        TelemetryBinding::GpuVramPct,
        TelemetryBinding::NetworkRxBps,
        TelemetryBinding::NetworkTxBps,
        TelemetryBinding::BatteryPct,
        TelemetryBinding::UptimeSecs,
    ]
}

/// Returns human-readable labels for widget actions.
#[must_use]
pub const fn action_label(action: WidgetAction) -> &'static str {
    match action {
        WidgetAction::None => "Нет действия",
        WidgetAction::Control(ControlAction::MediaPlayPause) => "Медиа: Пауза / Старт",
        WidgetAction::Control(ControlAction::MediaPreviousTrack) => "Медиа: Предыдущий трек",
        WidgetAction::Control(ControlAction::MediaNextTrack) => "Медиа: Следующий трек",
        WidgetAction::Control(ControlAction::VolumeUp) => "Громкость +",
        WidgetAction::Control(ControlAction::VolumeDown) => "Громкость -",
        WidgetAction::Control(ControlAction::VolumeMuteToggle) => "Звук: Выкл / Вкл",
        WidgetAction::Heartbeat => "Пинг (Heartbeat)",
    }
}

/// Returns all allowlisted widget actions in display order.
#[must_use]
pub const fn all_widget_actions() -> &'static [WidgetAction] {
    &[
        WidgetAction::None,
        WidgetAction::Control(ControlAction::MediaPlayPause),
        WidgetAction::Control(ControlAction::MediaPreviousTrack),
        WidgetAction::Control(ControlAction::MediaNextTrack),
        WidgetAction::Control(ControlAction::VolumeUp),
        WidgetAction::Control(ControlAction::VolumeDown),
        WidgetAction::Control(ControlAction::VolumeMuteToggle),
        WidgetAction::Heartbeat,
    ]
}

/// Returns human-readable labels for built-in curated icons.
#[must_use]
pub const fn icon_label(icon: WidgetIcon) -> &'static str {
    match icon {
        WidgetIcon::Cpu => "CPU",
        WidgetIcon::Memory => "Память",
        WidgetIcon::Gpu => "GPU",
        WidgetIcon::Disk => "Диск",
        WidgetIcon::Network => "Сеть",
        WidgetIcon::Battery => "Батарея",
        WidgetIcon::PlayPause => "Play/Pause",
        WidgetIcon::PreviousTrack => "Назад",
        WidgetIcon::NextTrack => "Вперёд",
        WidgetIcon::VolumeUp => "Громче",
        WidgetIcon::VolumeDown => "Тише",
        WidgetIcon::VolumeMute => "Mute",
        WidgetIcon::Terminal => "Терминал",
        WidgetIcon::Shield => "Щит",
    }
}

/// Returns all built-in icon variants.
#[must_use]
pub const fn all_widget_icons() -> &'static [WidgetIcon] {
    &[
        WidgetIcon::Cpu,
        WidgetIcon::Memory,
        WidgetIcon::Gpu,
        WidgetIcon::Disk,
        WidgetIcon::Network,
        WidgetIcon::Battery,
        WidgetIcon::PlayPause,
        WidgetIcon::PreviousTrack,
        WidgetIcon::NextTrack,
        WidgetIcon::VolumeUp,
        WidgetIcon::VolumeDown,
        WidgetIcon::VolumeMute,
        WidgetIcon::Terminal,
        WidgetIcon::Shield,
    ]
}

fn read_autostart_status() -> (Option<String>, Option<String>) {
    match query_autostart() {
        Ok(cmd) => (cmd, None),
        Err(AutostartError::UnsupportedPlatform) => (
            None,
            Some("Управление автозапуском HKCU\\...\\Run доступно только на Windows.".to_owned()),
        ),
        Err(err) => (None, Some(format!("Статус автозапуска недоступен: {err}"))),
    }
}

fn resolve_agent_binary_path() -> Result<PathBuf, String> {
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
        && let Some(found) = resolve_agent_exe_in_dir(parent)
    {
        return Ok(found);
    }
    Err(
        "Не найден бинарник nexdeck-agent рядом с nexdeck-desktop (соберите весь workspace)."
            .to_owned(),
    )
}

fn resolve_agent_binary_command_target() -> PathBuf {
    resolve_agent_binary_path().unwrap_or_else(|_| {
        PathBuf::from(if cfg!(windows) {
            "nexdeck-agent.exe"
        } else {
            "nexdeck-agent"
        })
    })
}

fn extract_pin_from_banner(line: &str) -> Option<String> {
    let marker = "Pairing enabled (PIN: ";
    let idx = line.find(marker)?;
    let rest = &line[idx + marker.len()..];
    let pin: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if pin.len() == 6 { Some(pin) } else { None }
}

fn next_unique_safe_id(fallback_prefix: &str, raw_hint: &str, existing: &[&str]) -> String {
    let mut base = String::with_capacity(32);
    for ch in raw_hint.chars() {
        if ch.is_ascii_alphanumeric() {
            base.push(ch.to_ascii_lowercase());
        } else if (ch == '-' || ch == '_' || ch.is_whitespace())
            && !base.is_empty()
            && !base.ends_with('-')
        {
            base.push('-');
        }
    }
    while base.ends_with('-') {
        base.pop();
    }
    if base.is_empty() {
        base = fallback_prefix.to_owned();
    }
    if base.len() > 48 {
        base.truncate(48);
    }

    if !existing.contains(&base.as_str()) {
        return base;
    }
    for n in 2..=9999 {
        let candidate = format!("{base}-{n}");
        if candidate.len() <= 64 && !existing.contains(&candidate.as_str()) {
            return candidate;
        }
    }
    format!("{fallback_prefix}-{}", existing.len() + 1)
}

#[allow(dead_code)]
const fn widget_kind_slug(kind: WidgetKind) -> &'static str {
    match kind {
        WidgetKind::Text => "text",
        WidgetKind::MetricNumber => "metric",
        WidgetKind::Button => "button",
        WidgetKind::Toggle => "toggle",
        WidgetKind::Chart => "chart",
        WidgetKind::Gauge => "gauge",
        WidgetKind::Icon => "icon",
        WidgetKind::Image => "image",
        WidgetKind::Custom => "custom",
    }
}

#[allow(dead_code)]
fn default_spans_for_kind(kind: WidgetKind, panel_columns: u16) -> (u16, u16) {
    let (col_span, row_span) = match kind {
        WidgetKind::Chart | WidgetKind::Text => (2, 2),
        WidgetKind::MetricNumber | WidgetKind::Gauge => (2, 1),
        WidgetKind::Button | WidgetKind::Toggle | WidgetKind::Icon => (1, 1),
        WidgetKind::Image | WidgetKind::Custom => (2, 1),
    };
    (col_span.min(panel_columns.max(1)), row_span)
}

#[allow(dead_code)]
fn default_widget_spec(
    id: String,
    kind: WidgetKind,
    col: u16,
    row: u16,
    col_span: u16,
    row_span: u16,
) -> WidgetSpec {
    match kind {
        WidgetKind::Chart => WidgetSpec {
            id,
            title: "CPU График".to_owned(),
            kind,
            col,
            row,
            col_span,
            row_span,
            telemetry: Some(TelemetryBinding::CpuUsagePct),
            action: WidgetAction::None,
            text: None,
            icon: Some(WidgetIcon::Cpu),
            asset_key: None,
            custom_component: None,
            style: WidgetStyle::default(),
        },
        WidgetKind::MetricNumber => WidgetSpec {
            id,
            title: "RAM %".to_owned(),
            kind,
            col,
            row,
            col_span,
            row_span,
            telemetry: Some(TelemetryBinding::RamUsagePct),
            action: WidgetAction::None,
            text: None,
            icon: Some(WidgetIcon::Memory),
            asset_key: None,
            custom_component: None,
            style: WidgetStyle::default(),
        },
        WidgetKind::Gauge => WidgetSpec {
            id,
            title: "Индикатор GPU".to_owned(),
            kind,
            col,
            row,
            col_span,
            row_span,
            telemetry: Some(TelemetryBinding::GpuUsagePct),
            action: WidgetAction::None,
            text: None,
            icon: Some(WidgetIcon::Gpu),
            asset_key: None,
            custom_component: None,
            style: WidgetStyle::default(),
        },
        WidgetKind::Button => WidgetSpec {
            id,
            title: "Пауза / Старт".to_owned(),
            kind,
            col,
            row,
            col_span,
            row_span,
            telemetry: None,
            action: WidgetAction::Control(ControlAction::MediaPlayPause),
            text: None,
            icon: Some(WidgetIcon::PlayPause),
            asset_key: None,
            custom_component: None,
            style: WidgetStyle::default(),
        },
        WidgetKind::Toggle => WidgetSpec {
            id,
            title: "Mute звука".to_owned(),
            kind,
            col,
            row,
            col_span,
            row_span,
            telemetry: None,
            action: WidgetAction::Control(ControlAction::VolumeMuteToggle),
            text: None,
            icon: Some(WidgetIcon::VolumeMute),
            asset_key: None,
            custom_component: None,
            style: WidgetStyle::default(),
        },
        WidgetKind::Text => WidgetSpec {
            id,
            title: "ExampleNote".to_owned(),
            kind,
            col,
            row,
            col_span,
            row_span,
            telemetry: None,
            action: WidgetAction::None,
            text: Some("Пример текстового виджета NexDeck".to_owned()),
            icon: None,
            asset_key: None,
            custom_component: None,
            style: WidgetStyle::default(),
        },
        WidgetKind::Icon => WidgetSpec {
            id,
            title: "Статус узла".to_owned(),
            kind,
            col,
            row,
            col_span,
            row_span,
            telemetry: None,
            action: WidgetAction::Heartbeat,
            text: None,
            icon: Some(WidgetIcon::Shield),
            asset_key: None,
            custom_component: None,
            style: WidgetStyle::default(),
        },
        WidgetKind::Image => WidgetSpec {
            id,
            title: "ExampleBanner".to_owned(),
            kind,
            col,
            row,
            col_span,
            row_span,
            telemetry: None,
            action: WidgetAction::None,
            text: None,
            icon: None,
            asset_key: Some("example-banner".to_owned()),
            custom_component: None,
            style: WidgetStyle::default(),
        },
        WidgetKind::Custom => WidgetSpec {
            id,
            title: "ExamplePlugin".to_owned(),
            kind,
            col,
            row,
            col_span,
            row_span,
            telemetry: None,
            action: WidgetAction::None,
            text: None,
            icon: Some(WidgetIcon::Terminal),
            asset_key: None,
            custom_component: Some("org.example.widget".to_owned()),
            style: WidgetStyle::default(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use nexdeck_transport::pairing::DeviceRecord;

    use super::*;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "nexdeck_desktop_test_{label}_{}_{nanos}",
            std::process::id()
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn settings_and_device_revocation_persist_to_storage() {
        let dir = unique_temp_dir("devices");
        let paths = StoragePaths::new(dir.clone());
        paths
            .save_devices(&[
                DeviceRecord {
                    device_id: "phone-1".to_owned(),
                    device_name: "ExamplePhoneA".to_owned(),
                    token_hash_sha256: "a".repeat(64),
                    paired_at_epoch_ms: 1_700_000_000_000,
                },
                DeviceRecord {
                    device_id: "phone-2".to_owned(),
                    device_name: "ExamplePhoneB".to_owned(),
                    token_hash_sha256: "b".repeat(64),
                    paired_at_epoch_ms: 1_700_000_001_000,
                },
            ])
            .expect("seed devices");

        let mut state = DesktopManagerState::load(paths.clone()).expect("load desktop state");
        assert_eq!(state.devices.len(), 2);
        assert_eq!(state.tls_fingerprint_sha256.len(), 64);

        state.settings_draft.poll_interval_ms = "500".to_owned();
        state.settings_draft.san_hosts_csv = "localhost, 127.0.0.1, example.invalid".to_owned();
        state.save_settings_draft();
        assert!(state.error_message.is_none());
        assert_eq!(
            paths
                .load_or_init_settings()
                .expect("reload settings")
                .agent
                .poll_interval_ms,
            500
        );

        state.revoke_device("phone-1");
        assert!(state.error_message.is_none());
        assert_eq!(state.devices.len(), 1);
        assert_eq!(state.devices[0].device_id, "phone-2");
        assert_eq!(paths.load_devices().expect("reload devices").len(), 1);

        state.new_api_client_id = "org.example.script".to_owned();
        state.new_api_client_name = "ExampleScript".to_owned();
        state.register_api_client();
        assert!(state.error_message.is_none());
        assert_eq!(state.api_clients.len(), 1);
        assert!(state.latest_issued_api_client.is_some());

        state.set_api_client_revoked("org.example.script", true);
        assert!(state.api_clients[0].revoked);

        state.delete_api_client("org.example.script");
        assert!(state.api_clients.is_empty());
        assert!(paths.load_clients().expect("reload clients").is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn panel_and_widget_builder_prevents_overlaps_and_exports_json() {
        let dir = unique_temp_dir("panels");
        let paths = StoragePaths::new(dir.clone());
        let mut state = DesktopManagerState::load(paths.clone()).expect("load desktop state");

        state.new_panel_name = "Игровая панель".to_owned();
        state.new_panel_columns = 3;
        state.add_panel();
        assert!(state.error_message.is_none());
        assert_eq!(state.selected_panel().expect("active panel").columns, 3);

        state.add_widget(WidgetKind::Button);
        assert!(state.error_message.is_none());
        let first_id = state.selected_widget_id.clone().expect("first widget");

        state.add_widget(WidgetKind::MetricNumber);
        assert!(state.error_message.is_none());
        let second = state.selected_widget().expect("second widget");
        assert_eq!((second.col, second.row), (1, 0));

        // Attempt moving second widget onto first widget at (0, 0) -> must reject cleanly.
        state.move_selected_widget(-1, 0);
        assert!(state.error_message.is_some());
        assert_eq!(state.selected_widget().expect("unchanged").col, 1);

        // Moving down to (1, 1) succeeds without overlap.
        state.move_selected_widget(0, 1);
        assert!(state.error_message.is_none());
        assert_eq!(state.selected_widget().expect("moved").row, 1);

        // Export deck, reset starter, and re-import JSON cleanly.
        state.export_deck_to_buffer();
        let exported = state.json_buffer.clone();
        state.reset_deck_to_starter();
        assert_eq!(
            state.deck.panels.len(),
            DeckConfig::default_starter().panels.len()
        );

        state.json_buffer = exported;
        state.import_deck_from_buffer();
        assert!(state.error_message.is_none());
        assert!(state.deck.panels.iter().any(|p| p.name == "Игровая панель"));

        // Verify delete first widget persists.
        state.select_widget(&first_id);
        state.delete_selected_widget();
        assert!(state.error_message.is_none());

        let reloaded = paths.load_or_init_panels().expect("reload panels.json");
        assert_eq!(reloaded, state.deck);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extracts_pairing_pin_and_json_from_agent_stdout() {
        let mut output = AgentOutputState::default();
        output.push_line(
            "",
            "NexDeck Core Agent v0.1.0 listening on wss://127.0.0.1:9443/ws",
        );
        output.push_line("", "Pairing enabled (PIN: 482913, TTL: 300s):");
        output.push_line("", "{");
        output.push_line("", r#"  "protocol_version": 1,"#);
        output.push_line("", r#"  "host": "127.0.0.1","#);
        output.push_line("", r#"  "port": 9443,"#);
        output.push_line(
            "",
            &format!(r#"  "fingerprint_sha256": "{}","#, "a".repeat(64)),
        );
        output.push_line("", r#"  "pin": "482913""#);
        output.push_line("", "}");

        assert_eq!(output.latest_pin.as_deref(), Some("482913"));
        assert!(
            output
                .latest_pairing_json
                .as_deref()
                .is_some_and(|raw| raw.contains("\"pin\": \"482913\""))
        );
    }

    #[test]
    fn resolve_agent_exe_finds_sibling_binary() {
        let dir = unique_temp_dir("exe_lookup");
        let name = if cfg!(windows) {
            "nexdeck-agent.exe"
        } else {
            "nexdeck-agent"
        };
        let exe = dir.join(name);
        fs::write(&exe, b"placeholder").expect("write placeholder file");
        assert_eq!(resolve_agent_exe_in_dir(&dir), Some(exe));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn persists_notification_filter_alert_thresholds_and_clears_buffer() {
        use nexdeck_notifications::NotificationSource;

        let dir = unique_temp_dir("notif_policy");
        let paths = StoragePaths::new(dir.clone());
        let mut state = DesktopManagerState::load(paths.clone()).expect("load desktop state");

        state.settings_draft.min_notification_priority = NotificationPriority::High;
        state.settings_draft.muted_origins_csv = "ExampleApp, NoisyPlugin".to_owned();
        state.settings_draft.alerts_enabled = true;
        state.settings_draft.alert_cpu_pct = "88".to_owned();
        state.settings_draft.alert_cpu_temp_c = "82".to_owned();
        state.settings_draft.alert_cooldown_secs = "45".to_owned();
        state.save_settings_draft();
        assert!(state.error_message.is_none());

        let events = vec![
            NotificationEvent {
                id: 1,
                source: NotificationSource::HardwareAlert,
                title: "CPU Temp High".to_owned(),
                body: "Reached 85C".to_owned(),
                priority: NotificationPriority::High,
                created_at_epoch_ms: 1_700_000_000_001,
            },
            NotificationEvent {
                id: 2,
                source: NotificationSource::Application("ExampleBackup".to_owned()),
                title: "Backup Finished".to_owned(),
                body: "Completed cleanly".to_owned(),
                priority: NotificationPriority::Normal,
                created_at_epoch_ms: 1_700_000_000_002,
            },
        ];
        paths
            .save_notifications(&events)
            .expect("persist synthetic sleep buffer");

        let mut reloaded =
            DesktopManagerState::load(paths.clone()).expect("reload updated desktop state");
        assert_eq!(
            reloaded.settings.agent.notification_filter.min_priority,
            NotificationPriority::High
        );
        assert_eq!(
            reloaded.settings.agent.notification_filter.muted_origins,
            vec!["ExampleApp".to_owned(), "NoisyPlugin".to_owned()]
        );
        assert_eq!(
            reloaded.settings.agent.alert_thresholds.cpu_usage_pct,
            Some(88)
        );
        assert_eq!(
            reloaded.settings.agent.alert_thresholds.cpu_temp_c,
            Some(82)
        );
        assert_eq!(reloaded.settings.agent.alert_thresholds.cooldown_secs, 45);
        assert_eq!(reloaded.buffered_notifications.len(), 2);
        assert_eq!(reloaded.buffered_notifications[0].id, 2);

        reloaded.clear_notifications_buffer();
        assert!(reloaded.error_message.is_none());
        assert!(reloaded.buffered_notifications.is_empty());
        assert!(
            paths
                .load_notifications()
                .expect("reload empty buffer")
                .is_empty()
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
