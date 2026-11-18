//! Asynchronous `tokio` runtime orchestration for `nexdeck-agent`.

use std::fmt;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nexdeck_api::clients::{ClientRecord, ClientRegistry};
use nexdeck_approval::{ApprovalDecisionMessage, ApprovalRegistry};
use nexdeck_monitoring::{MetricCollector, SystemSnapshot};
use nexdeck_notifications::{
    NotificationEvent, NotificationFilter, NotificationQueue, NotificationSyncCommand,
};
use nexdeck_transport::control::{ControlCommandRequest, ControlCommandResponse};
use nexdeck_transport::pairing::{DeviceRecord, DeviceRegistry, PairingQrPayload};
use nexdeck_transport::server::{InboundFrame, WssServerHandle, spawn_wss_server_with_backlog};
use nexdeck_transport::tls::HostTlsIdentity;
use nexdeck_transport::{Envelope, MessageKind, PROTOCOL_VERSION, TransportError};
use tokio::sync::{RwLock, broadcast, mpsc, oneshot, watch};
use tokio::time::MissedTickBehavior;

use crate::alerts::{AlertThresholds, HardwareAlertMonitor};
use crate::api_routes::{ApiRouterState, build_api_router};
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
    clients: Arc<RwLock<ClientRegistry>>,
    approvals: Arc<RwLock<ApprovalRegistry>>,
    notifications: Arc<RwLock<NotificationQueue>>,
    notification_filter: Arc<RwLock<NotificationFilter>>,
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

    /// Returns shared access to the Universal API [`ClientRegistry`].
    #[must_use]
    pub const fn clients(&self) -> &Arc<RwLock<ClientRegistry>> {
        &self.clients
    }

    /// Returns shared access to the [`ApprovalRegistry`].
    #[must_use]
    pub const fn approvals(&self) -> &Arc<RwLock<ApprovalRegistry>> {
        &self.approvals
    }

    /// Returns shared access to the [`NotificationQueue`].
    #[must_use]
    pub const fn notifications(&self) -> &Arc<RwLock<NotificationQueue>> {
        &self.notifications
    }

    /// Returns shared access to the active [`NotificationFilter`].
    #[must_use]
    pub const fn notification_filter(&self) -> &Arc<RwLock<NotificationFilter>> {
        &self.notification_filter
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
    collector: Box<dyn MetricCollector + Send>,
    executor: Box<dyn ControlExecutor + Send>,
    tls: HostTlsIdentity,
    storage: Option<StoragePaths>,
    initial_devices: Vec<DeviceRecord>,
) -> Result<RunningAgent, RuntimeError> {
    let initial_clients = match &storage {
        Some(paths) => paths.load_clients()?,
        None => Vec::new(),
    };
    spawn_agent_runtime_full(
        options,
        collector,
        executor,
        tls,
        storage,
        initial_devices,
        initial_clients,
    )
    .await
}

/// Spawns the headless `nexdeck-agent` runtime with explicit initial devices and API clients.
///
/// # Errors
///
/// Returns [`RuntimeError`] if core validation fails or the WSS server fails to bind.
pub async fn spawn_agent_runtime_full(
    options: RuntimeOptions,
    mut collector: Box<dyn MetricCollector + Send>,
    executor: Box<dyn ControlExecutor + Send>,
    tls: HostTlsIdentity,
    storage: Option<StoragePaths>,
    initial_devices: Vec<DeviceRecord>,
    initial_clients: Vec<ClientRecord>,
) -> Result<RunningAgent, RuntimeError> {
    let mut core = CoreAgent::new(options.agent.clone())?;
    let poll_interval = Duration::from_millis(options.agent.poll_interval_ms);

    let mut last_saved_devices = initial_devices.clone();
    last_saved_devices.sort_by(|left, right| left.device_id.cmp(&right.device_id));
    let registry = Arc::new(RwLock::new(DeviceRegistry::from_records(initial_devices)));

    let mut last_saved_clients = initial_clients.clone();
    last_saved_clients.sort_by(|left, right| left.client_id.cmp(&right.client_id));
    let clients = Arc::new(RwLock::new(ClientRegistry::from_records(initial_clients)));

    let latest_snapshot: Arc<RwLock<Option<SystemSnapshot>>> = Arc::new(RwLock::new(None));
    let approvals = Arc::new(RwLock::new(ApprovalRegistry::new()));

    let mut initial_notifications = match &storage {
        Some(paths) => paths.load_notifications()?,
        None => Vec::new(),
    };
    let initial_queue = NotificationQueue::from_items(
        options.agent.notification_queue_capacity,
        initial_notifications.clone(),
    );
    initial_notifications = initial_queue.items();
    let next_notification_id = Arc::new(AtomicU64::new(initial_queue.next_id_hint()));
    let notifications = Arc::new(RwLock::new(initial_queue));

    let mut initial_filter = options.agent.notification_filter.clone();
    initial_filter.normalize();
    let notification_filter = Arc::new(RwLock::new(initial_filter));
    let alert_thresholds = Arc::new(RwLock::new(options.agent.alert_thresholds.clone()));

    let shared_executor: Arc<Mutex<Box<dyn ControlExecutor + Send>>> =
        Arc::new(Mutex::new(executor));

    let (mirror_tx, _) = broadcast::channel::<String>(128);
    let (api_outbound_tx, mut api_outbound_rx) = mpsc::channel::<Envelope>(64);

    let api_router = build_api_router(
        ApiRouterState::new(
            Arc::clone(&clients),
            Arc::clone(&latest_snapshot),
            Arc::clone(&approvals),
            Arc::clone(&notifications),
            Arc::clone(&shared_executor),
            api_outbound_tx,
            mirror_tx.clone(),
        )
        .with_next_notification_id(Arc::clone(&next_notification_id)),
    );

    let (inbound_tx, mut inbound_rx) = mpsc::channel::<InboundFrame>(64);
    let (backlog_tx, mut backlog_rx) = mpsc::channel::<oneshot::Sender<Vec<Envelope>>>(16);
    let server = spawn_wss_server_with_backlog(
        options.bind_addr,
        &tls,
        Arc::clone(&registry),
        inbound_tx,
        Some(api_router),
        Some(backlog_tx),
    )
    .await?;
    let local_addr = server.local_addr();
    let fingerprint_sha256 = server.fingerprint_sha256().to_owned();

    let mirror_tx_for_loop = mirror_tx.clone();
    let mut server_rx = server.subscribe_broadcast();
    let forwarder = tokio::spawn(async move {
        while let Ok(msg) = server_rx.recv().await {
            let _ = mirror_tx_for_loop.send(msg);
        }
    });

    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let registry_for_loop = Arc::clone(&registry);
    let clients_for_loop = Arc::clone(&clients);
    let notifications_for_loop = Arc::clone(&notifications);
    let approvals_for_loop = Arc::clone(&approvals);
    let filter_for_loop = Arc::clone(&notification_filter);
    let thresholds_for_loop = Arc::clone(&alert_thresholds);
    let snapshot_for_loop = Arc::clone(&latest_snapshot);
    let executor_for_loop = Arc::clone(&shared_executor);
    let next_notif_id_for_loop = Arc::clone(&next_notification_id);
    let mut last_saved_notifications = initial_notifications;
    let mut last_config_mtime = storage
        .as_ref()
        .and_then(|p| std::fs::metadata(p.config_path()).ok()?.modified().ok());

    let task = tokio::spawn(async move {
        let mut tick = tokio::time::interval(poll_interval);
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut sequence_id: u64 = 1;
        let mut alert_monitor = HardwareAlertMonitor::new();

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    if let Some(snapshot) = publish_telemetry_sample(
                        &mut core,
                        collector.as_mut(),
                        &server,
                        &mut sequence_id,
                    ) {
                        *snapshot_for_loop.write().await = Some(snapshot.clone());
                        let thresholds = thresholds_for_loop.read().await.clone();
                        let alerts = alert_monitor.evaluate(&snapshot, &thresholds, || {
                            next_notif_id_for_loop.fetch_add(1, Ordering::Relaxed)
                        });
                        if !alerts.is_empty() {
                            let filter = filter_for_loop.read().await.clone();
                            let mut queue = notifications_for_loop.write().await;
                            for event in alerts {
                                queue.push(event.clone());
                                if filter.matches(&event)
                                    && let Ok(payload) = serde_json::to_vec(&event)
                                {
                                    let env = Envelope::new(
                                        sequence_id,
                                        MessageKind::Notification,
                                        payload,
                                    );
                                    sequence_id = next_sequence_id(sequence_id);
                                    let _ = server.broadcast_envelope(&env);
                                }
                            }
                        }
                    }

                    let newly_expired = {
                        let mut guard = approvals_for_loop.write().await;
                        guard.expire_stale(now_epoch_ms())
                    };
                    for req in newly_expired {
                        if let Ok(payload) = serde_json::to_vec(&req) {
                            let env = Envelope::new(sequence_id, MessageKind::Approval, payload);
                            sequence_id = next_sequence_id(sequence_id);
                            let _ = server.broadcast_envelope(&env);
                        }
                    }

                    if let Some(paths) = &storage {
                        sync_devices_if_changed(paths, &registry_for_loop, &mut last_saved_devices).await?;
                        sync_clients_if_changed(paths, &clients_for_loop, &mut last_saved_clients).await?;
                        sync_notifications_if_changed(
                            paths,
                            &notifications_for_loop,
                            &mut last_saved_notifications,
                        )
                        .await?;
                        sync_policy_from_config_if_changed(
                            paths,
                            &mut last_config_mtime,
                            &filter_for_loop,
                            &thresholds_for_loop,
                        )
                        .await;
                    }
                }
                maybe_backlog_req = backlog_rx.recv() => {
                    match maybe_backlog_req {
                        Some(resp_tx) => {
                            let envelopes = build_connect_backlog_envelopes(
                                &snapshot_for_loop,
                                &notifications_for_loop,
                                &filter_for_loop,
                                &approvals_for_loop,
                                &mut sequence_id,
                                now_epoch_ms(),
                            )
                            .await;
                            let _ = resp_tx.send(envelopes);
                        }
                        None => break,
                    }
                }
                maybe_frame = inbound_rx.recv() => {
                    match maybe_frame {
                        Some(frame) => {
                            handle_inbound_frame_async(
                                &server,
                                &executor_for_loop,
                                &approvals_for_loop,
                                &notifications_for_loop,
                                &filter_for_loop,
                                storage.as_ref(),
                                &frame,
                                &mut sequence_id,
                            )
                            .await;
                        }
                        None => break,
                    }
                }
                maybe_api_env = api_outbound_rx.recv() => {
                    if let Some(env) = maybe_api_env {
                        let allow_broadcast = match env.kind {
                            MessageKind::Notification => {
                                if let Ok(event) = serde_json::from_slice::<NotificationEvent>(&env.payload) {
                                    filter_for_loop.read().await.matches(&event)
                                } else {
                                    true
                                }
                            }
                            _ => true,
                        };
                        if allow_broadcast {
                            let _ = server.broadcast_envelope(&env);
                        }
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
            sync_clients_if_changed(paths, &clients_for_loop, &mut last_saved_clients).await?;
            sync_notifications_if_changed(
                paths,
                &notifications_for_loop,
                &mut last_saved_notifications,
            )
            .await?;
        }
        server.stop().await;
        forwarder.abort();
        Ok(())
    });

    Ok(RunningAgent {
        local_addr,
        fingerprint_sha256,
        registry,
        clients,
        approvals,
        notifications,
        notification_filter,
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
) -> Option<SystemSnapshot> {
    let Ok(snapshot) = collector.collect() else {
        return None;
    };
    core.record_snapshot(snapshot.clone());

    if let Ok(payload) = serde_json::to_vec(&snapshot) {
        let envelope = Envelope::new(*sequence_id, MessageKind::Telemetry, payload);
        *sequence_id = next_sequence_id(*sequence_id);
        let _ = server.broadcast_envelope(&envelope);
    }
    Some(snapshot)
}

#[allow(clippy::too_many_arguments)]
async fn handle_inbound_frame_async(
    server: &WssServerHandle,
    executor: &Arc<Mutex<Box<dyn ControlExecutor + Send>>>,
    approvals: &Arc<RwLock<ApprovalRegistry>>,
    notifications: &Arc<RwLock<NotificationQueue>>,
    filter: &Arc<RwLock<NotificationFilter>>,
    storage: Option<&StoragePaths>,
    frame: &InboundFrame,
    sequence_id: &mut u64,
) {
    if frame.envelope.validate().is_err() {
        return;
    }

    match frame.envelope.kind {
        MessageKind::Heartbeat | MessageKind::ControlCommand => {
            if let Ok(exec_guard) = executor.lock()
                && let Some(reply) = build_inbound_reply(exec_guard.as_ref(), frame, *sequence_id)
            {
                *sequence_id = next_sequence_id(*sequence_id);
                let _ = server.broadcast_envelope(&reply);
            }
        }
        MessageKind::Approval => {
            if let Ok(decision) =
                serde_json::from_slice::<ApprovalDecisionMessage>(&frame.envelope.payload)
                && let Some(reply) =
                    process_approval_decision(approvals, &decision, *sequence_id, now_epoch_ms())
                        .await
            {
                *sequence_id = next_sequence_id(*sequence_id);
                let _ = server.broadcast_envelope(&reply);
            }
        }
        MessageKind::Notification => {
            if let Ok(cmd) =
                serde_json::from_slice::<NotificationSyncCommand>(&frame.envelope.payload)
            {
                process_notification_sync(notifications, filter, storage, cmd).await;
            }
        }
        MessageKind::Telemetry => {
            let _ = &frame.device_id;
        }
    }
}

/// Builds initial synchronization [`Envelope`]s sent immediately upon mobile `/ws` connect:
/// latest telemetry snapshot, filtered sleep-buffered notifications, and active pending approvals.
pub async fn build_connect_backlog_envelopes(
    latest_snapshot: &Arc<RwLock<Option<SystemSnapshot>>>,
    notifications: &Arc<RwLock<NotificationQueue>>,
    filter: &Arc<RwLock<NotificationFilter>>,
    approvals: &Arc<RwLock<ApprovalRegistry>>,
    sequence_id: &mut u64,
    now_ms: u64,
) -> Vec<Envelope> {
    let mut envelopes = Vec::new();
    if let Some(snapshot) = latest_snapshot.read().await.clone()
        && let Ok(payload) = serde_json::to_vec(&snapshot)
    {
        envelopes.push(Envelope::new(*sequence_id, MessageKind::Telemetry, payload));
        *sequence_id = next_sequence_id(*sequence_id);
    }

    let active_filter = filter.read().await.clone();
    let buffered_notifs = notifications.read().await.items_matching(&active_filter);
    for event in buffered_notifs {
        if let Ok(payload) = serde_json::to_vec(&event) {
            envelopes.push(Envelope::new(
                *sequence_id,
                MessageKind::Notification,
                payload,
            ));
            *sequence_id = next_sequence_id(*sequence_id);
        }
    }

    let active_approvals = {
        let mut guard = approvals.write().await;
        guard.pending_active(now_ms)
    };
    for req in active_approvals {
        if let Ok(payload) = serde_json::to_vec(&req) {
            envelopes.push(Envelope::new(*sequence_id, MessageKind::Approval, payload));
            *sequence_id = next_sequence_id(*sequence_id);
        }
    }

    envelopes
}

/// Processes a mobile [`ApprovalDecisionMessage`] against `approvals` and produces an
/// authoritative [`Envelope`] reflecting the updated (or previously finalized) state.
pub async fn process_approval_decision(
    approvals: &Arc<RwLock<ApprovalRegistry>>,
    decision: &ApprovalDecisionMessage,
    reply_sequence_id: u64,
    now_ms: u64,
) -> Option<Envelope> {
    let snapshot = {
        let mut guard = approvals.write().await;
        match guard.resolve(&decision.request_id, decision.action.trim(), now_ms) {
            Ok(updated) => Some(updated.clone()),
            Err(_) => guard.get(&decision.request_id).cloned(),
        }
    }?;
    let payload = serde_json::to_vec(&snapshot).ok()?;
    Some(Envelope::new(
        reply_sequence_id,
        MessageKind::Approval,
        payload,
    ))
}

/// Applies a [`NotificationSyncCommand`] from a connected client to host queues and filters.
pub async fn process_notification_sync(
    notifications: &Arc<RwLock<NotificationQueue>>,
    filter: &Arc<RwLock<NotificationFilter>>,
    storage: Option<&StoragePaths>,
    command: NotificationSyncCommand,
) {
    match command {
        NotificationSyncCommand::Dismiss { event_id } => {
            let mut queue = notifications.write().await;
            let _ = queue.remove_by_id(event_id);
        }
        NotificationSyncCommand::ClearAll => {
            let mut queue = notifications.write().await;
            let _ = queue.clear();
        }
        NotificationSyncCommand::UpdateFilter { filter: mut next } => {
            next.normalize();
            *filter.write().await = next.clone();
            if let Some(paths) = storage
                && let Ok(mut settings) = paths.load_or_init_settings()
            {
                settings.agent.notification_filter = next;
                let _ = paths.save_settings(&settings);
            }
        }
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

async fn sync_clients_if_changed(
    paths: &StoragePaths,
    clients: &Arc<RwLock<ClientRegistry>>,
    last_saved: &mut Vec<ClientRecord>,
) -> Result<(), StorageError> {
    let mut current = clients.read().await.export_records();
    current.sort_by(|left, right| left.client_id.cmp(&right.client_id));
    if current != *last_saved {
        paths.save_clients(&current)?;
        *last_saved = current;
    }
    Ok(())
}

async fn sync_notifications_if_changed(
    paths: &StoragePaths,
    notifications: &Arc<RwLock<NotificationQueue>>,
    last_saved: &mut Vec<NotificationEvent>,
) -> Result<(), StorageError> {
    let current = notifications.read().await.items();
    if current != *last_saved {
        paths.save_notifications(&current)?;
        *last_saved = current;
    }
    Ok(())
}

async fn sync_policy_from_config_if_changed(
    paths: &StoragePaths,
    last_mtime: &mut Option<SystemTime>,
    filter: &Arc<RwLock<NotificationFilter>>,
    thresholds: &Arc<RwLock<AlertThresholds>>,
) {
    let current_mtime = std::fs::metadata(paths.config_path())
        .ok()
        .and_then(|m| m.modified().ok());
    if current_mtime.is_none() || current_mtime == *last_mtime {
        return;
    }
    *last_mtime = current_mtime;
    if let Ok(settings) = paths.load_or_init_settings() {
        let mut next_filter = settings.agent.notification_filter;
        next_filter.normalize();
        *filter.write().await = next_filter;
        *thresholds.write().await = settings.agent.alert_thresholds;
    }
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

    #[test]
    fn processes_approval_decisions_replay_and_notification_sync() {
        use nexdeck_approval::{ApprovalRequest, ApprovalStatus, RiskLevel};
        use nexdeck_notifications::{NotificationPriority, NotificationSource};

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        rt.block_on(async {
            let approvals = Arc::new(RwLock::new(ApprovalRegistry::new()));
            {
                let mut guard = approvals.write().await;
                guard
                    .register(ApprovalRequest {
                        request_id: "req-example-1".to_owned(),
                        application: "ExampleApp".to_owned(),
                        title: "Confirm action".to_owned(),
                        description: "Synthetic confirmation".to_owned(),
                        expires_at_epoch_ms: 50_000,
                        risk_level: RiskLevel::High,
                        allowed_actions: vec!["approve".to_owned(), "deny".to_owned()],
                        status: ApprovalStatus::Pending,
                    })
                    .expect("register request");
            }

            let first_env = process_approval_decision(
                &approvals,
                &ApprovalDecisionMessage {
                    request_id: "req-example-1".to_owned(),
                    action: "approve".to_owned(),
                },
                7,
                10_000,
            )
            .await
            .expect("decision envelope expected");
            assert_eq!(first_env.kind, MessageKind::Approval);
            let resolved: ApprovalRequest =
                serde_json::from_slice(&first_env.payload).expect("decode approval");
            assert_eq!(
                resolved.status,
                ApprovalStatus::Resolved("approve".to_owned())
            );

            // Replay attempt with "deny" returns authoritative Resolved("approve") without mutating.
            let replay_env = process_approval_decision(
                &approvals,
                &ApprovalDecisionMessage {
                    request_id: "req-example-1".to_owned(),
                    action: "deny".to_owned(),
                },
                8,
                12_000,
            )
            .await
            .expect("authoritative envelope expected");
            let replay_state: ApprovalRequest =
                serde_json::from_slice(&replay_env.payload).expect("decode replay state");
            assert_eq!(
                replay_state.status,
                ApprovalStatus::Resolved("approve".to_owned())
            );

            // Notification sync: Dismiss, UpdateFilter, ClearAll.
            let queue = Arc::new(RwLock::new(NotificationQueue::new(16)));
            queue.write().await.push(NotificationEvent {
                id: 1,
                source: NotificationSource::Application("ExampleApp".to_owned()),
                title: "Notify 1".to_owned(),
                body: "Body 1".to_owned(),
                priority: NotificationPriority::Normal,
                created_at_epoch_ms: 1_000,
            });
            queue.write().await.push(NotificationEvent {
                id: 2,
                source: NotificationSource::HardwareAlert,
                title: "Notify 2".to_owned(),
                body: "Body 2".to_owned(),
                priority: NotificationPriority::High,
                created_at_epoch_ms: 2_000,
            });
            let filter = Arc::new(RwLock::new(NotificationFilter::default()));

            process_notification_sync(
                &queue,
                &filter,
                None,
                NotificationSyncCommand::Dismiss { event_id: 1 },
            )
            .await;
            assert_eq!(queue.read().await.len(), 1);

            process_notification_sync(
                &queue,
                &filter,
                None,
                NotificationSyncCommand::UpdateFilter {
                    filter: NotificationFilter {
                        min_priority: NotificationPriority::High,
                        muted_origins: vec![" MutedExample ".to_owned()],
                    },
                },
            )
            .await;
            assert_eq!(filter.read().await.min_priority, NotificationPriority::High);
            assert_eq!(filter.read().await.muted_origins, vec!["MutedExample"]);

            process_notification_sync(&queue, &filter, None, NotificationSyncCommand::ClearAll)
                .await;
            assert!(queue.read().await.is_empty());
        });
    }

    #[test]
    fn builds_filtered_connect_backlog_with_telemetry_notifications_and_pending_approvals() {
        use nexdeck_approval::{ApprovalRequest, ApprovalStatus, RiskLevel};
        use nexdeck_notifications::{NotificationPriority, NotificationSource};

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        rt.block_on(async {
            let latest_snapshot = Arc::new(RwLock::new(Some(sample_snapshot())));
            let notifications = Arc::new(RwLock::new(NotificationQueue::new(10)));
            {
                let mut q = notifications.write().await;
                q.push(NotificationEvent {
                    id: 1,
                    source: NotificationSource::Application("MutedExample".to_owned()),
                    title: "Muted".to_owned(),
                    body: "Should be filtered out".to_owned(),
                    priority: NotificationPriority::High,
                    created_at_epoch_ms: 1_000,
                });
                q.push(NotificationEvent {
                    id: 2,
                    source: NotificationSource::HardwareAlert,
                    title: "Buffered hardware event".to_owned(),
                    body: "Should replay on connect".to_owned(),
                    priority: NotificationPriority::High,
                    created_at_epoch_ms: 2_000,
                });
            }
            let filter = Arc::new(RwLock::new(NotificationFilter {
                min_priority: NotificationPriority::Normal,
                muted_origins: vec!["MutedExample".to_owned()],
            }));
            let approvals = Arc::new(RwLock::new(ApprovalRegistry::new()));
            {
                let mut reg = approvals.write().await;
                reg.register(ApprovalRequest {
                    request_id: "req-expired".to_owned(),
                    application: "ExampleApp".to_owned(),
                    title: "Old".to_owned(),
                    description: "".to_owned(),
                    expires_at_epoch_ms: 5_000,
                    risk_level: RiskLevel::Low,
                    allowed_actions: vec!["approve".to_owned()],
                    status: ApprovalStatus::Pending,
                })
                .expect("register expired");
                reg.register(ApprovalRequest {
                    request_id: "req-active".to_owned(),
                    application: "ExampleApp".to_owned(),
                    title: "Live".to_owned(),
                    description: "".to_owned(),
                    expires_at_epoch_ms: 20_000,
                    risk_level: RiskLevel::High,
                    allowed_actions: vec!["approve".to_owned(), "deny".to_owned()],
                    status: ApprovalStatus::Pending,
                })
                .expect("register active");
            }

            let mut seq = 10_u64;
            let envelopes = build_connect_backlog_envelopes(
                &latest_snapshot,
                &notifications,
                &filter,
                &approvals,
                &mut seq,
                10_000,
            )
            .await;

            // Expected: 1 Telemetry + 1 matching Notification + 1 active Pending Approval = 3 frames.
            assert_eq!(envelopes.len(), 3);
            assert_eq!(envelopes[0].kind, MessageKind::Telemetry);
            assert_eq!(envelopes[1].kind, MessageKind::Notification);
            let notif: NotificationEvent =
                serde_json::from_slice(&envelopes[1].payload).expect("decode notif");
            assert_eq!(notif.id, 2);
            assert_eq!(envelopes[2].kind, MessageKind::Approval);
            let req: ApprovalRequest =
                serde_json::from_slice(&envelopes[2].payload).expect("decode approval");
            assert_eq!(req.request_id, "req-active");
            assert_eq!(seq, 13);
        });
    }
}
