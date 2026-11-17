//! Axum HTTP and WebSocket routes for the Universal API (`/api/v1/*`).
//!
//! Enforces constant-time bearer authentication via [`ClientRegistry`] and
//! fine-grained [`ApiPermission`] authorization (`deny-by-default`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequestParts, Json, Path, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use nexdeck_api::clients::ClientRegistry;
use nexdeck_api::protocol::{
    ApiControlRequest, ApiStatusResponse, ApprovalListResponse, ApprovalStatusResponse,
    CreateApprovalRequest, CreateNotificationRequest, CreateNotificationResponse,
    NotificationListResponse, ResolveApprovalRequest,
};
use nexdeck_api::{
    ApiError, ApiPermission, ApprovalRequestPayload, ClientPolicy, build_notification_event,
    submit_approval_request,
};
use nexdeck_approval::{ApprovalError, ApprovalRegistry, ApprovalStatus};
use nexdeck_monitoring::SystemSnapshot;
use nexdeck_notifications::NotificationQueue;
use nexdeck_transport::control::{ControlCommandRequest, ControlCommandResponse};
use nexdeck_transport::server::extract_bearer_token;
use nexdeck_transport::{Envelope, MessageKind, PROTOCOL_VERSION};
use tokio::sync::{RwLock, broadcast, mpsc};

use crate::control::ControlExecutor;

/// Shared state backing all `/api/v1/*` handlers.
#[derive(Clone)]
pub struct ApiRouterState {
    /// Registered external application credentials and permissions.
    pub clients: Arc<RwLock<ClientRegistry>>,
    /// Latest collected hardware telemetry snapshot.
    pub latest_snapshot: Arc<RwLock<Option<SystemSnapshot>>>,
    /// Active and historical remote approval requests.
    pub approvals: Arc<RwLock<ApprovalRegistry>>,
    /// Bounded notification queue.
    pub notifications: Arc<RwLock<NotificationQueue>>,
    /// Host media/volume command executor.
    pub executor: Arc<Mutex<Box<dyn ControlExecutor + Send>>>,
    /// Channel forwarding API-originated envelopes to the main WSS broadcast loop.
    pub outbound_tx: mpsc::Sender<Envelope>,
    /// Broadcast channel for streaming real-time frames to `/api/v1/ws` subscribers.
    pub broadcast_tx: broadcast::Sender<String>,
    next_notification_id: Arc<AtomicU64>,
    next_envelope_id: Arc<AtomicU64>,
}

impl ApiRouterState {
    /// Constructs a new [`ApiRouterState`].
    #[must_use]
    pub fn new(
        clients: Arc<RwLock<ClientRegistry>>,
        latest_snapshot: Arc<RwLock<Option<SystemSnapshot>>>,
        approvals: Arc<RwLock<ApprovalRegistry>>,
        notifications: Arc<RwLock<NotificationQueue>>,
        executor: Arc<Mutex<Box<dyn ControlExecutor + Send>>>,
        outbound_tx: mpsc::Sender<Envelope>,
        broadcast_tx: broadcast::Sender<String>,
    ) -> Self {
        Self {
            clients,
            latest_snapshot,
            approvals,
            notifications,
            executor,
            outbound_tx,
            broadcast_tx,
            next_notification_id: Arc::new(AtomicU64::new(1)),
            next_envelope_id: Arc::new(AtomicU64::new(10_000)),
        }
    }

    fn allocate_notification_id(&self) -> u64 {
        self.next_notification_id.fetch_add(1, Ordering::Relaxed)
    }

    fn allocate_envelope_id(&self) -> u64 {
        self.next_envelope_id.fetch_add(1, Ordering::Relaxed)
    }
}

/// Builds the Axum [`Router`] serving `/api/v1/*` routes.
pub fn build_api_router(state: ApiRouterState) -> Router {
    Router::new()
        .route("/api/v1/status", get(status_handler))
        .route("/api/v1/metrics", get(metrics_handler))
        .route(
            "/api/v1/notifications",
            get(list_notifications_handler).post(create_notification_handler),
        )
        .route(
            "/api/v1/approvals",
            get(list_approvals_handler).post(create_approval_handler),
        )
        .route("/api/v1/approvals/{request_id}", get(get_approval_handler))
        .route(
            "/api/v1/approvals/{request_id}/resolve",
            post(resolve_approval_handler),
        )
        .route(
            "/api/v1/approvals/{request_id}/revoke",
            post(revoke_approval_handler),
        )
        .route("/api/v1/control", post(control_handler))
        .route("/api/v1/ws", get(api_ws_handler))
        .with_state(state)
}

struct AuthenticatedClient(ClientPolicy);

impl FromRequestParts<ApiRouterState> for AuthenticatedClient {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ApiRouterState,
    ) -> Result<Self, Self::Rejection> {
        let Some(secret) = extract_bearer_token(&parts.headers) else {
            return Err((StatusCode::UNAUTHORIZED, "missing bearer token").into_response());
        };

        let clients = state.clients.read().await;
        match clients.authenticate(secret) {
            Ok(policy) => Ok(Self(policy)),
            Err(ApiError::ClientRevoked(_)) => {
                Err((StatusCode::FORBIDDEN, "client access has been revoked").into_response())
            }
            Err(_) => Err((StatusCode::UNAUTHORIZED, "invalid client token").into_response()),
        }
    }
}

async fn status_handler(
    AuthenticatedClient(client): AuthenticatedClient,
) -> Json<ApiStatusResponse> {
    let mut permissions: Vec<ApiPermission> = client.permissions.into_iter().collect();
    permissions.sort();
    Json(ApiStatusResponse {
        status: "ok".to_owned(),
        protocol_version: PROTOCOL_VERSION,
        client_id: client.client_id,
        application_name: client.application_name,
        permissions,
    })
}

async fn metrics_handler(
    AuthenticatedClient(client): AuthenticatedClient,
    State(state): State<ApiRouterState>,
) -> Response {
    if let Err(err) = client.authorize(ApiPermission::ReadMetrics) {
        return map_api_error(&err);
    }

    let snapshot = state.latest_snapshot.read().await.clone();
    match snapshot {
        Some(data) => (StatusCode::OK, Json(data)).into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "telemetry snapshot not yet available",
        )
            .into_response(),
    }
}

async fn list_notifications_handler(
    AuthenticatedClient(client): AuthenticatedClient,
    State(state): State<ApiRouterState>,
) -> Response {
    if client.authorize(ApiPermission::SendNotification).is_err()
        && client.authorize(ApiPermission::ReadMetrics).is_err()
    {
        return (
            StatusCode::FORBIDDEN,
            "requires SendNotification or ReadMetrics permission",
        )
            .into_response();
    }

    let items = state.notifications.read().await.items();
    (
        StatusCode::OK,
        Json(NotificationListResponse {
            notifications: items,
        }),
    )
        .into_response()
}

async fn create_notification_handler(
    AuthenticatedClient(client): AuthenticatedClient,
    State(state): State<ApiRouterState>,
    Json(req): Json<CreateNotificationRequest>,
) -> Response {
    let event_id = state.allocate_notification_id();
    let event = match build_notification_event(
        &client,
        event_id,
        req.title,
        req.body,
        req.priority,
        now_epoch_ms(),
    ) {
        Ok(event) => event,
        Err(err) => return map_api_error(&err),
    };

    let queued_count = {
        let mut queue = state.notifications.write().await;
        queue.push(event.clone());
        queue.len()
    };

    if let Ok(payload) = serde_json::to_vec(&event) {
        let env = Envelope::new(
            state.allocate_envelope_id(),
            MessageKind::Notification,
            payload,
        );
        let _ = state.outbound_tx.send(env).await;
    }

    (
        StatusCode::CREATED,
        Json(CreateNotificationResponse {
            event,
            queued_count,
        }),
    )
        .into_response()
}

async fn list_approvals_handler(
    AuthenticatedClient(client): AuthenticatedClient,
    State(state): State<ApiRouterState>,
) -> Response {
    if let Err(err) = client.authorize(ApiPermission::RequestApproval) {
        return map_api_error(&err);
    }

    let now = now_epoch_ms();
    let all = state.approvals.read().await.list_all();
    let approvals = all
        .into_iter()
        .filter(|item| item.application == client.application_name)
        .map(|mut item| {
            if item.status == ApprovalStatus::Pending && now > item.expires_at_epoch_ms {
                item.status = ApprovalStatus::Expired;
            }
            item
        })
        .collect();

    (StatusCode::OK, Json(ApprovalListResponse { approvals })).into_response()
}

async fn create_approval_handler(
    AuthenticatedClient(client): AuthenticatedClient,
    State(state): State<ApiRouterState>,
    Json(req): Json<CreateApprovalRequest>,
) -> Response {
    let app_name = match req.application {
        Some(provided) if !provided.trim().is_empty() => provided,
        _ => client.application_name.clone(),
    };

    let payload = ApprovalRequestPayload {
        request_id: req.request_id.clone(),
        application: app_name,
        title: req.title,
        description: req.description,
        timeout_secs: req.timeout_secs,
        actions: req.actions,
        risk_level: req.risk_level,
    };

    let now = now_epoch_ms();
    let registered = {
        let mut registry = state.approvals.write().await;
        if let Err(err) = submit_approval_request(&client, &mut registry, payload, now) {
            return map_api_error(&err);
        }
        registry.get(&req.request_id).cloned()
    };

    let Some(approval_req) = registered else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to retrieve registered approval request",
        )
            .into_response();
    };

    if let Ok(bytes) = serde_json::to_vec(&approval_req) {
        let env = Envelope::new(state.allocate_envelope_id(), MessageKind::Approval, bytes);
        let _ = state.outbound_tx.send(env).await;
    }

    (
        StatusCode::CREATED,
        Json(ApprovalStatusResponse {
            request: approval_req,
        }),
    )
        .into_response()
}

async fn get_approval_handler(
    AuthenticatedClient(client): AuthenticatedClient,
    State(state): State<ApiRouterState>,
    Path(request_id): Path<String>,
) -> Response {
    if let Err(err) = client.authorize(ApiPermission::RequestApproval) {
        return map_api_error(&err);
    }

    let now = now_epoch_ms();
    let found = state.approvals.read().await.get(&request_id).cloned();
    let Some(mut approval_req) = found else {
        return (StatusCode::NOT_FOUND, "approval request not found").into_response();
    };
    if approval_req.application != client.application_name {
        return (
            StatusCode::FORBIDDEN,
            "approval request belongs to another application",
        )
            .into_response();
    }
    if approval_req.status == ApprovalStatus::Pending && now > approval_req.expires_at_epoch_ms {
        approval_req.status = ApprovalStatus::Expired;
    }

    (
        StatusCode::OK,
        Json(ApprovalStatusResponse {
            request: approval_req,
        }),
    )
        .into_response()
}

async fn resolve_approval_handler(
    AuthenticatedClient(client): AuthenticatedClient,
    State(state): State<ApiRouterState>,
    Path(request_id): Path<String>,
    Json(req): Json<ResolveApprovalRequest>,
) -> Response {
    if let Err(err) = client.authorize(ApiPermission::RequestApproval) {
        return map_api_error(&err);
    }

    let now = now_epoch_ms();
    let result = {
        let mut registry = state.approvals.write().await;
        registry
            .resolve(&request_id, req.action.trim(), now)
            .cloned()
    };

    match result {
        Ok(updated) => {
            if let Ok(bytes) = serde_json::to_vec(&updated) {
                let env = Envelope::new(state.allocate_envelope_id(), MessageKind::Approval, bytes);
                let _ = state.outbound_tx.send(env).await;
            }
            (
                StatusCode::OK,
                Json(ApprovalStatusResponse { request: updated }),
            )
                .into_response()
        }
        Err(err) => map_approval_error(&err),
    }
}

async fn revoke_approval_handler(
    AuthenticatedClient(client): AuthenticatedClient,
    State(state): State<ApiRouterState>,
    Path(request_id): Path<String>,
) -> Response {
    if let Err(err) = client.authorize(ApiPermission::RequestApproval) {
        return map_api_error(&err);
    }

    let updated = {
        let mut registry = state.approvals.write().await;
        let Some(existing) = registry.get(&request_id) else {
            return (StatusCode::NOT_FOUND, "approval request not found").into_response();
        };
        if existing.application != client.application_name {
            return (
                StatusCode::FORBIDDEN,
                "approval request belongs to another application",
            )
                .into_response();
        }
        if let Err(err) = registry.revoke(&request_id) {
            return map_approval_error(&err);
        }
        registry.get(&request_id).cloned()
    };

    let Some(request) = updated else {
        return (StatusCode::NOT_FOUND, "approval request not found").into_response();
    };

    if let Ok(bytes) = serde_json::to_vec(&request) {
        let env = Envelope::new(state.allocate_envelope_id(), MessageKind::Approval, bytes);
        let _ = state.outbound_tx.send(env).await;
    }

    (StatusCode::OK, Json(ApprovalStatusResponse { request })).into_response()
}

async fn control_handler(
    AuthenticatedClient(client): AuthenticatedClient,
    State(state): State<ApiRouterState>,
    Json(req): Json<ApiControlRequest>,
) -> Response {
    if let Err(err) = client.authorize(ApiPermission::ExecuteAllowedAction) {
        return map_api_error(&err);
    }

    let cmd = ControlCommandRequest {
        command_id: req.command_id,
        action: req.action,
    };
    if let Err(err) = cmd.validate() {
        return (StatusCode::BAD_REQUEST, err.to_string()).into_response();
    }

    let exec_result = state
        .executor
        .lock()
        .map_err(|_| "control executor lock poisoned".to_owned())
        .and_then(|guard| guard.execute(cmd.action));

    let response = match exec_result {
        Ok(message) => ControlCommandResponse {
            command_id: cmd.command_id,
            action: cmd.action,
            ok: true,
            message,
        },
        Err(message) => ControlCommandResponse {
            command_id: cmd.command_id,
            action: cmd.action,
            ok: false,
            message,
        },
    };
    (StatusCode::OK, Json(response)).into_response()
}

async fn api_ws_handler(
    AuthenticatedClient(client): AuthenticatedClient,
    State(state): State<ApiRouterState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_api_ws_socket(socket, client, state))
        .into_response()
}

async fn handle_api_ws_socket(mut socket: WebSocket, client: ClientPolicy, state: ApiRouterState) {
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
                        if should_deliver_frame_to_client(&client, &json)
                            && socket.send(Message::Text(json.into())).await.is_err()
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            inbound = socket.recv() => {
                match inbound {
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(env) = serde_json::from_str::<Envelope>(&text)
                            && env.kind == MessageKind::Heartbeat
                        {
                            let reply = Envelope::new(env.message_id, MessageKind::Heartbeat, Vec::new());
                            if let Ok(reply_json) = serde_json::to_string(&reply)
                                && socket.send(Message::Text(reply_json.into())).await.is_err()
                            {
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Binary(_) | Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                }
            }
        }
    }
}

fn should_deliver_frame_to_client(client: &ClientPolicy, raw_json: &str) -> bool {
    let Ok(envelope) = serde_json::from_str::<Envelope>(raw_json) else {
        return false;
    };
    match envelope.kind {
        MessageKind::Telemetry => client.authorize(ApiPermission::ReadMetrics).is_ok(),
        MessageKind::Notification => {
            client.authorize(ApiPermission::SendNotification).is_ok()
                || client.authorize(ApiPermission::ReadMetrics).is_ok()
        }
        MessageKind::Approval => client.authorize(ApiPermission::RequestApproval).is_ok(),
        MessageKind::ControlCommand => client
            .authorize(ApiPermission::ExecuteAllowedAction)
            .is_ok(),
        MessageKind::Heartbeat => true,
    }
}

fn map_api_error(err: &ApiError) -> Response {
    match err {
        ApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, *msg).into_response(),
        ApiError::ClientRevoked(_) | ApiError::PermissionDenied(_) => {
            (StatusCode::FORBIDDEN, err.to_string()).into_response()
        }
        ApiError::UnknownClient(_) => (StatusCode::NOT_FOUND, err.to_string()).into_response(),
        ApiError::DuplicateClient(_) => (StatusCode::CONFLICT, err.to_string()).into_response(),
        ApiError::ValidationFailed(msg) => (StatusCode::BAD_REQUEST, *msg).into_response(),
        ApiError::Approval(inner) => map_approval_error(inner),
    }
}

fn map_approval_error(err: &ApprovalError) -> Response {
    match err {
        ApprovalError::DuplicateRequestId(_) | ApprovalError::AlreadyFinalized => {
            (StatusCode::CONFLICT, err.to_string()).into_response()
        }
        ApprovalError::UnknownRequestId(_) => {
            (StatusCode::NOT_FOUND, err.to_string()).into_response()
        }
        ApprovalError::RequestExpired => (StatusCode::GONE, err.to_string()).into_response(),
        ApprovalError::InvalidRequest(_) | ApprovalError::DisallowedAction(_) => {
            (StatusCode::BAD_REQUEST, err.to_string()).into_response()
        }
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
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, header};
    use nexdeck_approval::RiskLevel;
    use nexdeck_monitoring::{CpuMetrics, MemoryMetrics};
    use nexdeck_notifications::NotificationPriority;
    use nexdeck_transport::control::ControlAction;
    use tower::ServiceExt;

    use crate::control::RecordingControlExecutor;

    fn sample_snapshot() -> SystemSnapshot {
        SystemSnapshot {
            captured_at_epoch_ms: 100_000,
            uptime_secs: 600,
            cpu: CpuMetrics {
                total_usage_pct: 25.0,
                per_core_usage_pct: vec![20.0, 30.0],
                frequency_mhz: Some(4_000),
                temperature_celsius: None,
            },
            memory: MemoryMetrics {
                total_bytes: 8 * 1024 * 1024 * 1024,
                used_bytes: 2 * 1024 * 1024 * 1024,
            },
            gpus: Vec::new(),
            disks: Vec::new(),
            networks: Vec::new(),
            battery: None,
        }
    }

    #[tokio::test]
    async fn enforces_bearer_auth_permissions_and_routes_notifications_approvals_and_controls() {
        let mut reg = ClientRegistry::new();
        let full_client = reg
            .register_client(
                "org.example.full".to_owned(),
                "ExampleApp".to_owned(),
                [
                    ApiPermission::ReadMetrics,
                    ApiPermission::SendNotification,
                    ApiPermission::RequestApproval,
                    ApiPermission::ExecuteAllowedAction,
                ],
                1_000,
            )
            .expect("register full client");
        let metrics_only = reg
            .register_client(
                "org.example.readonly".to_owned(),
                "ReadOnlyApp".to_owned(),
                [ApiPermission::ReadMetrics],
                1_000,
            )
            .expect("register readonly client");

        let clients = Arc::new(RwLock::new(reg));
        let snapshot = Arc::new(RwLock::new(Some(sample_snapshot())));
        let approvals = Arc::new(RwLock::new(ApprovalRegistry::new()));
        let notifications = Arc::new(RwLock::new(NotificationQueue::new(16)));
        let recorder = RecordingControlExecutor::new();
        let executor: Arc<Mutex<Box<dyn ControlExecutor + Send>>> =
            Arc::new(Mutex::new(Box::new(recorder.clone())));
        let (outbound_tx, mut outbound_rx) = mpsc::channel(16);
        let (broadcast_tx, _) = broadcast::channel(16);

        let app = build_api_router(ApiRouterState::new(
            Arc::clone(&clients),
            snapshot,
            approvals,
            notifications,
            executor,
            outbound_tx,
            broadcast_tx,
        ));

        // 1. Unauthenticated request returns 401.
        let unauth_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/status")
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(unauth_resp.status(), StatusCode::UNAUTHORIZED);

        // 2. Read-only client can read metrics (200) but is forbidden from creating approvals (403).
        let ro_auth = format!("Bearer {}", metrics_only.access_secret);
        let metrics_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/metrics")
                    .header(header::AUTHORIZATION, &ro_auth)
                    .body(Body::empty())
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(metrics_resp.status(), StatusCode::OK);

        let approval_req = CreateApprovalRequest {
            request_id: "req-example-1".to_owned(),
            application: None,
            title: "Подтвердите операцию".to_owned(),
            description: "Разрешить тестовое действие?".to_owned(),
            timeout_secs: 60,
            actions: vec!["approve".to_owned(), "deny".to_owned()],
            risk_level: RiskLevel::Medium,
        };
        let forbidden_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/approvals")
                    .header(header::AUTHORIZATION, &ro_auth)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&approval_req).expect("json")))
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(forbidden_resp.status(), StatusCode::FORBIDDEN);

        // 3. Authorized client submits notification, approval, resolves approval, and invokes control.
        let full_auth = format!("Bearer {}", full_client.access_secret);
        let notif_req = CreateNotificationRequest {
            title: "Сборка завершена".to_owned(),
            body: "Проект ExampleApp собран без ошибок".to_owned(),
            priority: NotificationPriority::Normal,
        };
        let notif_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/notifications")
                    .header(header::AUTHORIZATION, &full_auth)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&notif_req).expect("json")))
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(notif_resp.status(), StatusCode::CREATED);
        let emitted_notif = outbound_rx.recv().await.expect("notification envelope");
        assert_eq!(emitted_notif.kind, MessageKind::Notification);

        let create_app_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/approvals")
                    .header(header::AUTHORIZATION, &full_auth)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(serde_json::to_vec(&approval_req).expect("json")))
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(create_app_resp.status(), StatusCode::CREATED);
        let emitted_app = outbound_rx.recv().await.expect("approval envelope");
        assert_eq!(emitted_app.kind, MessageKind::Approval);

        let resolve_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/approvals/req-example-1/resolve")
                    .header(header::AUTHORIZATION, &full_auth)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&ResolveApprovalRequest {
                            action: "approve".to_owned(),
                        })
                        .expect("json"),
                    ))
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(resolve_resp.status(), StatusCode::OK);
        let resolved_bytes = to_bytes(resolve_resp.into_body(), 4096)
            .await
            .expect("bytes");
        let resolved_dto: ApprovalStatusResponse =
            serde_json::from_slice(&resolved_bytes).expect("decode");
        assert_eq!(
            resolved_dto.request.status,
            ApprovalStatus::Resolved("approve".to_owned())
        );

        // Replay resolve is rejected with 409 Conflict.
        let replay_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/approvals/req-example-1/resolve")
                    .header(header::AUTHORIZATION, &full_auth)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&ResolveApprovalRequest {
                            action: "deny".to_owned(),
                        })
                        .expect("json"),
                    ))
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(replay_resp.status(), StatusCode::CONFLICT);

        // Control endpoint executes allowlisted action.
        let ctrl_resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/control")
                    .header(header::AUTHORIZATION, &full_auth)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&ApiControlRequest {
                            command_id: "cmd-api-1".to_owned(),
                            action: ControlAction::MediaPlayPause,
                        })
                        .expect("json"),
                    ))
                    .expect("req"),
            )
            .await
            .expect("resp");
        assert_eq!(ctrl_resp.status(), StatusCode::OK);
        assert_eq!(recorder.actions(), vec![ControlAction::MediaPlayPause]);
    }
}
