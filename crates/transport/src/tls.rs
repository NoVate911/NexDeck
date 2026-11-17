//! Self-signed TLS identity generation and fingerprint pinning for WSS.

use axum_server::tls_rustls::RustlsConfig;
use rcgen::generate_simple_self_signed;

use crate::TransportError;
use crate::pairing::sha256_hex;

/// Self-signed TLS certificate, private key, and SHA-256 fingerprint for pinning.
#[derive(Debug, Clone)]
pub struct HostTlsIdentity {
    /// PEM-encoded X.509 certificate.
    pub cert_pem: String,
    /// PEM-encoded PKCS#8 private key.
    pub key_pem: String,
    /// Lowercase SHA-256 hex digest of the DER-encoded certificate.
    pub fingerprint_sha256: String,
}

impl HostTlsIdentity {
    /// Generates a new self-signed ECDSA TLS certificate for `san_hosts`.
    ///
    /// When `san_hosts` is empty, defaults to `["localhost", "127.0.0.1"]`.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::ChannelFailure`] if certificate generation fails.
    pub fn generate_self_signed(san_hosts: &[String]) -> Result<Self, TransportError> {
        let hosts = if san_hosts.is_empty() {
            vec!["localhost".to_owned(), "127.0.0.1".to_owned()]
        } else {
            san_hosts.to_vec()
        };

        let certified = generate_simple_self_signed(hosts)
            .map_err(|err| TransportError::ChannelFailure(format!("rcgen failed: {err}")))?;

        let fingerprint_sha256 = sha256_hex(certified.cert.der().as_ref());
        let cert_pem = certified.cert.pem();
        let key_pem = certified.signing_key.serialize_pem();

        Ok(Self {
            cert_pem,
            key_pem,
            fingerprint_sha256,
        })
    }

    /// Builds an `axum-server` [`RustlsConfig`] backed by the `ring` crypto provider.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::ChannelFailure`] if loading the PEM pair fails.
    pub async fn to_rustls_config(&self) -> Result<RustlsConfig, TransportError> {
        ensure_ring_provider();
        RustlsConfig::from_pem(
            self.cert_pem.as_bytes().to_vec(),
            self.key_pem.as_bytes().to_vec(),
        )
        .await
        .map_err(|err| TransportError::ChannelFailure(format!("rustls config failed: {err}")))
    }
}

/// Installs the `ring` crypto provider once per process (idempotent).
pub fn ensure_ring_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_valid_pem_and_sha256_fingerprint() {
        let id =
            HostTlsIdentity::generate_self_signed(&[]).expect("cert generation should succeed");
        assert!(id.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(id.key_pem.contains("BEGIN PRIVATE KEY"));
        assert_eq!(id.fingerprint_sha256.len(), 64);
        assert!(id.fingerprint_sha256.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
