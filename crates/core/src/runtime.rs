//! Asynchronous `tokio` runtime orchestration for `nexdeck-agent`.

use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nexdeck_monitoring::MetricCollector;
use nexdeck_transport::control::{ControlCommandRequest, ControlCommandResponse};
use nexdeck_transport::pairing::{DeviceRecord, DeviceRegistry, PairingQrPayload};
use nexdeck_transport::server::{InboundFrame, WssServerHandle, spawn_wss_server};
use nexdeck_transport::tls::HostTlsIdentity;
use nexdeck_transport::{Envelope, MessageKind, PROTOCOL_VERSION, TransportError};
use tokio::sync::{RwLock, broadcast, mpsc, watch};
use tokio::time::MissedTickBehavior;

use crate::control::{ControlExecutor, default_control_executor};
use crate::storage::{StorageError, StoragePaths};
use crate::{AgentConfig, CoreAgent, CoreError};

/// Options controlling the async [`RunningAgent`] runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeOptions {
    /// Listening address for the HTTPS/WSS server.
    pub bind_addr: SocketAddr,
    /// Subject Alternative Name entries if TLS identity must be generated dynamically.
    pub san_hosts: Vec<String>,
    /// Core agent policy and polling settings.
    pub agent: AgentConfig,
}

/// Errors produced during async runtime startup or execution.
#[derive(Debug)]
pub enum RuntimeError {
    /// Core configuration invariant validation failed.
    Core(CoreError),
    /// Transport server startup or protocol handling failed.
    Transport(TransportError),
    /// Persistent device registry synchronization failed.
    Storage(StorageError),
    /// Background task exited unexpectedly.
    TaskJoin(String),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(err) => write!(f, "core initialization error: {err}"),
            Self::Transport(err) => write!(f, "transport runtime error: {err}"),
            Self::Storage(err) => write!(f, "storage sync error: {err}"),
            Self::TaskJoin(msg) => write!(f, "runtime task failed: {msg}"),
        }
    }
}

impl std::error::Error for RuntimeError {}

impl From<CoreError> for RuntimeError {
    fn from(value: CoreError) -> Self {
        Self::Core(value)
    }
}

impl From<TransportError> for RuntimeError {
    fn from(value: TransportError) -> Self {
        Self::Transport(value)
    }
}

impl From<StorageError> for RuntimeError {
    fn from(value: StorageError) -> Self {
        Self::Storage(value)
    }
}

/// Active handle to the running headless `nexdeck-agent` async loop.
pub struct RunningAgent {
    local_addr: SocketAddr,
    fingerprint_sha256: String,
    registry: Arc<RwLock<DeviceRegistry>>,
    broadcast_rx_source: broadcast::Sender<String>,
    shutdown_tx: watch::Sender<bool>,
    task: tokio::task::JoinHandle<Result<(), RuntimeError>>,
}

impl RunningAgent {
    /// Returns the actual socket address bound by the WSS server.
    #[must_use]
    pub const fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Returns the lowercase SHA-256 hex fingerprint of the active host TLS certificate.
    #[must_use]
    pub fn fingerprint_sha256(&self) -> &str {
        &self.fingerprint_sha256
    }

    /// Returns shared access to the active [`DeviceRegistry`].
    #[must_use]
    pub const fn registry(&self) -> &Arc<RwLock<DeviceRegistry>> {
        &self.registry
    }

    /// Subscribes directly to serialized outbound JSON [`Envelope`] broadcasts.
    #[must_use]
    pub fn subscribe_broadcast(&self) -> broadcast::Receiver<String> {
        self.broadcast_rx_source.subscribe()
    }

    /// Opens a single-use pairing session and returns a mobile QR onboarding payload.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError`] if `ttl_secs == 0` or CSPRNG PIN generation fails.
    pub async fn start_pairing(
        &self,
        advertised_host: impl Into<String>,
        ttl_secs: u64,
    ) -> Result<PairingQrPayload, TransportError> {
        let pin = {
            let mut reg = self.registry.write().await;
            reg.start_pairing(now_epoch_ms(), ttl_secs)?
        };
        Ok(PairingQrPayload {
            protocol_version: PROTOCOL_VERSION,
            host: advertised_host.into(),
            port: self.local_addr.port(),
            fingerprint_sha256: self.fingerprint_sha256.clone(),
            pin,
        })
    }

    /// Requests graceful shutdown and awaits termination of the agent event loop.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError`] if the background runtime loop returned an error or panicked.
    pub async fn shutdown(self) -> Result<(), RuntimeError> {
        let _ = self.shutdown_tx.send(true);
        self.task
            .await
            .map_err(|err| RuntimeError::TaskJoin(err.to_string()))?
    }
}

/// Spawns the headless `nexdeck-agent` async runtime onto the active Tokio executor.
///
/// The loop binds the TLS/WSS listener, periodically polls `collector` to broadcast
/// [`MessageKind::Telemetry`] envelopes, ingests inbound authenticated frames, and
/// flushes pairing updates to `storage` whenever `DeviceRegistry` changes.
///
/// # Errors
///
/// Returns [`RuntimeError`] if core validation fails or the WSS server fails to bind.
pub async fn spawn_agent_runtime(
    options: RuntimeOptions,
    collector: Box<dyn MetricCollector + Send>,
    tls: HostTlsIdentity,
    storage: Option<StoragePaths>,
    initial_devices: Vec<DeviceRecord>,
) -> Result<RunningAgent, RuntimeError> {
    spawn_agent_runtime_with_executor(
        options,
        collector,
        default_control_executor(),
        tls,
        storage,
        initial_devices,
    )
    .await
}

/// Spawns the headless `nexdeck-agent` async runtime with a custom [`ControlExecutor`].
///
/// Useful for deterministic testing or custom host command executors without
/// injecting real OS keyboard events.
///
/// # Errors
///
/// Returns [`RuntimeError`] if core validation fails or the WSS server fails to bind.
pub async fn spawn_agent_runtime_with_executor(
    options: RuntimeOptions,
    mut collector: Box<dyn MetricCollector + Send>,
    executor: Box<dyn ControlExecutor + Send>,
    tls: HostTlsIdentity,
    storage: Option<StoragePaths>,
    initial_devices: Vec<DeviceRecord>,
) -> Result<RunningAgent, RuntimeError> {
    let mut core = CoreAgent::new(options.agent.clone())?;
    let poll_interval = Duration::from_millis(options.agent.poll_interval_ms);

    let mut last_saved_devices = initial_devices.clone();
    last_saved_devices.sort_by(|left, right| left.device_id.cmp(&right.device_id));
    let registry = Arc::new(RwLock::new(DeviceRegistry::from_records(initial_devices)));

    let (inbound_tx, mut inbound_rx) = mpsc::channel::<InboundFrame>(64);
    let server =
        spawn_wss_server(options.bind_addr, &tls, Arc::clone(&registry), inbound_tx).await?;
    let local_addr = server.local_addr();
    let fingerprint_sha256 = server.fingerprint_sha256().to_owned();

    // Reuse an internal fan-out mirror or direct `WssServerHandle::subscribe_broadcast`.
    let (mirror_tx, _) = broadcast::channel::<String>(128);
    let mirror_tx_for_loop = mirror_tx.clone();
    let mut server_rx = server.subscribe_broadcast();
    let forwarder = tokio::spawn(async move {
        while let Ok(msg) = server_rx.recv().await {
            let _ = mirror_tx_for_loop.send(msg);
        }
    });

    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let registry_for_loop = Arc::clone(&registry);

    let task = tokio::spawn(async move {
        let mut tick = tokio::time::interval(poll_interval);
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut sequence_id: u64 = 1;

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    publish_telemetry_sample(&mut core, collector.as_mut(), &server, &mut sequence_id);
                    if let Some(paths) = &storage {
                        sync_devices_if_changed(paths, &registry_for_loop, &mut last_saved_devices).await?;
                    }
                }
                maybe_frame = inbound_rx.recv() => {
                    match maybe_frame {
                        Some(frame) => {
                            handle_inbound_frame(&server, executor.as_ref(), &frame, &mut sequence_id);
                        }
                        None => break,
                    }
                }
                changed = shutdown_rx.changed() => {
                    if changed.is_err() || *shutdown_rx.borrow() {
                        break;
                    }
                }
            }
        }

        if let Some(paths) = &storage {
            sync_devices_if_changed(paths, &registry_for_loop, &mut last_saved_devices).await?;
        }
        server.stop().await;
        forwarder.abort();
        Ok(())
    });

    Ok(RunningAgent {
        local_addr,
        fingerprint_sha256,
        registry,
        broadcast_rx_source: mirror_tx,
        shutdown_tx,
        task,
    })
}

fn publish_telemetry_sample(
    core: &mut CoreAgent,
    collector: &mut (dyn MetricCollector + Send),
    server: &WssServerHandle,
    sequence_id: &mut u64,
) {
    let Ok(snapshot) = collector.collect() else {
        return;
    };
    core.record_snapshot(snapshot.clone());

    let Ok(payload) = serde_json::to_vec(&snapshot) else {
        return;
    };
    let envelope = Envelope::new(*sequence_id, MessageKind::Telemetry, payload);
    *sequence_id = next_sequence_id(*sequence_id);
    let _ = server.broadcast_envelope(&envelope);
}

fn handle_inbound_frame(
    server: &WssServerHandle,
    executor: &dyn ControlExecutor,
    frame: &InboundFrame,
    sequence_id: &mut u64,
) {
    if let Some(reply) = build_inbound_reply(executor, frame, *sequence_id) {
        *sequence_id = next_sequence_id(*sequence_id);
        let _ = server.broadcast_envelope(&reply);
    }
}

/// Validates and routes an authenticated [`InboundFrame`], producing an optional response [`Envelope`].
#[must_use]
pub fn build_inbound_reply(
    executor: &dyn ControlExecutor,
    frame: &InboundFrame,
    reply_sequence_id: u64,
) -> Option<Envelope> {
    if frame.envelope.validate().is_err() {
        return None;
    }

    match frame.envelope.kind {
        MessageKind::Heartbeat => Some(Envelope::new(
            reply_sequence_id,
            MessageKind::Heartbeat,
            Vec::new(),
        )),
        MessageKind::ControlCommand => {
            let Ok(req) = serde_json::from_slice::<ControlCommandRequest>(&frame.envelope.payload)
            else {
                return None;
            };
            let response = match req.validate() {
                Ok(()) => match executor.execute(req.action) {
                    Ok(message) => ControlCommandResponse {
                        command_id: req.command_id,
                        action: req.action,
                        ok: true,
                        message,
                    },
                    Err(err) => ControlCommandResponse {
                        command_id: req.command_id,
                        action: req.action,
                        ok: false,
                        message: err,
                    },
                },
                Err(err) => ControlCommandResponse {
                    command_id: req.command_id,
                    action: req.action,
                    ok: false,
                    message: err.to_string(),
                },
            };
            let payload = serde_json::to_vec(&response).ok()?;
            Some(Envelope::new(
                reply_sequence_id,
                MessageKind::ControlCommand,
                payload,
            ))
        }
        MessageKind::Telemetry | MessageKind::Notification | MessageKind::Approval => {
            // Subsystem routing for notifications and remote approvals lands in Stages 4–5.
            let _ = &frame.device_id;
            None
        }
    }
}

const fn next_sequence_id(current: u64) -> u64 {
    let next = current.wrapping_add(1);
    if next == 0 { 1 } else { next }
}

async fn sync_devices_if_changed(
    paths: &StoragePaths,
    registry: &Arc<RwLock<DeviceRegistry>>,
    last_saved: &mut Vec<DeviceRecord>,
) -> Result<(), StorageError> {
    let mut current = registry.read().await.export_records();
    current.sort_by(|left, right| left.device_id.cmp(&right.device_id));
    if current != *last_saved {
        paths.save_devices(&current)?;
        *last_saved = current;
    }
    Ok(())
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1_000_000, |d| {
            u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::net::{IpAddr, Ipv4Addr};

    use nexdeck_monitoring::{CpuMetrics, MemoryMetrics, StaticCollector, SystemSnapshot};

    fn sample_snapshot() -> SystemSnapshot {
        SystemSnapshot {
            captured_at_epoch_ms: 42_000,
            uptime_secs: 3600,
            cpu: CpuMetrics {
                total_usage_pct: 14.5,
                per_core_usage_pct: vec![12.0, 17.0],
                frequency_mhz: Some(3_800),
                temperature_celsius: None,
            },
            memory: MemoryMetrics {
                total_bytes: 16 * 1024 * 1024 * 1024,
                used_bytes: 4 * 1024 * 1024 * 1024,
            },
            gpus: Vec::new(),
            disks: Vec::new(),
            networks: Vec::new(),
            battery: None,
        }
    }

    #[test]
    fn runtime_broadcasts_telemetry_and_shuts_down_cleanly() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current_thread runtime should build");

        rt.block_on(async {
            let temp_dir = std::env::temp_dir().join(format!(
                "nexdeck-runtime-test-{}-{}",
                std::process::id(),
                now_epoch_ms()
            ));
            let storage = StoragePaths::new(temp_dir.clone());
            let tls = HostTlsIdentity::generate_self_signed(&["localhost".to_owned()])
                .expect("TLS identity generation");

            let options = RuntimeOptions {
                bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
                san_hosts: vec!["localhost".to_owned()],
                agent: AgentConfig {
                    poll_interval_ms: 100,
                    ..AgentConfig::default()
                },
            };

            let collector = Box::new(StaticCollector::new(sample_snapshot()));
            let agent =
                spawn_agent_runtime(options, collector, tls, Some(storage.clone()), Vec::new())
                    .await
                    .expect("runtime should start");

            let mut rx = agent.subscribe_broadcast();
            let pairing = agent
                .start_pairing("127.0.0.1", 120)
                .await
                .expect("pairing should open");
            assert_eq!(pairing.port, agent.local_addr().port());
            assert_eq!(pairing.fingerprint_sha256, agent.fingerprint_sha256());

            let frame_json = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .expect("telemetry should broadcast promptly")
                .expect("broadcast channel should remain open");

            let envelope: Envelope =
                serde_json::from_str(&frame_json).expect("valid envelope JSON");
            assert_eq!(envelope.kind, MessageKind::Telemetry);
            let decoded_snapshot: SystemSnapshot =
                serde_json::from_slice(&envelope.payload).expect("valid snapshot payload");
            assert_eq!(decoded_snapshot, sample_snapshot());

            // Complete pairing and confirm runtime flushes the new record to devices.json on shutdown.
            {
                let mut reg = agent.registry().write().await;
                let creds = reg
                    .complete_pairing(
                        &pairing.pin,
                        "example-device-1".to_owned(),
                        "ExamplePhone".to_owned(),
                        pairing.fingerprint_sha256.clone(),
                        now_epoch_ms(),
                    )
                    .expect("valid pin should pair");
                assert_eq!(creds.device_id, "example-device-1");
            }

            agent
                .shutdown()
                .await
                .expect("graceful shutdown should succeed");

            let persisted = storage
                .load_devices()
                .expect("devices.json should persist paired device");
            assert_eq!(persisted.len(), 1);
            assert_eq!(persisted[0].device_id, "example-device-1");

            let _ = fs::remove_dir_all(&temp_dir);
        });
    }

    #[test]
    fn routes_valid_control_command_and_rejects_invalid_id() {
        use crate::control::RecordingControlExecutor;
        use nexdeck_transport::control::ControlAction;

        let executor = RecordingControlExecutor::new();
        let req = ControlCommandRequest {
            command_id: "cmd-7".to_owned(),
            action: ControlAction::MediaPlayPause,
        };
        let frame = InboundFrame {
            device_id: "example-device-1".to_owned(),
            envelope: Envelope::new(
                1,
                MessageKind::ControlCommand,
                serde_json::to_vec(&req).expect("encode"),
            ),
        };

        let reply = build_inbound_reply(&executor, &frame, 42).expect("reply expected");
        assert_eq!(reply.kind, MessageKind::ControlCommand);
        let decoded: ControlCommandResponse =
            serde_json::from_slice(&reply.payload).expect("decode response");
        assert!(decoded.ok);
        assert_eq!(decoded.command_id, "cmd-7");
        assert_eq!(decoded.action, ControlAction::MediaPlayPause);
        assert_eq!(executor.actions(), vec![ControlAction::MediaPlayPause]);

        let invalid_req = ControlCommandRequest {
            command_id: "bad id with spaces".to_owned(),
            action: ControlAction::VolumeUp,
        };
        let invalid_frame = InboundFrame {
            device_id: "example-device-1".to_owned(),
            envelope: Envelope::new(
                2,
                MessageKind::ControlCommand,
                serde_json::to_vec(&invalid_req).expect("encode"),
            ),
        };
        let invalid_reply =
            build_inbound_reply(&executor, &invalid_frame, 43).expect("error reply expected");
        let decoded_err: ControlCommandResponse =
            serde_json::from_slice(&invalid_reply.payload).expect("decode error");
        assert!(!decoded_err.ok);
        assert_eq!(executor.actions(), vec![ControlAction::MediaPlayPause]);
    }
}
