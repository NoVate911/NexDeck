//! Remote Approval subsystem state machine for NexDeck.
//!
//! Enforces single-use request identifiers (nonces), application binding,
//! allowed action verification, and finite expiration timeouts (`TZ.md` §7.2, §8).
//! Mobile approvals explicitly do not replace Windows User Account Control (UAC).

use std::collections::HashMap;
use std::fmt;

/// Risk classification surfaced on the mobile confirmation prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    /// Routine non-destructive operation.
    Low,
    /// Operation modifies state or user configuration.
    Medium,
    /// Sensitive operation (e.g., process termination, privileged script).
    High,
    /// Irreversible or power-state operation.
    Critical,
}

/// Current lifecycle status of a remote approval request.
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequest {
    /// Single-use unique identifier preventing replay attacks.
    pub nonce: String,
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

/// Errors produced when registering or resolving remote approvals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalError {
    /// A request with this nonce has already been registered (replay attempt).
    DuplicateNonce(String),
    /// No registered request matches the provided nonce.
    UnknownNonce(String),
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
            Self::DuplicateNonce(n) => write!(f, "duplicate approval nonce: {n}"),
            Self::UnknownNonce(n) => write!(f, "unknown approval nonce: {n}"),
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

    /// Registers a new [`ApprovalRequest`] after validating invariants and nonce uniqueness.
    ///
    /// # Errors
    ///
    /// Returns [`ApprovalError::InvalidRequest`] if required fields are empty or
    /// [`ApprovalError::DuplicateNonce`] if the nonce already exists.
    pub fn register(&mut self, mut request: ApprovalRequest) -> Result<(), ApprovalError> {
        if request.nonce.trim().is_empty() {
            return Err(ApprovalError::InvalidRequest("nonce must not be empty"));
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
        if self.requests.contains_key(&request.nonce) {
            return Err(ApprovalError::DuplicateNonce(request.nonce));
        }
        request.status = ApprovalStatus::Pending;
        self.requests.insert(request.nonce.clone(), request);
        Ok(())
    }

    /// Records a one-time decision for an active request if within timeout and allowed actions.
    ///
    /// # Errors
    ///
    /// Rejects unknown nonces, expired requests, repeated decisions (replay),
    /// or actions outside `allowed_actions`.
    pub fn resolve(
        &mut self,
        nonce: &str,
        action: &str,
        now_epoch_ms: u64,
    ) -> Result<&ApprovalRequest, ApprovalError> {
        let req = self
            .requests
            .get_mut(nonce)
            .ok_or_else(|| ApprovalError::UnknownNonce(nonce.to_owned()))?;

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
    /// Returns [`ApprovalError::UnknownNonce`] or [`ApprovalError::AlreadyFinalized`].
    pub fn revoke(&mut self, nonce: &str) -> Result<(), ApprovalError> {
        let req = self
            .requests
            .get_mut(nonce)
            .ok_or_else(|| ApprovalError::UnknownNonce(nonce.to_owned()))?;

        if req.status != ApprovalStatus::Pending {
            return Err(ApprovalError::AlreadyFinalized);
        }

        req.status = ApprovalStatus::Revoked;
        Ok(())
    }

    /// Retrieves a stored approval request by nonce for audit inspection.
    #[must_use]
    pub fn get(&self, nonce: &str) -> Option<&ApprovalRequest> {
        self.requests.get(nonce)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request(nonce: &str, expires_at_epoch_ms: u64) -> ApprovalRequest {
        ApprovalRequest {
            nonce: nonce.to_owned(),
            application: "MyApplication".to_owned(),
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
        assert_eq!(reg.register(sample_request("nonce-1", 2_000)), Ok(()));

        let resolved = reg.resolve("nonce-1", "approve", 1_500);
        assert!(resolved.is_ok());
        assert_eq!(
            reg.get("nonce-1").map(|r| &r.status),
            Some(&ApprovalStatus::Resolved("approve".to_owned()))
        );

        assert_eq!(
            reg.resolve("nonce-1", "deny", 1_600),
            Err(ApprovalError::AlreadyFinalized)
        );
    }

    #[test]
    fn rejects_duplicate_nonce_registration() {
        let mut reg = ApprovalRegistry::new();
        assert_eq!(reg.register(sample_request("nonce-dup", 2_000)), Ok(()));
        assert_eq!(
            reg.register(sample_request("nonce-dup", 3_000)),
            Err(ApprovalError::DuplicateNonce("nonce-dup".to_owned()))
        );
    }

    #[test]
    fn rejects_expired_and_disallowed_actions() {
        let mut reg = ApprovalRegistry::new();
        assert_eq!(reg.register(sample_request("nonce-exp", 1_000)), Ok(()));
        assert_eq!(
            reg.resolve("nonce-exp", "approve", 1_001),
            Err(ApprovalError::RequestExpired)
        );

        assert_eq!(reg.register(sample_request("nonce-act", 5_000)), Ok(()));
        assert_eq!(
            reg.resolve("nonce-act", "escalate", 2_000),
            Err(ApprovalError::DisallowedAction("escalate".to_owned()))
        );
    }
}
