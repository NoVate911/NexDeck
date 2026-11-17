//! External application client registry and hashed token authentication.
//!
//! Raw client secrets are returned only once upon registration or rotation;
//! persistent records store only lowercase SHA-256 hex digests verified in constant time.

use std::collections::{BTreeSet, HashMap};

use nexdeck_transport::pairing::{constant_time_eq, generate_random_secret_hex, sha256_hex};
use serde::{Deserialize, Serialize};

use crate::{ApiError, ApiPermission, ClientPolicy};

/// Persisted metadata for an authorized third-party application (`clients.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientRecord {
    /// Unique client identifier (`1..=64` ASCII safe chars).
    pub client_id: String,
    /// Human-readable application name (e.g., `"ExampleApp"`).
    pub application_name: String,
    /// Lowercase SHA-256 hex digest of the issued client secret.
    pub token_hash_sha256: String,
    /// Granted permissions (sorted and deduplicated).
    pub permissions: Vec<ApiPermission>,
    /// Whether this client's access has been revoked.
    pub revoked: bool,
    /// Unix epoch milliseconds when the client was registered.
    pub created_at_epoch_ms: u64,
}

impl ClientRecord {
    /// Converts this record into a runtime [`ClientPolicy`].
    #[must_use]
    pub fn to_policy(&self) -> ClientPolicy {
        let mut policy = ClientPolicy::new(
            self.client_id.clone(),
            self.application_name.clone(),
            self.permissions.iter().copied(),
        );
        policy.revoked = self.revoked;
        policy
    }
}

/// One-time credentials returned when registering or rotating an API client secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssuedClientCredentials {
    /// Confirmed client identifier.
    pub client_id: String,
    /// Confirmed application display name.
    pub application_name: String,
    /// Newly minted bearer secret (returned once, never stored in plain text).
    #[serde(rename = "client_token")]
    pub access_secret: String,
    /// Granted permission scopes.
    pub permissions: Vec<ApiPermission>,
}

/// Registry managing registered Universal API applications and their token hashes.
#[derive(Debug, Default, Clone)]
pub struct ClientRegistry {
    clients: HashMap<String, ClientRecord>,
}

impl ClientRegistry {
    /// Creates an empty [`ClientRegistry`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Restores a [`ClientRegistry`] from persisted [`ClientRecord`] entries.
    #[must_use]
    pub fn from_records(records: Vec<ClientRecord>) -> Self {
        let mut clients = HashMap::with_capacity(records.len());
        for mut record in records {
            record.permissions = normalize_permissions(record.permissions);
            clients.insert(record.client_id.clone(), record);
        }
        Self { clients }
    }

    /// Exports all client records ordered deterministically by `client_id`.
    #[must_use]
    pub fn export_records(&self) -> Vec<ClientRecord> {
        let mut list: Vec<ClientRecord> = self.clients.values().cloned().collect();
        list.sort_by(|left, right| left.client_id.cmp(&right.client_id));
        list
    }

    /// Returns the number of registered clients (including revoked entries).
    #[must_use]
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// Looks up a [`ClientRecord`] by `client_id`.
    #[must_use]
    pub fn get(&self, client_id: &str) -> Option<&ClientRecord> {
        self.clients.get(client_id)
    }

    /// Registers a new external application and mints a one-time bearer secret.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::ValidationFailed`] on invalid IDs/names or
    /// [`ApiError::DuplicateClient`] if `client_id` is already registered.
    pub fn register_client(
        &mut self,
        client_id: String,
        application_name: String,
        permissions: impl IntoIterator<Item = ApiPermission>,
        now_epoch_ms: u64,
    ) -> Result<IssuedClientCredentials, ApiError> {
        validate_client_id(&client_id)?;
        let trimmed_name = application_name.trim();
        if trimmed_name.is_empty() || trimmed_name.len() > 80 {
            return Err(ApiError::ValidationFailed(
                "application_name must be 1..=80 characters",
            ));
        }
        if self.clients.contains_key(&client_id) {
            return Err(ApiError::DuplicateClient(client_id));
        }

        let issued_secret = generate_random_secret_hex()
            .map_err(|_| ApiError::ValidationFailed("failed to generate random client secret"))?;
        let token_hash_sha256 = sha256_hex(issued_secret.as_bytes());
        let normalized_perms = normalize_permissions(permissions);

        let record = ClientRecord {
            client_id: client_id.clone(),
            application_name: trimmed_name.to_owned(),
            token_hash_sha256,
            permissions: normalized_perms.clone(),
            revoked: false,
            created_at_epoch_ms: now_epoch_ms,
        };
        self.clients.insert(client_id.clone(), record);

        Ok(IssuedClientCredentials {
            client_id,
            application_name: trimmed_name.to_owned(),
            access_secret: issued_secret,
            permissions: normalized_perms,
        })
    }

    /// Rotates the secret for an existing client and clears any revoked flag.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::UnknownClient`] if `client_id` is not found.
    pub fn rotate_secret(&mut self, client_id: &str) -> Result<IssuedClientCredentials, ApiError> {
        let record = self
            .clients
            .get_mut(client_id)
            .ok_or_else(|| ApiError::UnknownClient(client_id.to_owned()))?;
        let issued_secret = generate_random_secret_hex()
            .map_err(|_| ApiError::ValidationFailed("failed to generate random client secret"))?;
        record.token_hash_sha256 = sha256_hex(issued_secret.as_bytes());
        record.revoked = false;

        Ok(IssuedClientCredentials {
            client_id: record.client_id.clone(),
            application_name: record.application_name.clone(),
            access_secret: issued_secret,
            permissions: record.permissions.clone(),
        })
    }

    /// Authenticates a presented bearer secret in constant time across all registered records.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::Unauthorized`] if no record matches, or [`ApiError::ClientRevoked`]
    /// if the matching client has been revoked.
    pub fn authenticate(&self, presented_secret: &str) -> Result<ClientPolicy, ApiError> {
        let trimmed = presented_secret.trim();
        if trimmed.is_empty() {
            return Err(ApiError::Unauthorized("missing client bearer credential"));
        }

        let presented_hash = sha256_hex(trimmed.as_bytes());
        let mut matched: Option<&ClientRecord> = None;
        for record in self.clients.values() {
            if constant_time_eq(&record.token_hash_sha256, &presented_hash) {
                matched = Some(record);
            }
        }

        let Some(record) = matched else {
            return Err(ApiError::Unauthorized("invalid client bearer credential"));
        };
        if record.revoked {
            return Err(ApiError::ClientRevoked(record.client_id.clone()));
        }
        Ok(record.to_policy())
    }

    /// Updates the revoked status of `client_id`.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::UnknownClient`] if `client_id` does not exist.
    pub fn set_revoked(&mut self, client_id: &str, revoked: bool) -> Result<(), ApiError> {
        let record = self
            .clients
            .get_mut(client_id)
            .ok_or_else(|| ApiError::UnknownClient(client_id.to_owned()))?;
        record.revoked = revoked;
        Ok(())
    }

    /// Replaces the granted permissions for `client_id`.
    ///
    /// # Errors
    ///
    /// Returns [`ApiError::UnknownClient`] if `client_id` does not exist.
    pub fn update_permissions(
        &mut self,
        client_id: &str,
        permissions: impl IntoIterator<Item = ApiPermission>,
    ) -> Result<(), ApiError> {
        let record = self
            .clients
            .get_mut(client_id)
            .ok_or_else(|| ApiError::UnknownClient(client_id.to_owned()))?;
        record.permissions = normalize_permissions(permissions);
        Ok(())
    }

    /// Permanently removes a client record, returning `true` if it existed.
    pub fn remove_client(&mut self, client_id: &str) -> bool {
        self.clients.remove(client_id).is_some()
    }
}

fn normalize_permissions(perms: impl IntoIterator<Item = ApiPermission>) -> Vec<ApiPermission> {
    let set: BTreeSet<ApiPermission> = perms.into_iter().collect();
    set.into_iter().collect()
}

fn validate_client_id(value: &str) -> Result<(), ApiError> {
    let valid_chars = |b: u8| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.';
    if value.is_empty()
        || value.len() > 64
        || value.contains("..")
        || !value.bytes().all(valid_chars)
    {
        return Err(ApiError::ValidationFailed(
            "client_id must use 1..=64 safe ASCII identifier characters",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_authenticates_revokes_and_rotates_client_credentials() {
        let mut reg = ClientRegistry::new();
        let creds = reg
            .register_client(
                "org.example.demo".to_owned(),
                "ExampleApp".to_owned(),
                [ApiPermission::ReadMetrics, ApiPermission::RequestApproval],
                1_700_000_000_000,
            )
            .expect("register client");

        assert_eq!(creds.access_secret.len(), 64);
        assert_ne!(
            reg.get("org.example.demo")
                .expect("stored record")
                .token_hash_sha256,
            creds.access_secret
        );

        let policy = reg
            .authenticate(&creds.access_secret)
            .expect("valid credential should authenticate");
        assert_eq!(policy.client_id, "org.example.demo");
        assert!(policy.authorize(ApiPermission::ReadMetrics).is_ok());
        assert!(policy.authorize(ApiPermission::SendNotification).is_err());

        // Revoking blocks authentication until re-enabled or rotated.
        reg.set_revoked("org.example.demo", true)
            .expect("revoke client");
        assert_eq!(
            reg.authenticate(&creds.access_secret),
            Err(ApiError::ClientRevoked("org.example.demo".to_owned()))
        );

        let rotated = reg
            .rotate_secret("org.example.demo")
            .expect("rotate secret");
        assert!(reg.authenticate(&creds.access_secret).is_err());
        assert!(reg.authenticate(&rotated.access_secret).is_ok());
    }
}
