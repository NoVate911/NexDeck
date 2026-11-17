//! Persistent local configuration, device registry records, and TLS identity storage.

use std::fmt;
use std::fs;
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use nexdeck_api::clients::ClientRecord;
use nexdeck_api::panels::DeckConfig;
use nexdeck_transport::TransportError;
use nexdeck_transport::pairing::DeviceRecord;
use nexdeck_transport::tls::HostTlsIdentity;
use serde::{Deserialize, Serialize};

use crate::{AgentConfig, CoreAgent};

const DATA_DIR_NAME: &str = "NexDeck";
const CONFIG_FILE_NAME: &str = "config.json";
const DEVICES_FILE_NAME: &str = "devices.json";
const CLIENTS_FILE_NAME: &str = "clients.json";
const PANELS_FILE_NAME: &str = "panels.json";
const TLS_CERT_FILE_NAME: &str = "host-cert.pem";
const TLS_KEY_FILE_NAME: &str = "host-key.pem";
const TLS_FINGERPRINT_FILE_NAME: &str = "host-fingerprint.sha256";

/// Default loopback WSS bind address used when no override is configured.
pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1:9443";

/// Persistent user configuration loaded from `config.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentSettings {
    /// Socket address (`ip:port`) bound by the WSS server.
    pub bind_addr: String,
    /// Subject Alternative Name hosts embedded in generated self-signed certificates.
    pub san_hosts: Vec<String>,
    /// Core agent runtime and queue settings.
    pub agent: AgentConfig,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            bind_addr: DEFAULT_BIND_ADDR.to_owned(),
            san_hosts: vec!["localhost".to_owned(), "127.0.0.1".to_owned()],
            agent: AgentConfig::default(),
        }
    }
}

impl AgentSettings {
    /// Validates all settings invariants and returns the parsed bind [`SocketAddr`].
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidSettings`] if `bind_addr`, `san_hosts`, or
    /// `agent` fails validation.
    pub fn validate(&self) -> Result<SocketAddr, StorageError> {
        let addr: SocketAddr = self.bind_addr.parse().map_err(|err| {
            StorageError::InvalidSettings(format!("invalid bind_addr '{}': {err}", self.bind_addr))
        })?;
        if self.san_hosts.iter().any(|host| host.trim().is_empty()) {
            return Err(StorageError::InvalidSettings(
                "san_hosts entries must be non-empty".to_owned(),
            ));
        }
        CoreAgent::new(self.agent.clone())
            .map_err(|err| StorageError::InvalidSettings(err.to_string()))?;
        Ok(addr)
    }
}

/// Resolved file-system layout for persistent `nexdeck-agent` state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoragePaths {
    root_dir: PathBuf,
}

impl StoragePaths {
    /// Constructs [`StoragePaths`] rooted at `root_dir`.
    #[must_use]
    pub const fn new(root_dir: PathBuf) -> Self {
        Self { root_dir }
    }

    /// Resolves the agent data directory using `override_dir` or platform defaults
    /// (`%LOCALAPPDATA%\NexDeck` on Windows, `$XDG_DATA_HOME/NexDeck` or `~/.local/share/NexDeck` on Unix).
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::MissingDataDir`] if no user data directory environment variable is set.
    pub fn resolve(override_dir: Option<&Path>) -> Result<Self, StorageError> {
        if let Some(dir) = override_dir {
            return Ok(Self::new(dir.to_path_buf()));
        }
        let base = default_data_base_dir().ok_or(StorageError::MissingDataDir)?;
        Ok(Self::new(base.join(DATA_DIR_NAME)))
    }

    /// Returns the root storage directory path.
    #[must_use]
    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    /// Returns the path to `config.json`.
    #[must_use]
    pub fn config_path(&self) -> PathBuf {
        self.root_dir.join(CONFIG_FILE_NAME)
    }

    /// Returns the path to `devices.json`.
    #[must_use]
    pub fn devices_path(&self) -> PathBuf {
        self.root_dir.join(DEVICES_FILE_NAME)
    }

    /// Returns the path to `clients.json`.
    #[must_use]
    pub fn clients_path(&self) -> PathBuf {
        self.root_dir.join(CLIENTS_FILE_NAME)
    }

    /// Returns the path to `panels.json`.
    #[must_use]
    pub fn panels_path(&self) -> PathBuf {
        self.root_dir.join(PANELS_FILE_NAME)
    }

    /// Returns the path to the host TLS certificate PEM file.
    #[must_use]
    pub fn tls_cert_path(&self) -> PathBuf {
        self.root_dir.join(TLS_CERT_FILE_NAME)
    }

    /// Returns the path to the host TLS private key PEM file.
    #[must_use]
    pub fn tls_key_path(&self) -> PathBuf {
        self.root_dir.join(TLS_KEY_FILE_NAME)
    }

    /// Returns the path to the SHA-256 certificate fingerprint file.
    #[must_use]
    pub fn tls_fingerprint_path(&self) -> PathBuf {
        self.root_dir.join(TLS_FINGERPRINT_FILE_NAME)
    }

    /// Loads `config.json` or initializes it with validated defaults if absent.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if reading, parsing, validating, or writing defaults fails.
    pub fn load_or_init_settings(&self) -> Result<AgentSettings, StorageError> {
        let path = self.config_path();
        if path.exists() {
            let raw = fs::read_to_string(&path)?;
            let settings: AgentSettings = serde_json::from_str(&raw)?;
            let _ = settings.validate()?;
            return Ok(settings);
        }

        let defaults = AgentSettings::default();
        self.save_settings(&defaults)?;
        Ok(defaults)
    }

    /// Validates and atomically writes `settings` to `config.json`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if `settings` is invalid or disk persistence fails.
    pub fn save_settings(&self, settings: &AgentSettings) -> Result<(), StorageError> {
        let _ = settings.validate()?;
        let json = serde_json::to_vec_pretty(settings)?;
        write_atomic(&self.config_path(), &json, false)
    }

    /// Loads persisted paired [`DeviceRecord`] entries from `devices.json` (empty if absent).
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if reading or JSON decoding fails.
    pub fn load_devices(&self) -> Result<Vec<DeviceRecord>, StorageError> {
        let path = self.devices_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let raw = fs::read_to_string(&path)?;
        let mut records: Vec<DeviceRecord> = serde_json::from_str(&raw)?;
        records.sort_by(|left, right| left.device_id.cmp(&right.device_id));
        Ok(records)
    }

    /// Atomically persists paired [`DeviceRecord`] entries (token hashes only) to `devices.json`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if serialization or atomic file replacement fails.
    pub fn save_devices(&self, devices: &[DeviceRecord]) -> Result<(), StorageError> {
        let mut sorted = devices.to_vec();
        sorted.sort_by(|left, right| left.device_id.cmp(&right.device_id));
        let json = serde_json::to_vec_pretty(&sorted)?;
        write_atomic(&self.devices_path(), &json, true)
    }

    /// Loads persisted Universal API [`ClientRecord`] entries from `clients.json` (empty if absent).
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if reading or JSON decoding fails.
    pub fn load_clients(&self) -> Result<Vec<ClientRecord>, StorageError> {
        let path = self.clients_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let raw = fs::read_to_string(&path)?;
        let mut records: Vec<ClientRecord> = serde_json::from_str(&raw)?;
        records.sort_by(|left, right| left.client_id.cmp(&right.client_id));
        Ok(records)
    }

    /// Atomically persists Universal API [`ClientRecord`] entries (token hashes only) to `clients.json`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if serialization or atomic file replacement fails.
    pub fn save_clients(&self, clients: &[ClientRecord]) -> Result<(), StorageError> {
        let mut sorted = clients.to_vec();
        sorted.sort_by(|left, right| left.client_id.cmp(&right.client_id));
        let json = serde_json::to_vec_pretty(&sorted)?;
        write_atomic(&self.clients_path(), &json, true)
    }

    /// Loads `panels.json` or initializes it with [`DeckConfig::default_starter`] if absent.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if reading, parsing, grid validation, or disk persistence fails.
    pub fn load_or_init_panels(&self) -> Result<DeckConfig, StorageError> {
        let path = self.panels_path();
        if path.exists() {
            let raw = fs::read_to_string(&path)?;
            return DeckConfig::import_json(&raw)
                .map_err(|err| StorageError::InvalidSettings(err.to_string()));
        }

        let starter = DeckConfig::default_starter();
        self.save_panels(&starter)?;
        Ok(starter)
    }

    /// Validates and atomically writes `deck` to `panels.json`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if `deck` fails layout validation or disk writes fail.
    pub fn save_panels(&self, deck: &DeckConfig) -> Result<(), StorageError> {
        let json = deck
            .export_json()
            .map_err(|err| StorageError::InvalidSettings(err.to_string()))?;
        write_atomic(&self.panels_path(), json.as_bytes(), false)
    }

    /// Loads an existing host TLS certificate/key pair from disk or generates and persists one.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if certificate generation or disk I/O fails.
    pub fn load_or_create_tls_identity(
        &self,
        san_hosts: &[String],
    ) -> Result<HostTlsIdentity, StorageError> {
        let cert_path = self.tls_cert_path();
        let key_path = self.tls_key_path();
        let fp_path = self.tls_fingerprint_path();

        if cert_path.exists() && key_path.exists() && fp_path.exists() {
            let cert_pem = fs::read_to_string(&cert_path)?;
            let key_pem = fs::read_to_string(&key_path)?;
            let fingerprint_sha256 = fs::read_to_string(&fp_path)?.trim().to_ascii_lowercase();
            if is_valid_pem_material(&cert_pem, &key_pem, &fingerprint_sha256) {
                return Ok(HostTlsIdentity {
                    cert_pem,
                    key_pem,
                    fingerprint_sha256,
                });
            }
        }

        let identity = HostTlsIdentity::generate_self_signed(san_hosts)?;
        write_atomic(&cert_path, identity.cert_pem.as_bytes(), false)?;
        write_atomic(&key_path, identity.key_pem.as_bytes(), true)?;
        write_atomic(&fp_path, identity.fingerprint_sha256.as_bytes(), false)?;
        Ok(identity)
    }
}

/// Errors produced by persistent storage operations.
#[derive(Debug)]
pub enum StorageError {
    /// No standard user data directory could be resolved from the environment.
    MissingDataDir,
    /// Underlying filesystem error.
    Io(std::io::Error),
    /// JSON serialization or deserialization error.
    Json(serde_json::Error),
    /// Loaded or saved configuration violated agent invariants.
    InvalidSettings(String),
    /// TLS identity generation failed.
    Transport(TransportError),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDataDir => write!(f, "could not locate user application data directory"),
            Self::Io(err) => write!(f, "storage I/O error: {err}"),
            Self::Json(err) => write!(f, "storage JSON error: {err}"),
            Self::InvalidSettings(msg) => write!(f, "invalid persistent settings: {msg}"),
            Self::Transport(err) => write!(f, "TLS identity storage error: {err}"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<std::io::Error> for StorageError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for StorageError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<TransportError> for StorageError {
    fn from(value: TransportError) -> Self {
        Self::Transport(value)
    }
}

#[cfg(windows)]
fn default_data_base_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
}

#[cfg(not(windows))]
fn default_data_base_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
}

fn is_valid_pem_material(cert_pem: &str, key_pem: &str, fingerprint: &str) -> bool {
    let cert_ok = cert_pem.trim_start().starts_with("-----");
    let key_ok = key_pem.trim_start().starts_with("-----");
    let fp_ok = fingerprint.len() == 64 && fingerprint.bytes().all(|b| b.is_ascii_hexdigit());
    cert_ok && key_ok && fp_ok
}

fn write_atomic(path: &Path, bytes: &[u8], sensitive: bool) -> Result<(), StorageError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("nexdeck");
    let tmp_name = format!("{file_name}.tmp.{}.{nonce}", std::process::id());
    let tmp_path = path.with_file_name(tmp_name);

    let mut file = open_temp_file(&tmp_path, sensitive)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);

    fs::rename(&tmp_path, path)?;
    Ok(())
}

fn open_temp_file(path: &Path, sensitive: bool) -> Result<fs::File, std::io::Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);
        if sensitive {
            opts.mode(0o600);
        }
        opts.open(path)
    }
    #[cfg(not(unix))]
    {
        let _ = sensitive;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_storage_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        std::env::temp_dir().join(format!(
            "nexdeck-core-{label}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn initializes_and_reloads_settings_and_devices() {
        let dir = temp_storage_dir("settings");
        let paths = StoragePaths::new(dir.clone());

        let initial = paths
            .load_or_init_settings()
            .expect("default settings should persist");
        assert_eq!(initial, AgentSettings::default());
        assert!(paths.config_path().exists());

        let mut modified = initial;
        modified.bind_addr = "127.0.0.1:9444".to_owned();
        modified.agent.poll_interval_ms = 500;
        paths
            .save_settings(&modified)
            .expect("modified settings should save");

        let reloaded = paths.load_or_init_settings().expect("should reload");
        assert_eq!(reloaded, modified);

        let devices = vec![
            DeviceRecord {
                device_id: "example-device-b".to_owned(),
                device_name: "ExamplePhoneB".to_owned(),
                token_hash_sha256: "b".repeat(64),
                paired_at_epoch_ms: 2_000,
            },
            DeviceRecord {
                device_id: "example-device-a".to_owned(),
                device_name: "ExamplePhoneA".to_owned(),
                token_hash_sha256: "a".repeat(64),
                paired_at_epoch_ms: 1_000,
            },
        ];
        paths
            .save_devices(&devices)
            .expect("devices should persist");
        let loaded_devices = paths.load_devices().expect("devices should reload");
        assert_eq!(loaded_devices.len(), 2);
        assert_eq!(loaded_devices[0].device_id, "example-device-a");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn generates_and_reloads_host_tls_identity() {
        let dir = temp_storage_dir("tls");
        let paths = StoragePaths::new(dir.clone());
        let san = vec!["localhost".to_owned(), "127.0.0.1".to_owned()];

        let generated = paths
            .load_or_create_tls_identity(&san)
            .expect("first load generates TLS identity");
        assert_eq!(generated.fingerprint_sha256.len(), 64);

        let reloaded = paths
            .load_or_create_tls_identity(&san)
            .expect("second load reuses persisted TLS identity");
        assert_eq!(reloaded.fingerprint_sha256, generated.fingerprint_sha256);
        assert_eq!(reloaded.cert_pem, generated.cert_pem);
        assert_eq!(reloaded.key_pem, generated.key_pem);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn initializes_and_persists_deck_panels() {
        let dir = temp_storage_dir("panels");
        let paths = StoragePaths::new(dir.clone());

        let deck = paths
            .load_or_init_panels()
            .expect("starter panels should initialize");
        assert_eq!(deck, DeckConfig::default_starter());
        assert!(paths.panels_path().exists());

        let mut updated = deck;
        updated.panels[0].refresh_interval_ms = 500;
        paths
            .save_panels(&updated)
            .expect("modified panels should persist");

        let reloaded = paths
            .load_or_init_panels()
            .expect("modified panels should reload");
        assert_eq!(reloaded.panels[0].refresh_interval_ms, 500);

        let _ = fs::remove_dir_all(&dir);
    }
}
