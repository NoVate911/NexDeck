//! Device pairing (one-time PIN / QR payload) and hashed token authentication.
//!
//! Raw pairing PINs and issued `device_token` secrets are never stored in plain
//! text; only SHA-256 digests are persisted and verified in constant time.

use std::collections::HashMap;
use std::fmt::Write as _;

use ring::constant_time::verify_slices_are_equal;
use ring::digest::{SHA256, digest};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

use crate::TransportError;

/// Default maximum failed PIN attempts before invalidating a pairing session.
pub const DEFAULT_MAX_PAIRING_ATTEMPTS: u8 = 5;

/// QR code / pairing bootstrap payload shared with a mobile client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingQrPayload {
    /// Protocol version expected by the server.
    pub protocol_version: u16,
    /// Host address or IP where the WSS endpoint is reachable.
    pub host: String,
    /// Listening TCP port for WSS.
    pub port: u16,
    /// SHA-256 hex fingerprint of the host's self-signed TLS certificate.
    pub fingerprint_sha256: String,
    /// One-time 6-digit pairing PIN.
    pub pin: String,
}

/// Persisted metadata for an authorized mobile device (stores only token hash).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRecord {
    /// Unique client-supplied or assigned device identifier.
    pub device_id: String,
    /// Human-readable device label (e.g., `"ExamplePhone"`).
    pub device_name: String,
    /// SHA-256 hex digest of the issued device bearer token.
    pub token_hash_sha256: String,
    /// Unix epoch milliseconds when pairing succeeded.
    pub paired_at_epoch_ms: u64,
}

/// Credentials returned once to a newly paired mobile device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedCredentials {
    /// Confirmed device identifier.
    pub device_id: String,
    /// Newly minted bearer token (returned once, never stored in plain text).
    pub device_token: String,
    /// SHA-256 hex fingerprint of the host TLS certificate for pinning.
    pub fingerprint_sha256: String,
}

/// Errors produced during device pairing or bearer token verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingError {
    /// No active pairing session has been started.
    NoActiveSession,
    /// Active pairing session has expired.
    SessionExpired,
    /// Pairing session was already consumed or locked out after failed attempts.
    SessionExhausted,
    /// Submitted PIN did not match.
    InvalidPin,
    /// Request parameters failed validation.
    InvalidInput(&'static str),
}

#[derive(Debug, Clone)]
struct PairingSession {
    pin_hash_sha256: String,
    expires_at_epoch_ms: u64,
    attempts: u8,
    max_attempts: u8,
    consumed: bool,
}

/// Registry managing pairing sessions and paired device token hashes.
#[derive(Debug, Default)]
pub struct DeviceRegistry {
    active_pairing: Option<PairingSession>,
    devices: HashMap<String, DeviceRecord>,
}

impl DeviceRegistry {
    /// Creates an empty [`DeviceRegistry`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Restores a [`DeviceRegistry`] from previously persisted [`DeviceRecord`] items.
    #[must_use]
    pub fn from_records(records: Vec<DeviceRecord>) -> Self {
        let mut devices = HashMap::with_capacity(records.len());
        for record in records {
            devices.insert(record.device_id.clone(), record);
        }
        Self {
            active_pairing: None,
            devices,
        }
    }

    /// Exports paired device records (containing only token hashes) for persistence.
    #[must_use]
    pub fn export_records(&self) -> Vec<DeviceRecord> {
        let mut list: Vec<DeviceRecord> = self.devices.values().cloned().collect();
        list.sort_by(|a, b| a.device_id.cmp(&b.device_id));
        list
    }

    /// Starts a new one-time PIN pairing session valid for `ttl_secs`.
    ///
    /// Returns the plaintext 6-digit PIN once so it can be displayed to the user
    /// or embedded in a [`PairingQrPayload`]. Only its SHA-256 hash is retained.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError`] if `ttl_secs == 0` or CSPRNG generation fails.
    pub fn start_pairing(
        &mut self,
        now_epoch_ms: u64,
        ttl_secs: u64,
    ) -> Result<String, TransportError> {
        if ttl_secs == 0 {
            return Err(TransportError::ChannelFailure(
                "pairing ttl_secs must be greater than zero".to_owned(),
            ));
        }
        let pin = generate_pairing_pin()?;
        self.active_pairing = Some(PairingSession {
            pin_hash_sha256: sha256_hex(pin.as_bytes()),
            expires_at_epoch_ms: now_epoch_ms.saturating_add(ttl_secs * 1_000),
            attempts: 0,
            max_attempts: DEFAULT_MAX_PAIRING_ATTEMPTS,
            consumed: false,
        });
        Ok(pin)
    }

    /// Completes pairing by verifying `pin`, storing a hashed token, and issuing
    /// [`PairedCredentials`].
    ///
    /// # Errors
    ///
    /// Rejects expired, exhausted, missing sessions, invalid inputs, or mismatched PINs.
    pub fn complete_pairing(
        &mut self,
        pin: &str,
        device_id: String,
        device_name: String,
        fingerprint_sha256: String,
        now_epoch_ms: u64,
    ) -> Result<PairedCredentials, PairingError> {
        if device_id.trim().is_empty() || device_name.trim().is_empty() {
            return Err(PairingError::InvalidInput(
                "device_id and device_name must not be empty",
            ));
        }
        if pin.trim().is_empty() {
            return Err(PairingError::InvalidInput("pin must not be empty"));
        }

        let session = self
            .active_pairing
            .as_mut()
            .ok_or(PairingError::NoActiveSession)?;

        if session.consumed || session.attempts >= session.max_attempts {
            return Err(PairingError::SessionExhausted);
        }

        if now_epoch_ms > session.expires_at_epoch_ms {
            session.consumed = true;
            return Err(PairingError::SessionExpired);
        }

        let candidate_hash = sha256_hex(pin.trim().as_bytes());
        if !constant_time_eq(&session.pin_hash_sha256, &candidate_hash) {
            session.attempts = session.attempts.saturating_add(1);
            if session.attempts >= session.max_attempts {
                session.consumed = true;
            }
            return Err(PairingError::InvalidPin);
        }

        let issued = generate_device_token().map_err(|_| PairingError::SessionExhausted)?;
        let token_hash_sha256 = sha256_hex(issued.as_bytes());
        session.consumed = true;

        self.devices.insert(
            device_id.clone(),
            DeviceRecord {
                device_id: device_id.clone(),
                device_name,
                token_hash_sha256,
                paired_at_epoch_ms: now_epoch_ms,
            },
        );

        Ok(PairedCredentials {
            device_id,
            device_token: issued,
            fingerprint_sha256,
        })
    }

    /// Verifies a presented bearer token against registered device hashes in constant time.
    #[must_use]
    pub fn authenticate_token(&self, presented: &str) -> Option<&DeviceRecord> {
        if presented.is_empty() {
            return None;
        }
        let presented_hash = sha256_hex(presented.as_bytes());
        let mut matched: Option<&DeviceRecord> = None;
        // Iterate all entries to keep lookup timing uniform across devices.
        for record in self.devices.values() {
            if constant_time_eq(&record.token_hash_sha256, &presented_hash) {
                matched = Some(record);
            }
        }
        matched
    }

    /// Revokes a paired device by ID, returning `true` if it existed.
    pub fn revoke_device(&mut self, device_id: &str) -> bool {
        self.devices.remove(device_id).is_some()
    }

    /// Returns the number of currently paired devices.
    #[must_use]
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }
}

/// Computes lowercase hex-encoded SHA-256 digest of `data`.
#[must_use]
pub fn sha256_hex(data: &[u8]) -> String {
    let hash = digest(&SHA256, data);
    bytes_to_hex(hash.as_ref())
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    verify_slices_are_equal(a.as_bytes(), b.as_bytes()).is_ok()
}

fn generate_pairing_pin() -> Result<String, TransportError> {
    let rng = SystemRandom::new();
    let mut buf = [0u8; 4];
    // Rejection sampling over 32-bit unsigned integers eliminates modulo bias for 1_000_000 values.
    let limit = (u32::MAX / 1_000_000) * 1_000_000;
    loop {
        rng.fill(&mut buf)
            .map_err(|_| TransportError::ChannelFailure("CSPRNG failure".to_owned()))?;
        let candidate = u32::from_le_bytes(buf);
        if candidate < limit {
            return Ok(format!("{:06}", candidate % 1_000_000));
        }
    }
}

fn generate_device_token() -> Result<String, TransportError> {
    let rng = SystemRandom::new();
    let mut buf = [0u8; 32];
    rng.fill(&mut buf)
        .map_err(|_| TransportError::ChannelFailure("CSPRNG failure".to_owned()))?;
    Ok(bytes_to_hex(&buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_issues_token_and_authenticates_without_storing_plaintext() {
        let mut reg = DeviceRegistry::new();
        let pin = reg.start_pairing(1_000, 120).expect("pairing should start");
        assert_eq!(pin.len(), 6);
        assert!(pin.chars().all(|c| c.is_ascii_digit()));

        let creds = reg
            .complete_pairing(
                &pin,
                "device-example-1".to_owned(),
                "ExamplePhone".to_owned(),
                sha256_hex(b"cert-der"),
                2_000,
            )
            .expect("valid PIN should complete pairing");

        assert_eq!(reg.device_count(), 1);
        assert_ne!(
            reg.export_records()[0].token_hash_sha256,
            creds.device_token
        );
        assert!(reg.authenticate_token(&creds.device_token).is_some());

        // Single-use pairing session cannot be replayed.
        assert_eq!(
            reg.complete_pairing(
                &pin,
                "device-example-2".to_owned(),
                "OtherPhone".to_owned(),
                sha256_hex(b"cert-der"),
                2_100,
            ),
            Err(PairingError::SessionExhausted)
        );
    }

    #[test]
    fn pairing_lockout_and_revocation_work() {
        let mut reg = DeviceRegistry::new();
        let pin = reg.start_pairing(1_000, 60).expect("pairing should start");
        let wrong_pin = format!(
            "{:06}",
            (pin.parse::<u32>().unwrap_or(0).wrapping_add(1)) % 1_000_000
        );

        for attempt in 1..DEFAULT_MAX_PAIRING_ATTEMPTS {
            let err = reg.complete_pairing(
                wrong_pin,
                "device-example-1".to_owned(),
                "ExamplePhone".to_owned(),
                "fp".to_owned(),
                1_000 + u64::from(attempt),
            );
            assert_eq!(err, Err(PairingError::InvalidPin));
        }

        // 5th invalid attempt triggers exhaustion.
        assert_eq!(
            reg.complete_pairing(
                wrong_pin,
                "device-example-1".to_owned(),
                "ExamplePhone".to_owned(),
                "fp".to_owned(),
                1_100,
            ),
            Err(PairingError::InvalidPin)
        );
        assert_eq!(
            reg.complete_pairing(
                &pin,
                "device-example-1".to_owned(),
                "ExamplePhone".to_owned(),
                "fp".to_owned(),
                1_200,
            ),
            Err(PairingError::SessionExhausted)
        );
    }

    #[test]
    fn expired_session_is_rejected_and_revoked_device_fails_auth() {
        let mut reg = DeviceRegistry::new();
        let pin = reg.start_pairing(1_000, 5).expect("pairing should start");
        assert_eq!(
            reg.complete_pairing(
                &pin,
                "device-example-1".to_owned(),
                "ExamplePhone".to_owned(),
                "fp".to_owned(),
                7_000,
            ),
            Err(PairingError::SessionExpired)
        );

        let pin2 = reg.start_pairing(10_000, 30).expect("pairing should start");
        let creds = reg
            .complete_pairing(
                &pin2,
                "device-example-1".to_owned(),
                "ExamplePhone".to_owned(),
                "fp".to_owned(),
                12_000,
            )
            .expect("valid pairing");
        assert!(reg.revoke_device("device-example-1"));
        assert!(reg.authenticate_token(&creds.device_token).is_none());
    }
}
