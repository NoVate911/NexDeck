//! Universal API contracts, client identity policies, and input validation.
//!
//! External applications authenticate via [`ClientPolicy`], hold scoped
//! [`ApiPermission`] grants, and submit validated requests (`SendNotification`,
//! `RequestApproval`, `ReadAllowedMetrics`) without unrestricted OS access.

use std::collections::HashSet;
use std::fmt;

use nexdeck_approval::{ApprovalError, ApprovalRegistry, ApprovalRequest, RiskLevel};
use nexdeck_notifications::{NotificationEvent, NotificationPriority, NotificationSource};

/// Maximum allowed timeout in seconds for an API approval request (`1 hour`).
pub const MAX_APPROVAL_TIMEOUT_SECS: u64 = 3_600;

/// Granular permissions granted to an external Universal API client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiPermission {
    /// Read authorized hardware metrics.
    ReadMetrics,
    /// Publish notification events into the Notification Center.
    SendNotification,
    /// Submit interactive remote approval prompts to paired devices.
    RequestApproval,
    /// Interact with authorized custom plugins.
    InteractPlugins,
    /// Invoke explicitly allowlisted remote control actions.
    ExecuteAllowedAction,
}

/// Registered external application identity and permission policy.
#[derive(Debug, Clone)]
pub struct ClientPolicy {
    /// Unique identifier assigned to the client application.
    pub client_id: String,
    /// Human-readable application name.
    pub application_name: String,
    /// Whether the client access is currently active or revoked.
    pub revoked: bool,
    /// Granted permissions (deny-by-default).
    pub permissions: HashSet<ApiPermission>,
}

impl ClientPolicy {
    /// Creates an active client policy with the supplied permissions.
    #[must_use]
    pub fn new(
        client_id: impl Into<String>,
        application_name: impl Into<String>,
        permissions: impl IntoIterator<Item = ApiPermission>,
    ) -> Self {
        Self {
            client_id: client_id.into(),
            application_name: application_name.into(),
            revoked: false,
            permissions: permissions.into_iter().collect(),
        }
    }

    /// Checks whether the client is active and holds `required`.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::ClientRevoked`] or [`ApiError::PermissionDenied`].
    pub fn authorize(&self, required: ApiPermission) -> Result<(), ApiError> {
        if self.revoked {
            return Err(ApiError::ClientRevoked(self.client_id.clone()));
        }
        if !self.permissions.contains(&required) {
            return Err(ApiError::PermissionDenied(required));
        }
        Ok(())
    }
}

/// Universal API payload for creating a remote approval request (`TZ.md` §7.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequestPayload {
    /// Single-use unique request ID protecting against replay attacks.
    pub request_id: String,
    /// Source application name.
    pub application: String,
    /// Prompt heading.
    pub title: String,
    /// Prompt description.
    pub description: String,
    /// Timeout in seconds (`1..=3600`).
    pub timeout_secs: u64,
    /// Acceptable action strings (`["approve", "deny"]`, etc.).
    pub actions: Vec<String>,
    /// Risk level displayed to the user.
    pub risk_level: RiskLevel,
}

/// Errors produced during Universal API validation or dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// Client access token or identity was revoked.
    ClientRevoked(String),
    /// Client lacks the required capability.
    PermissionDenied(ApiPermission),
    /// Request failed input validation.
    ValidationFailed(&'static str),
    /// Downstream approval registry error.
    Approval(ApprovalError),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClientRevoked(id) => write!(f, "client access revoked: {id}"),
            Self::PermissionDenied(perm) => write!(f, "missing required API permission: {perm:?}"),
            Self::ValidationFailed(msg) => write!(f, "API validation failed: {msg}"),
            Self::Approval(err) => write!(f, "approval error: {err}"),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<ApprovalError> for ApiError {
    fn from(value: ApprovalError) -> Self {
        Self::Approval(value)
    }
}

/// Validates and registers an external [`ApprovalRequestPayload`] on behalf of `client`.
///
/// # Errors
///
/// Fails if the client is revoked, lacks [`ApiPermission::RequestApproval`],
/// submits an invalid timeout, or collides with an existing single-use ID.
pub fn submit_approval_request(
    client: &ClientPolicy,
    registry: &mut ApprovalRegistry,
    payload: ApprovalRequestPayload,
    now_epoch_ms: u64,
) -> Result<(), ApiError> {
    client.authorize(ApiPermission::RequestApproval)?;
    if payload.timeout_secs == 0 || payload.timeout_secs > MAX_APPROVAL_TIMEOUT_SECS {
        return Err(ApiError::ValidationFailed(
            "timeout_secs must be between 1 and 3600",
        ));
    }
    let expires_at_epoch_ms = now_epoch_ms.saturating_add(payload.timeout_secs * 1_000);
    let request = ApprovalRequest {
        request_id: payload.request_id,
        application: payload.application,
        title: payload.title,
        description: payload.description,
        expires_at_epoch_ms,
        risk_level: payload.risk_level,
        allowed_actions: payload.actions,
        status: nexdeck_approval::ApprovalStatus::Pending,
    };
    registry.register(request)?;
    Ok(())
}

/// Converts an authorized API notification input into a [`NotificationEvent`].
///
/// # Errors
///
/// Returns [`ApiError::ClientRevoked`], [`ApiError::PermissionDenied`], or
/// [`ApiError::ValidationFailed`] if `title` is empty.
pub fn build_notification_event(
    client: &ClientPolicy,
    id: u64,
    title: String,
    body: String,
    priority: NotificationPriority,
    now_epoch_ms: u64,
) -> Result<NotificationEvent, ApiError> {
    client.authorize(ApiPermission::SendNotification)?;
    if title.trim().is_empty() {
        return Err(ApiError::ValidationFailed("title must not be empty"));
    }
    Ok(NotificationEvent {
        id,
        source: NotificationSource::Application(client.application_name.clone()),
        title,
        body,
        priority,
        created_at_epoch_ms: now_epoch_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_id(seq: u64) -> String {
        format!("api-req-{seq}")
    }

    #[test]
    fn unauthorized_or_revoked_client_is_blocked() {
        let mut reg = ApprovalRegistry::new();
        let mut policy = ClientPolicy::new("app-1", "MyApp", [ApiPermission::ReadMetrics]);

        let payload = ApprovalRequestPayload {
            request_id: test_id(1),
            application: "MyApp".to_owned(),
            title: "Confirm".to_owned(),
            description: "Proceed?".to_owned(),
            timeout_secs: 60,
            actions: vec!["approve".to_owned(), "deny".to_owned()],
            risk_level: RiskLevel::Low,
        };

        assert_eq!(
            submit_approval_request(&policy, &mut reg, payload.clone(), 1_000),
            Err(ApiError::PermissionDenied(ApiPermission::RequestApproval))
        );

        policy.permissions.insert(ApiPermission::RequestApproval);
        policy.revoked = true;
        assert_eq!(
            submit_approval_request(&policy, &mut reg, payload, 1_000),
            Err(ApiError::ClientRevoked("app-1".to_owned()))
        );
    }

    #[test]
    fn valid_approval_payload_is_registered() {
        let mut reg = ApprovalRegistry::new();
        let policy = ClientPolicy::new("app-2", "MyApp", [ApiPermission::RequestApproval]);
        let req_id = test_id(2);

        let payload = ApprovalRequestPayload {
            request_id: req_id.clone(),
            application: "MyApp".to_owned(),
            title: "Confirm".to_owned(),
            description: "Allow execution?".to_owned(),
            timeout_secs: 60,
            actions: vec!["approve".to_owned(), "deny".to_owned()],
            risk_level: RiskLevel::Medium,
        };

        assert_eq!(
            submit_approval_request(&policy, &mut reg, payload, 10_000),
            Ok(())
        );
        let stored = reg.get(&req_id);
        assert_eq!(stored.map(|r| r.expires_at_epoch_ms), Some(70_000));
    }
}
