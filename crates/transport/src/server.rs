//! Async HTTP + WebSocket server over TLS (`axum` + `axum-server` + `rustls`).
//!
//! Exposes:
//! - `GET /health` — server readiness, protocol version, and TLS certificate fingerprint;
//! - `POST /pair` — one-time PIN pairing returning [`crate::pairing::PairedCredentials`];
//! - `GET /ws` — authenticated WebSocket transport (`Authorization: Bearer <token>`).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequestParts, Json, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum_server::Handle;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, broadcast, mpsc};

use crate::pairing::{DeviceRegistry, PairingError};
use crate::tls::HostTlsIdentity;
use crate::{Envelope, MessageKind, PROTOCOL_VERSION, TransportError};

const BROADCAST_CAPACITY: usize = 64;

/// JSON payload submitted to `POST /pair`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PairRequest {
    /// One-time 6-digit pairing PIN.
    pub pin: String,
    /// Client-assigned unique device identifier.
    pub device_id: String,
    /// Human-readable device name.
    pub device_name: String,
}

/// JSON response returned by `GET /health`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthResponse {
    /// Service status indicator.
    pub status: String,
    /// Supported protocol version.
    pub protocol_version: u16,
    /// Lowercase SHA-256 hex digest of the host TLS certificate.
    pub fingerprint_sha256: String,
}

/// Validated inbound transport envelope paired with its authenticated source device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundFrame {
    /// Authenticated device identifier.
    pub device_id: String,
    /// Validated protocol envelope.
    pub envelope: Envelope,
}

/// Shared state injected into the Axum router handlers.
#[derive(Clone)]
pub struct ServerState {
    registry: Arc<RwLock<DeviceRegistry>>,
    fingerprint_sha256: String,
    broadcast_tx: broadcast::Sender<String>,
    inbound_tx: mpsc::Sender<InboundFrame>,
}

impl ServerState {
    /// Creates a new [`ServerState`] bound to `registry`, `fingerprint_sha256`,
    /// and an inbound frame channel.
    #[must_use]
    pub fn new(
        registry: Arc<RwLock<DeviceRegistry>>,
        fingerprint_sha256: String,
        inbound_tx: mpsc::Sender<InboundFrame>,
    ) -> Self {
        let (broadcast_tx, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            registry,
            fingerprint_sha256,
            broadcast_tx,
            inbound_tx,
        }
    }

    /// Returns the host TLS certificate SHA-256 fingerprint.
    #[must_use]
    pub fn fingerprint_sha256(&self) -> &str {
        &self.fingerprint_sha256
    }
}

/// Builds the Axum [`Router`] for `/health`, `/pair`, and `/ws`.
pub fn build_router(state: ServerState) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .route("/pair", post(pair_handler))
        .route("/ws", get(ws_handler))
        .with_state(state)
}

/// Running handle for a spawned WSS server instance.
pub struct WssServerHandle {
    local_addr: SocketAddr,
    fingerprint_sha256: String,
    broadcast_tx: broadcast::Sender<String>,
    handle: Handle<SocketAddr>,
    task: tokio::task::JoinHandle<()>,
}

impl WssServerHandle {
    /// Returns the actual bound socket address.
    #[must_use]
    pub const fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Returns the SHA-256 hex fingerprint of the active TLS certificate.
    #[must_use]
    pub fn fingerprint_sha256(&self) -> &str {
        &self.fingerprint_sha256
    }

    /// Serializes, validates, and broadcasts an [`Envelope`] to all connected WebSocket peers.
    ///
    /// Returns the number of active subscribers that received the message (`0` if none).
    ///
    /// # Errors
    ///
    /// Returns [`TransportError`] if the envelope fails validation or serialization.
    pub fn broadcast_envelope(&self, envelope: &Envelope) -> Result<usize, TransportError> {
        envelope.validate()?;
        let json = serde_json::to_string(envelope)
            .map_err(|err| TransportError::ChannelFailure(format!("json encode failed: {err}")))?;
        Ok(self.broadcast_tx.send(json).unwrap_or(0))
    }

    /// Subscribes to serialized outbound envelope JSON frames.
    #[must_use]
    pub fn subscribe_broadcast(&self) -> broadcast::Receiver<String> {
        self.broadcast_tx.subscribe()
    }

    /// Initiates graceful shutdown of the WSS server.
    pub fn shutdown(&self) {
        self.handle.graceful_shutdown(Some(Duration::from_secs(2)));
    }

    /// Requests shutdown and awaits server task completion.
    pub async fn stop(self) {
        self.shutdown();
        let _ = self.task.await;
    }
}

/// Binds and spawns the TLS-secured Axum HTTP/WebSocket server on `bind_addr`.
///
/// # Errors
///
/// Returns [`TransportError::ChannelFailure`] if TLS configuration or socket binding fails.
pub async fn spawn_wss_server(
    bind_addr: SocketAddr,
    tls: &HostTlsIdentity,
    registry: Arc<RwLock<DeviceRegistry>>,
    inbound_tx: mpsc::Sender<InboundFrame>,
) -> Result<WssServerHandle, TransportError> {
    spawn_wss_server_with_routes(bind_addr, tls, registry, inbound_tx, None).await
}

/// Binds and spawns the TLS-secured Axum server on `bind_addr`, merging optional `extra_routes`.
///
/// # Errors
///
/// Returns [`TransportError::ChannelFailure`] if TLS configuration or socket binding fails.
pub async fn spawn_wss_server_with_routes(
    bind_addr: SocketAddr,
    tls: &HostTlsIdentity,
    registry: Arc<RwLock<DeviceRegistry>>,
    inbound_tx: mpsc::Sender<InboundFrame>,
    extra_routes: Option<Router>,
) -> Result<WssServerHandle, TransportError> {
    let rustls_config = tls.to_rustls_config().await?;
    let state = ServerState::new(registry, tls.fingerprint_sha256.clone(), inbound_tx);
    let broadcast_tx = state.broadcast_tx.clone();
    let mut app = build_router(state);
    if let Some(extra) = extra_routes {
        app = app.merge(extra);
    }

    let handle: Handle<SocketAddr> = Handle::new();
    let server_handle = handle.clone();

    let task = tokio::spawn(async move {
        let _ = axum_server::bind_rustls(bind_addr, rustls_config)
            .handle(server_handle)
            .serve(app.into_make_service())
            .await;
    });

    let local_addr = handle.listening().await.ok_or_else(|| {
        TransportError::ChannelFailure(format!("failed to bind WSS server on {bind_addr}"))
    })?;

    Ok(WssServerHandle {
        local_addr,
        fingerprint_sha256: tls.fingerprint_sha256.clone(),
        broadcast_tx,
        handle,
        task,
    })
}

async fn health_handler(State(state): State<ServerState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_owned(),
        protocol_version: PROTOCOL_VERSION,
        fingerprint_sha256: state.fingerprint_sha256,
    })
}

async fn pair_handler(
    State(state): State<ServerState>,
    Json(req): Json<PairRequest>,
) -> impl IntoResponse {
    let now_epoch_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);

    let result = {
        let mut reg = state.registry.write().await;
        reg.complete_pairing(
            &req.pin,
            req.device_id,
            req.device_name,
            state.fingerprint_sha256.clone(),
            now_epoch_ms,
        )
    };

    match result {
        Ok(creds) => (StatusCode::OK, Json(creds)).into_response(),
        Err(PairingError::InvalidPin) => {
            (StatusCode::UNAUTHORIZED, "invalid pairing PIN").into_response()
        }
        Err(PairingError::InvalidInput(msg)) => (StatusCode::BAD_REQUEST, msg).into_response(),
        Err(
            PairingError::NoActiveSession
            | PairingError::SessionExpired
            | PairingError::SessionExhausted,
        ) => (StatusCode::GONE, "pairing session unavailable or expired").into_response(),
    }
}

struct AuthenticatedDevice(String);

impl FromRequestParts<ServerState> for AuthenticatedDevice {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ServerState,
    ) -> Result<Self, Self::Rejection> {
        let Some(token) = extract_bearer_token(&parts.headers) else {
            return Err((StatusCode::UNAUTHORIZED, "missing bearer token"));
        };

        let reg = state.registry.read().await;
        reg.authenticate_token(token)
            .map(|record| Self(record.device_id.clone()))
            .ok_or((StatusCode::UNAUTHORIZED, "invalid or revoked device token"))
    }
}

async fn ws_handler(
    AuthenticatedDevice(device_id): AuthenticatedDevice,
    State(state): State<ServerState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_socket(socket, device_id, state))
        .into_response()
}

/// Extracts a `Bearer <value>` authorization credential from HTTP headers.
#[must_use]
pub fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let trimmed = raw.trim();
    trimmed
        .strip_prefix("Bearer ")
        .or_else(|| trimmed.strip_prefix("bearer "))
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

async fn handle_ws_socket(mut socket: WebSocket, device_id: String, state: ServerState) {
    let mut rx = state.broadcast_tx.subscribe();

    if let Ok(welcome) =
        serde_json::to_string(&Envelope::new(0, MessageKind::Heartbeat, Vec::new()))
        && socket.send(Message::Text(welcome.into())).await.is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            outbound = rx.recv() => {
                match outbound {
                    Ok(json) => {
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            inbound = socket.recv() => {
                match inbound {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(envelope) = serde_json::from_str::<Envelope>(text.as_str())
                            && envelope.validate().is_ok()
                            && state.inbound_tx.send(InboundFrame {
                                device_id: device_id.clone(),
                                envelope,
                            }).await.is_err()
                        {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(Message::Binary(_) | Message::Ping(_) | Message::Pong(_))) => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use tower::ServiceExt;

    use crate::pairing::PairedCredentials;

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(1_000_000, |d| d.as_millis() as u64)
    }

    #[tokio::test]
    async fn health_and_pair_routes_enforce_pin_and_issue_credentials() {
        let tls = HostTlsIdentity::generate_self_signed(&[]).expect("cert generation");
        let registry = Arc::new(RwLock::new(DeviceRegistry::new()));
        let pin = {
            let mut reg = registry.write().await;
            reg.start_pairing(now_ms(), 120)
                .expect("pairing should start")
        };

        let (inbound_tx, _inbound_rx) = mpsc::channel(8);
        let app = build_router(ServerState::new(
            Arc::clone(&registry),
            tls.fingerprint_sha256.clone(),
            inbound_tx,
        ));

        let health_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("health response");
        assert_eq!(health_resp.status(), StatusCode::OK);
        let body = to_bytes(health_resp.into_body(), 4096).await.expect("body");
        let health: HealthResponse = serde_json::from_slice(&body).expect("json");
        assert_eq!(health.fingerprint_sha256, tls.fingerprint_sha256);

        let bad_pin = format!(
            "{:06}",
            (pin.parse::<u32>().unwrap_or(0).wrapping_add(1)) % 1_000_000
        );
        let bad_req = PairRequest {
            pin: bad_pin,
            device_id: "device-example-1".to_owned(),
            device_name: "ExamplePhone".to_owned(),
        };
        let bad_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/pair")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&bad_req).expect("json")))
                    .expect("request"),
            )
            .await
            .expect("pair response");
        assert_eq!(bad_resp.status(), StatusCode::UNAUTHORIZED);

        let good_req = PairRequest {
            pin,
            device_id: "device-example-1".to_owned(),
            device_name: "ExamplePhone".to_owned(),
        };
        let ok_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/pair")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&good_req).expect("json")))
                    .expect("request"),
            )
            .await
            .expect("pair response");
        assert_eq!(ok_resp.status(), StatusCode::OK);
        let ok_bytes = to_bytes(ok_resp.into_body(), 4096).await.expect("body");
        let creds: PairedCredentials = serde_json::from_slice(&ok_bytes).expect("creds");
        assert_eq!(creds.device_id, "device-example-1");
        assert!(!creds.device_token.is_empty());

        let ws_unauth = app
            .oneshot(
                Request::builder()
                    .uri("/ws")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("ws response");
        assert_eq!(ws_unauth.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wss_server_binds_ephemeral_port_and_accepts_tls_connection() {
        let tls = HostTlsIdentity::generate_self_signed(&[]).expect("cert generation");
        let registry = Arc::new(RwLock::new(DeviceRegistry::new()));
        let (inbound_tx, _inbound_rx) = mpsc::channel(8);
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

        let server = spawn_wss_server(addr, &tls, registry, inbound_tx)
            .await
            .expect("WSS server should bind");
        assert_ne!(server.local_addr().port(), 0);
        assert_eq!(server.fingerprint_sha256(), tls.fingerprint_sha256);

        let stream = tokio::net::TcpStream::connect(server.local_addr())
            .await
            .expect("TCP connect to WSS port should succeed");
        drop(stream);

        let env = Envelope::new(1, MessageKind::Heartbeat, Vec::new());
        assert_eq!(server.broadcast_envelope(&env), Ok(0));

        server.stop().await;
    }
}
