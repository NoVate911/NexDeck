//! Plugin Engine contracts for isolated extension execution.
//!
//! Per ADR-0004, **WebAssembly (WASM)** is the primary runtime for Stage 6,
//! enforcing capability permissions, strict memory/CPU/time quotas, and
//! deterministic termination of hung plugins.

use std::collections::HashSet;
use std::fmt;

/// Maximum permitted per-plugin memory quota (`64 MiB`).
pub const MAX_PLUGIN_MEMORY_BYTES: usize = 64 * 1024 * 1024;

/// Maximum permitted single-invocation timeout (`10 seconds`).
pub const MAX_PLUGIN_EXECUTION_MS: u64 = 10_000;

/// Supported plugin execution runtime kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginRuntimeKind {
    /// Primary isolated sandbox runtime (`wasmtime`, Stage 6).
    Wasm,
    /// Optional out-of-process JavaScript bridge (future).
    ExternalJavaScript,
    /// Optional out-of-process Python bridge (future).
    ExternalPython,
}

/// Capabilities a plugin can request in its manifest (deny-by-default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginPermission {
    /// Read authorized hardware telemetry snapshots.
    MetricsRead,
    /// Publish notifications into the Notification Center.
    NotificationPublish,
    /// Provide custom widget rendering data.
    WidgetRender,
    /// Request execution of an explicitly allowlisted command.
    AllowedCommandInvoke,
}

/// Resource limits enforced on every sandboxed plugin instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimits {
    /// Maximum linear memory bytes allocated to the plugin.
    pub max_memory_bytes: usize,
    /// Maximum execution duration per tick/call in milliseconds.
    pub max_execution_ms: u64,
    /// Maximum percentage of a single logical CPU core (`1..=100`).
    pub max_cpu_share_pct: u8,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: 4 * 1024 * 1024,
            max_execution_ms: 250,
            max_cpu_share_pct: 10,
        }
    }
}

/// Runtime lifecycle state of a registered plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginLifecycleState {
    /// Manifest verified, module not yet instantiated.
    Registered,
    /// Active inside sandbox.
    Running,
    /// Paused by user or policy.
    Suspended,
    /// Stopped gracefully or killed due to timeout/hang.
    Terminated,
    /// Disabled after exceeding resource limits or trapping.
    Faulted,
}

/// Declarative metadata describing an isolated NexDeck plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifest {
    /// Unique reverse-DNS or slug identifier.
    pub id: String,
    /// Human-readable display name.
    pub name: String,
    /// Semantic version string.
    pub version: String,
    /// Runtime target (WASM primary).
    pub runtime: PluginRuntimeKind,
    /// Requested permissions.
    pub permissions: HashSet<PluginPermission>,
    /// Enforced resource quotas.
    pub limits: ResourceLimits,
}

/// Errors returned during plugin manifest validation or lifecycle transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginError {
    /// Manifest contained empty or invalid fields.
    InvalidManifest(&'static str),
    /// Requested resource limits exceed host policy ceilings.
    QuotaExceeded(&'static str),
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidManifest(msg) => write!(f, "invalid plugin manifest: {msg}"),
            Self::QuotaExceeded(msg) => write!(f, "plugin quota exceeds host policy: {msg}"),
        }
    }
}

impl std::error::Error for PluginError {}

impl PluginManifest {
    /// Validates manifest fields and resource quota bounds before registration.
    ///
    /// # Errors
    ///
    /// Returns [`PluginError::InvalidManifest`] or [`PluginError::QuotaExceeded`]
    /// if any constraint violates host policy.
    pub fn validate(&self) -> Result<(), PluginError> {
        if self.id.trim().is_empty() {
            return Err(PluginError::InvalidManifest("plugin id must not be empty"));
        }
        if self.name.trim().is_empty() {
            return Err(PluginError::InvalidManifest(
                "plugin name must not be empty",
            ));
        }
        if self.version.trim().is_empty() {
            return Err(PluginError::InvalidManifest(
                "plugin version must not be empty",
            ));
        }
        if self.limits.max_memory_bytes == 0
            || self.limits.max_memory_bytes > MAX_PLUGIN_MEMORY_BYTES
        {
            return Err(PluginError::QuotaExceeded(
                "max_memory_bytes must be between 1 byte and 64 MiB",
            ));
        }
        if self.limits.max_execution_ms == 0
            || self.limits.max_execution_ms > MAX_PLUGIN_EXECUTION_MS
        {
            return Err(PluginError::QuotaExceeded(
                "max_execution_ms must be between 1 ms and 10000 ms",
            ));
        }
        if self.limits.max_cpu_share_pct == 0 || self.limits.max_cpu_share_pct > 100 {
            return Err(PluginError::QuotaExceeded(
                "max_cpu_share_pct must be between 1 and 100",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_manifest_limits_validate_cleanly() {
        let manifest = PluginManifest {
            id: "dev.nexdeck.clock".to_owned(),
            name: "Clock Widget".to_owned(),
            version: "0.1.0".to_owned(),
            runtime: PluginRuntimeKind::Wasm,
            permissions: HashSet::from([PluginPermission::WidgetRender]),
            limits: ResourceLimits::default(),
        };
        assert_eq!(manifest.validate(), Ok(()));
    }

    #[test]
    fn rejects_excessive_memory_quota() {
        let manifest = PluginManifest {
            id: "dev.nexdeck.heavy".to_owned(),
            name: "Heavy Plugin".to_owned(),
            version: "0.1.0".to_owned(),
            runtime: PluginRuntimeKind::Wasm,
            permissions: HashSet::new(),
            limits: ResourceLimits {
                max_memory_bytes: MAX_PLUGIN_MEMORY_BYTES + 1,
                ..ResourceLimits::default()
            },
        };
        assert!(matches!(
            manifest.validate(),
            Err(PluginError::QuotaExceeded(_))
        ));
    }
}
