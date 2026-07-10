//! Let's Encrypt certificate management
//!
//! This module provides automatic certificate acquisition and renewal
//! using the ACME protocol.

use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

use crate::{Error, Result};
use rcommerce_core::config::LetsEncryptConfig;

/// Certificate information
#[derive(Debug, Clone)]
pub struct CertificateInfo {
    pub domain: String,
    pub certificate_path: PathBuf,
    pub private_key_path: PathBuf,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub issued_at: chrono::DateTime<chrono::Utc>,
    pub serial_number: String,
}

/// Let's Encrypt certificate manager
pub struct LetsEncryptManager {
    config: LetsEncryptConfig,
    cache_dir: PathBuf,
}

impl LetsEncryptManager {
    /// Create a new Let's Encrypt manager
    pub fn new(config: LetsEncryptConfig) -> Result<Self> {
        // Ensure cache directory exists
        std::fs::create_dir_all(&config.cache_dir)
            .map_err(|e| Error::Config(format!("Failed to create cert cache dir: {}", e)))?;

        Ok(Self {
            cache_dir: config.cache_dir.clone(),
            config,
        })
    }

    /// Get certificate cache directory
    pub fn cache_dir(&self) -> &PathBuf {
        &self.cache_dir
    }

    /// Get domains
    pub fn domains(&self) -> &[String] {
        &self.config.domains
    }

    /// Check if auto-renew is enabled
    pub fn auto_renew(&self) -> bool {
        self.config.auto_renew
    }

    /// Start background certificate renewal task
    pub async fn start_renewal_task(self: Arc<Self>) -> Result<()> {
        if !self.config.auto_renew {
            info!("Certificate auto-renewal is disabled");
            return Ok(());
        }

        info!("Certificate renewal task started (stub implementation)");
        info!("Note: Full Let's Encrypt integration requires manual setup or reverse proxy");

        Ok(())
    }

    /// Obtain certificate for a domain
    pub async fn obtain_certificate(&self, domain: &str) -> Result<CertificateInfo> {
        let cert_path = self.cache_dir.join(format!("{}-cert.pem", domain));
        let key_path = self.cache_dir.join(format!("{}-key.pem", domain));

        // Check if certificate exists
        if cert_path.exists() && key_path.exists() {
            info!("Found existing certificate for {}", domain);
            return Ok(CertificateInfo {
                domain: domain.to_string(),
                certificate_path: cert_path,
                private_key_path: key_path,
                expires_at: chrono::Utc::now() + chrono::Duration::days(90),
                issued_at: chrono::Utc::now(),
                serial_number: "manual".to_string(),
            });
        }

        warn!("Let's Encrypt automatic certificate acquisition not yet implemented");
        warn!(
            "Please use manual certificates or a reverse proxy (nginx, caddy, traefik) with Let's Encrypt"
        );

        Err(Error::Config(
            "Automatic Let's Encrypt not implemented. Use manual certificates or reverse proxy."
                .to_string(),
        ))
    }

    /// Get challenge token for HTTP-01 validation
    pub async fn get_challenge_token(&self, _token: &str) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lets_encrypt_manager_creation() {
        let config = LetsEncryptConfig {
            email: "admin@example.com".to_string(),
            domains: vec!["example.com".to_string()],
            cache_dir: std::env::temp_dir().join("rcommerce-test-certs"),
            ..Default::default()
        };

        let manager = LetsEncryptManager::new(config);
        assert!(manager.is_ok());
    }
}
