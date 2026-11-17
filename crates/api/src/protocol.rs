//! Wire DTO contracts for Universal API HTTP and WebSocket endpoints (`/api/v1/*`).

use nexdeck_approval::{ApprovalRequest, RiskLevel};
use nexdeck_notifications::{NotificationEvent, NotificationPriority};
use nexdeck_transport::control::ControlAction;
use serde::{Deserialize, Serialize};

use crate::ApiPermission;

/// Response returned by `GET /api/v1/status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiStatusResponse {
    /// Service readiness string (`"ok"`).
    pub status: String,
    /// Supported protocol version (`1`).
    pub protocol_version: u16,
    /// Authenticated client identifier.
    pub client_id: String,
    /// Authenticated application display name.
    pub application_name: String,
    /// Active permissions granted to this client.
    pub permissions: Vec<ApiPermission>,
}

/// Request payload submitted to `POST /api/v1/notifications`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateNotificationRequest {
    /// Notification heading (`1..=120` chars).
    pub title: String,
    /// Detailed notification message (`<= 1000` chars).
    #[serde(default)]
    pub body: String,
    /// Urgency level (defaults to [`NotificationPriority::Normal`]).
    #[serde(default)]
    pub priority: NotificationPriority,
}

/// Response returned by `POST /api/v1/notifications`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateNotificationResponse {
    /// Created and queued notification event.
    pub event: NotificationEvent,
    /// Current number of events buffered in the agent queue.
    pub queued_count: usize,
}

/// Response returned by `GET /api/v1/notifications`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationListResponse {
    /// Buffered notification events in FIFO order.
    pub notifications: Vec<NotificationEvent>,
}

/// Request payload submitted to `POST /api/v1/approvals`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateApprovalRequest {
    /// Single-use unique request identifier (`1..=64` non-empty chars).
    pub request_id: String,
    /// Optional application name (defaults to authenticated `client.application_name`).
    #[serde(default)]
    pub application: Option<String>,
    /// Short prompt title.
    pub title: String,
    /// Detailed explanation of the operation requiring confirmation.
    #[serde(default)]
    pub description: String,
    /// Expiration timeout in seconds (`1..=3600`). Accepts `"timeout_secs"` or `"timeout"`.
    #[serde(alias = "timeout")]
    pub timeout_secs: u64,
    /// Allowed decision action strings (e.g., `["approve", "deny"]`).
    pub actions: Vec<String>,
    /// Risk level surfaced on the approval prompt.
    #[serde(default)]
    pub risk_level: RiskLevel,
}

/// Response returned when creating, querying, or resolving a single [`ApprovalRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalStatusResponse {
    /// Current state of the approval request.
    pub request: ApprovalRequest,
}

/// Response returned by `GET /api/v1/approvals`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalListResponse {
    /// Registered approval requests ordered by `request_id`.
    pub approvals: Vec<ApprovalRequest>,
}

/// Request payload submitted to `POST /api/v1/approvals/{request_id}/resolve`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveApprovalRequest {
    /// Selected action string from `allowed_actions`.
    pub action: String,
}

/// Request payload submitted to `POST /api/v1/control`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiControlRequest {
    /// Correlation identifier (`1..=64` ASCII non-whitespace chars).
    pub command_id: String,
    /// Strictly allowlisted media/volume control action.
    pub action: ControlAction,
}
