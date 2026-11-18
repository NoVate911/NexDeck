//! Remote Approval subsystem state machine for NexDeck.
//!
//! Enforces single-use request identifiers, application binding, allowed action
//! verification, and finite expiration timeouts.
//! Mobile approvals explicitly do not replace Windows User Account Control (UAC).

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Risk classification surfaced on the mobile confirmation prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Routine non-destructive operation.
    Low,
    /// Operation modifies state or user configuration.
    #[default]
    Medium,
    /// Sensitive operation (e.g., process termination, privileged script).
    High,
    /// Irreversible or power-state operation.
    Critical,
}

/// Current lifecycle status of a remote approval request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "selected_action")]
pub enum ApprovalStatus {
    /// Waiting for user input prior to expiration.
    Pending,
    /// User selected one of the allowed actions (`approve`, `deny`, etc.).
    Resolved(String),
    /// Timeout elapsed before a decision arrived.
    Expired,
    /// Request was cancelled by the originating application or administrator.
    Revoked,
}

/// Registered approval request stored until resolved or archived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Single-use unique identifier preventing replay attacks.
    pub request_id: String,
    /// Identifier of the originating application.
    pub application: String,
    /// Short prompt title.
    pub title: String,
    /// Human-readable description of the operation to confirm.
    pub description: String,
    /// Expiration timestamp in Unix epoch milliseconds.
    pub expires_at_epoch_ms: u64,
    /// Risk indicator shown to the user.
    pub risk_level: RiskLevel,
    /// Non-empty set of acceptable decision strings (e.g., `approve`, `deny`).
    pub allowed_actions: Vec<String>,
    /// Current status of the request.
    pub status: ApprovalStatus,
}

/// Wire decision payload sent by an authenticated mobile client over WSS (`MessageKind::Approval`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalDecisionMessage {
    /// Target single-use approval request identifier.
    pub request_id: String,
    /// Chosen action string from [`ApprovalRequest::allowed_actions`].
    pub action: String,
}

/// Errors produced when registering or resolving remote approvals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalError {
    /// A request with this single-use ID has already been registered (replay attempt).
    DuplicateRequestId(String),
    /// No registered request matches the provided ID.
    UnknownRequestId(String),
    /// Request metadata failed validation.
    InvalidRequest(&'static str),
    /// Decision submitted after `expires_at_epoch_ms`.
    RequestExpired,
    /// Request was already resolved or revoked and cannot accept another decision.
    AlreadyFinalized,
    /// Chosen action was not declared in `allowed_actions`.
    DisallowedAction(String),
}

impl fmt::Display for ApprovalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateRequestId(id) => write!(f, "duplicate approval request_id: {id}"),
            Self::UnknownRequestId(id) => write!(f, "unknown approval request_id: {id}"),
            Self::InvalidRequest(msg) => write!(f, "invalid approval request: {msg}"),
            Self::RequestExpired => write!(f, "approval request has expired"),
            Self::AlreadyFinalized => write!(f, "approval request is already finalized"),
            Self::DisallowedAction(act) => write!(f, "disallowed approval action: {act}"),
        }
    }
}

impl std::error::Error for ApprovalError {}

/// In-memory registry tracking active and historical approval requests.
#[derive(Debug, Default)]
pub struct ApprovalRegistry {
    requests: HashMap<String, ApprovalRequest>,
}

impl ApprovalRegistry {
    /// Creates an empty [`ApprovalRegistry`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reconstructs an [`ApprovalRegistry`] from persisted or pre-seeded records.
    #[must_use]
    pub fn from_records(records: impl IntoIterator<Item = ApprovalRequest>) -> Self {
        let mut requests = HashMap::new();
        for req in records {
            if !req.request_id.trim().is_empty() {
                requests.insert(req.request_id.clone(), req);
            }
        }
        Self { requests }
    }

    /// Registers a new [`ApprovalRequest`] after validating invariants and ID uniqueness.
    ///
    /// # Errors
    ///
    /// Returns [`ApprovalError::InvalidRequest`] if required fields are empty or
    /// [`ApprovalError::DuplicateRequestId`] if the ID already exists.
    pub fn register(&mut self, mut request: ApprovalRequest) -> Result<(), ApprovalError> {
        if request.request_id.trim().is_empty() {
            return Err(ApprovalError::InvalidRequest(
                "request_id must not be empty",
            ));
        }
        if request.application.trim().is_empty() {
            return Err(ApprovalError::InvalidRequest(
                "application must not be empty",
            ));
        }
        if request.title.trim().is_empty() {
            return Err(ApprovalError::InvalidRequest("title must not be empty"));
        }
        if request.allowed_actions.is_empty() {
            return Err(ApprovalError::InvalidRequest(
                "allowed_actions must contain at least one action",
            ));
        }
        if self.requests.contains_key(&request.request_id) {
            return Err(ApprovalError::DuplicateRequestId(request.request_id));
        }
        request.status = ApprovalStatus::Pending;
        self.requests.insert(request.request_id.clone(), request);
        Ok(())
    }

    /// Records a one-time decision for an active request if within timeout and allowed actions.
    ///
    /// # Errors
    ///
    /// Rejects unknown IDs, expired requests, repeated decisions (replay),
    /// or actions outside `allowed_actions`.
    pub fn resolve(
        &mut self,
        request_id: &str,
        action: &str,
        now_epoch_ms: u64,
    ) -> Result<&ApprovalRequest, ApprovalError> {
        let req = self
            .requests
            .get_mut(request_id)
            .ok_or_else(|| ApprovalError::UnknownRequestId(request_id.to_owned()))?;

        if req.status != ApprovalStatus::Pending {
            return Err(ApprovalError::AlreadyFinalized);
        }

        if now_epoch_ms > req.expires_at_epoch_ms {
            req.status = ApprovalStatus::Expired;
            return Err(ApprovalError::RequestExpired);
        }

        if !req.allowed_actions.iter().any(|allowed| allowed == action) {
            return Err(ApprovalError::DisallowedAction(action.to_owned()));
        }

        req.status = ApprovalStatus::Resolved(action.to_owned());
        Ok(req)
    }

    /// Revokes a pending request so it can no longer be approved.
    ///
    /// # Errors
    ///
    /// Returns [`ApprovalError::UnknownRequestId`] or [`ApprovalError::AlreadyFinalized`].
    pub fn revoke(&mut self, request_id: &str) -> Result<(), ApprovalError> {
        let req = self
            .requests
            .get_mut(request_id)
            .ok_or_else(|| ApprovalError::UnknownRequestId(request_id.to_owned()))?;

        if req.status != ApprovalStatus::Pending {
            return Err(ApprovalError::AlreadyFinalized);
        }

        req.status = ApprovalStatus::Revoked;
        Ok(())
    }

    /// Retrieves a stored approval request by single-use ID for audit inspection.
    #[must_use]
    pub fn get(&self, request_id: &str) -> Option<&ApprovalRequest> {
        self.requests.get(request_id)
    }

    /// Returns all stored approval requests ordered deterministically by `request_id`.
    #[must_use]
    pub fn list_all(&self) -> Vec<ApprovalRequest> {
        let mut items: Vec<ApprovalRequest> = self.requests.values().cloned().collect();
        items.sort_by(|left, right| left.request_id.cmp(&right.request_id));
        items
    }

    /// Transitions all timed-out [`ApprovalStatus::Pending`] requests to [`ApprovalStatus::Expired`],
    /// returning the newly expired entries ordered by `request_id`.
    pub fn expire_stale(&mut self, now_epoch_ms: u64) -> Vec<ApprovalRequest> {
        let mut expired = Vec::new();
        for req in self.requests.values_mut() {
            if req.status == ApprovalStatus::Pending && now_epoch_ms > req.expires_at_epoch_ms {
                req.status = ApprovalStatus::Expired;
                expired.push(req.clone());
            }
        }
        expired.sort_by(|left, right| left.request_id.cmp(&right.request_id));
        expired
    }

    /// Sweeps expired requests and returns active [`ApprovalStatus::Pending`] requests ordered by
    /// expiration timestamp then `request_id`.
    pub fn pending_active(&mut self, now_epoch_ms: u64) -> Vec<ApprovalRequest> {
        let _ = self.expire_stale(now_epoch_ms);
        let mut pending: Vec<ApprovalRequest> = self
            .requests
            .values()
            .filter(|req| req.status == ApprovalStatus::Pending)
            .cloned()
            .collect();
        pending.sort_by(|left, right| {
            left.expires_at_epoch_ms
                .cmp(&right.expires_at_epoch_ms)
                .then_with(|| left.request_id.cmp(&right.request_id))
        });
        pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_id(seq: u64) -> String {
        format!("req-{seq}")
    }

    fn sample_request(request_id: String, expires_at_epoch_ms: u64) -> ApprovalRequest {
        ApprovalRequest {
            request_id,
            application: "ExampleApp".to_owned(),
            title: "Confirm Action".to_owned(),
            description: "Allow execution?".to_owned(),
            expires_at_epoch_ms,
            risk_level: RiskLevel::Medium,
            allowed_actions: vec!["approve".to_owned(), "deny".to_owned()],
            status: ApprovalStatus::Pending,
        }
    }

    #[test]
    fn resolves_valid_request_once_and_blocks_replay() {
        let mut reg = ApprovalRegistry::new();
        let id = test_id(1);
        assert_eq!(reg.register(sample_request(id.clone(), 2_000)), Ok(()));

        let resolved = reg.resolve(&id, "approve", 1_500);
        assert!(resolved.is_ok());
        assert_eq!(
            reg.get(&id).map(|r| &r.status),
            Some(&ApprovalStatus::Resolved("approve".to_owned()))
        );

        assert_eq!(
            reg.resolve(&id, "deny", 1_600),
            Err(ApprovalError::AlreadyFinalized)
        );
    }

    #[test]
    fn rejects_duplicate_request_id_registration() {
        let mut reg = ApprovalRegistry::new();
        let id = test_id(2);
        assert_eq!(reg.register(sample_request(id.clone(), 2_000)), Ok(()));
        assert_eq!(
            reg.register(sample_request(id.clone(), 3_000)),
            Err(ApprovalError::DuplicateRequestId(id))
        );
    }

    #[test]
    fn rejects_expired_and_disallowed_actions() {
        let mut reg = ApprovalRegistry::new();
        let exp_id = test_id(3);
        assert_eq!(reg.register(sample_request(exp_id.clone(), 1_000)), Ok(()));
        assert_eq!(
            reg.resolve(&exp_id, "approve", 1_001),
            Err(ApprovalError::RequestExpired)
        );

        let act_id = test_id(4);
        assert_eq!(reg.register(sample_request(act_id.clone(), 5_000)), Ok(()));
        assert_eq!(
            reg.resolve(&act_id, "escalate", 2_000),
            Err(ApprovalError::DisallowedAction("escalate".to_owned()))
        );
    }

    #[test]
    fn sweeps_expired_requests_and_lists_pending_active() {
        let mut reg = ApprovalRegistry::new();
        let id_old = test_id(10);
        let id_live = test_id(11);
        assert_eq!(reg.register(sample_request(id_old.clone(), 1_000)), Ok(()));
        assert_eq!(reg.register(sample_request(id_live.clone(), 5_000)), Ok(()));

        let expired = reg.expire_stale(1_500);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].request_id, id_old);
        assert_eq!(expired[0].status, ApprovalStatus::Expired);

        let pending = reg.pending_active(1_500);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].request_id, id_live);

        // Subsequent sweep is a no-op until id_live expires.
        assert!(reg.expire_stale(1_500).is_empty());
    }
}
