//! TLS certificate management

use rustls::ServerConfig;
use std::fs;
use std::sync::Arc;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CertError {
    #[error("Failed to read certificate file: {0}")]
    CertReadError(#[from] std::io::Error),

    #[error("Failed to parse certificate: {0}")]
    CertParseError(String),

    #[error("Invalid certificate path: {0}")]
    InvalidPath(String),
}

/// TLS certificate manager
pub struct CertManager {
    config: Arc<ServerConfig>,
}

impl CertManager {
    /// Load certificates from files and create server config
    pub fn load(cert_path: &str, key_path: &str) -> Result<Self, CertError> {
        let cert_bytes = fs::read(cert_path)?;
        let key_bytes = fs::read(key_path)?;

        let certs: Vec<rustls::pki_types::CertificateDer<'static>> =
            rustls_pemfile::certs(&mut cert_bytes.as_slice())
                .filter_map(|c| c.ok())
                .collect();

        let key = rustls_pemfile::private_key(&mut key_bytes.as_slice())
            .ok()
            .flatten()
            .ok_or_else(|| CertError::CertParseError("No private key found".into()))?;

        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(|e| CertError::CertParseError(e.to_string()))?;

        let mut config = config;
        config.alpn_protocols = vec![b"h3".to_vec()];

        Ok(Self {
            config: Arc::new(config),
        })
    }

    /// Get the server config
    pub fn config(&self) -> Arc<ServerConfig> {
        Arc::clone(&self.config)
    }

    /// Check if certificates need rotation
    pub fn needs_rotation(&self, _days_before_expiry: u64) -> Result<bool, CertError> {
        // This would parse the certificate and check expiration
        // For now, return false
        Ok(false)
    }

    /// Get certificate expiry time
    pub fn expiry_time(&self) -> Option<std::time::SystemTime> {
        // Parse certificate and extract expiry
        // For now, return None
        None
    }
}

// Helper for PEM parsing
mod rustls_pemfile {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};

    pub fn certs<'a>(
        cursor: &mut &'a [u8],
    ) -> impl Iterator<Item = std::io::Result<CertificateDer<'static>>> + 'a {
        let mut cursor_mut = *cursor;
        std::iter::from_fn(move || {
            let data = std::str::from_utf8(cursor_mut).unwrap_or("");
            let start = match data.find("-----BEGIN CERTIFICATE-----") {
                Some(s) => s,
                None => return None,
            };

            let end = match data[start..].find("-----END CERTIFICATE-----") {
                Some(e) => start + e + 25,
                None => return None,
            };

            let pem_data = &cursor_mut[start..end];
            cursor_mut = &cursor_mut[end..];

            Some(Ok(CertificateDer::from(pem_data.to_vec())))
        })
    }

    pub fn private_key(
        cursor: &mut &[u8],
    ) -> Result<Option<PrivateKeyDer<'static>>, std::io::Error> {
        let data = std::str::from_utf8(*cursor).unwrap_or("");

        let (start, end_marker) = if let Some(s) = data.find("-----BEGIN PRIVATE KEY-----") {
            (s, "-----END PRIVATE KEY-----")
        } else if let Some(s) = data.find("-----BEGIN RSA PRIVATE KEY-----") {
            (s, "-----END RSA PRIVATE KEY-----")
        } else if let Some(s) = data.find("-----BEGIN EC PRIVATE KEY-----") {
            (s, "-----END EC PRIVATE KEY-----")
        } else {
            return Ok(None);
        };

        let end = match data[start..].find(end_marker) {
            Some(e) => start + e + end_marker.len(),
            None => return Ok(None),
        };

        let key_data = &cursor[start..end];
        *cursor = &cursor[end..];

        Ok(Some(PrivateKeyDer::Pkcs8(key_data.to_vec().into())))
    }
}
