//! Headless Core Agent orchestration crate for NexDeck.
//!
//! Coordinates [`nexdeck_monitoring`], [`nexdeck_transport`],
//! [`nexdeck_notifications`], [`nexdeck_approval`], [`nexdeck_api`], and
//! [`nexdeck_plugin_engine`] with zero heavy UI dependencies.

pub mod alerts;
pub mod api_routes;
pub mod autostart;
pub mod control;
pub mod process_metrics;
pub mod runtime;
pub mod storage;

use std::fmt;

use nexdeck_api::ClientPolicy;
use nexdeck_approval::ApprovalRegistry;
use nexdeck_monitoring::SystemSnapshot;
use nexdeck_notifications::{NotificationFilter, NotificationQueue};
use nexdeck_plugin_engine::{PluginError, PluginManifest};
use nexdeck_transport::TransportKind;
use serde::{Deserialize, Serialize};

use crate::alerts::AlertThresholds;

/// Crate version synchronized via workspace metadata.
pub const AGENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Target idle memory guidance for minimal Core Agent builds (5 MiB baseline).
pub const TARGET_IDLE_RAM_BUDGET_BYTES: usize = 5 * 1024 * 1024;

/// Configuration settings for the background `nexdeck-agent` process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    /// Primary transport enabled at startup (MVP: [`TransportKind::WifiWebSocket`]).
    pub primary_transport: TransportKind,
    /// Telemetry sampling interval in milliseconds (`>= 100`).
    pub poll_interval_ms: u64,
    /// Bounded queue capacity for buffered sleep/disconnect notifications.
    pub notification_queue_capacity: usize,
    /// Whether the isolated WASM plugin engine is enabled.
    pub enable_plugins: bool,
    /// Notification priority threshold and muted origins filter.
    #[serde(default)]
    pub notification_filter: NotificationFilter,
    /// Automatic hardware monitoring alert thresholds.
    #[serde(default)]
    pub alert_thresholds: AlertThresholds,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            primary_transport: TransportKind::WifiWebSocket,
            poll_interval_ms: 1_000,
            notification_queue_capacity: 256,
            enable_plugins: false,
            notification_filter: NotificationFilter::default(),
            alert_thresholds: AlertThresholds::default(),
        }
    }
}

/// Errors produced when validating or initializing [`CoreAgent`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    /// Configuration failed invariant validation.
    InvalidConfig(&'static str),
    /// Plugin registration failed manifest or quota validation.
    Plugin(PluginError),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(msg) => write!(f, "invalid agent configuration: {msg}"),
            Self::Plugin(err) => write!(f, "plugin error: {err}"),
        }
    }
}

impl std::error::Error for CoreError {}

impl From<PluginError> for CoreError {
    fn from(value: PluginError) -> Self {
        Self::Plugin(value)
    }
}

/// Headless Core Agent coordinator holding Stage 0 subsystem state.
#[derive(Debug)]
pub struct CoreAgent {
    config: AgentConfig,
    notifications: NotificationQueue,
    approvals: ApprovalRegistry,
    clients: Vec<ClientPolicy>,
    plugins: Vec<PluginManifest>,
    latest_snapshot: Option<SystemSnapshot>,
}

impl CoreAgent {
    /// Validates `config` and initializes a [`CoreAgent`] instance.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidConfig`] if `poll_interval_ms < 100` or
    /// `notification_queue_capacity == 0`.
    pub fn new(config: AgentConfig) -> Result<Self, CoreError> {
        if config.poll_interval_ms < 100 {
            return Err(CoreError::InvalidConfig(
                "poll_interval_ms must be at least 100 ms",
            ));
        }
        if config.notification_queue_capacity == 0 {
            return Err(CoreError::InvalidConfig(
                "notification_queue_capacity must be greater than 0",
            ));
        }
        let notifications = NotificationQueue::new(config.notification_queue_capacity);
        Ok(Self {
            config,
            notifications,
            approvals: ApprovalRegistry::new(),
            clients: Vec::new(),
            plugins: Vec::new(),
            latest_snapshot: None,
        })
    }

    /// Returns the active [`AgentConfig`].
    #[must_use]
    pub const fn config(&self) -> &AgentConfig {
        &self.config
    }

    /// Returns mutable access to the bounded [`NotificationQueue`].
    pub fn notifications_mut(&mut self) -> &mut NotificationQueue {
        &mut self.notifications
    }

    /// Returns mutable access to the [`ApprovalRegistry`].
    pub fn approvals_mut(&mut self) -> &mut ApprovalRegistry {
        &mut self.approvals
    }

    /// Registers an authorized external Universal API client policy.
    pub fn register_client(&mut self, policy: ClientPolicy) {
        self.clients.push(policy);
    }

    /// Returns registered Universal API client policies.
    #[must_use]
    pub fn clients(&self) -> &[ClientPolicy] {
        &self.clients
    }

    /// Validates and registers a plugin manifest under configured policy bounds.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Plugin`] if the manifest or quotas violate host policy.
    pub fn register_plugin(&mut self, manifest: PluginManifest) -> Result<(), CoreError> {
        manifest.validate()?;
        self.plugins.push(manifest);
        Ok(())
    }

    /// Returns registered plugin manifests.
    #[must_use]
    pub fn plugins(&self) -> &[PluginManifest] {
        &self.plugins
    }

    /// Records the latest collected hardware telemetry snapshot.
    pub fn record_snapshot(&mut self, snapshot: SystemSnapshot) {
        self.latest_snapshot = Some(snapshot);
    }

    /// Returns the most recently recorded hardware snapshot, if available.
    #[must_use]
    pub const fn latest_snapshot(&self) -> Option<&SystemSnapshot> {
        self.latest_snapshot.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    use nexdeck_plugin_engine::{PluginPermission, PluginRuntimeKind, ResourceLimits};

    #[test]
    fn default_config_initializes_agent() {
        let agent = CoreAgent::new(AgentConfig::default());
        assert!(agent.is_ok());
    }

    #[test]
    fn rejects_excessively_fast_polling_interval() {
        let bad = AgentConfig {
            poll_interval_ms: 10,
            ..AgentConfig::default()
        };
        assert!(matches!(
            CoreAgent::new(bad),
            Err(CoreError::InvalidConfig(_))
        ));
    }

    #[test]
    fn registers_valid_plugin_manifest() {
        let mut agent = CoreAgent::new(AgentConfig::default()).expect("valid default config");
        let manifest = PluginManifest {
            id: "org.example.sample".to_owned(),
            name: "Sample".to_owned(),
            version: "0.1.0".to_owned(),
            runtime: PluginRuntimeKind::Wasm,
            permissions: HashSet::from([PluginPermission::WidgetRender]),
            limits: ResourceLimits::default(),
        };
        assert_eq!(agent.register_plugin(manifest), Ok(()));
        assert_eq!(agent.plugins().len(), 1);
    }
}
