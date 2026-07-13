//! Configuration structures for CDN edge server

use serde::Deserialize;
use std::time::Duration;

/// Main settings structure
#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub server: ServerSettings,
    pub cache: CacheSettings,
    pub origin: OriginSettings,
    pub tls: TlsSettings,
    pub control_plane: ControlPlaneSettings,
    pub security: SecuritySettings,
    pub logging: LoggingSettings,
}

/// Server configuration
#[derive(Debug, Deserialize, Clone)]
pub struct ServerSettings {
    /// Server bind address
    pub address: String,
    /// Metrics server address
    pub metrics_address: String,
    /// Number of worker threads (0 = auto)
    pub worker_threads: usize,
}

/// Cache configuration
#[derive(Debug, Deserialize, Clone)]
pub struct CacheSettings {
    /// Maximum in-memory cache size (e.g., "4Gi", "512Mi")
    pub memory_max_size: String,
    /// Disk cache path
    pub disk_path: String,
    /// Maximum disk cache size
    pub disk_max_size: String,
    /// Default TTL for cached items
    pub default_ttl: u64,
}

/// Origin server configuration
#[derive(Debug, Deserialize, Clone)]
pub struct OriginSettings {
    /// Request timeout
    pub timeout: u64,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Origin shield nodes
    pub shield_nodes: Vec<String>,
    /// Connection pool size
    pub pool_size: u32,
}

/// TLS configuration
#[derive(Debug, Deserialize, Clone)]
pub struct TlsSettings {
    /// Certificate file path
    pub cert_path: String,
    /// Private key file path
    pub key_path: String,
    /// Enable ACME/Let's Encrypt
    pub acme_enabled: bool,
    /// ACME email for notifications
    pub acme_email: Option<String>,
}

/// Control plane configuration
#[derive(Debug, Deserialize, Clone)]
pub struct ControlPlaneSettings {
    /// Control plane API endpoint
    pub api_endpoint: String,
    /// Heartbeat interval
    pub heartbeat_interval: u64,
    /// Edge location identifier
    pub location: String,
}

/// Security configuration
#[derive(Debug, Deserialize, Clone)]
pub struct SecuritySettings {
    /// Rate limit requests per window
    pub rate_limit_requests: u64,
    /// Rate limit time window
    pub rate_limit_window: u64,
    /// Enable DDoS detection
    pub ddos_detection_enabled: bool,
}

/// Logging configuration
#[derive(Debug, Deserialize, Clone)]
pub struct LoggingSettings {
    /// Log level (trace, debug, info, warn, error)
    pub level: String,
}

impl Settings {
    /// Parse human-readable size string (e.g., "4Gi") to bytes
    pub fn parse_size(size_str: &str) -> Option<u64> {
        let s = size_str.to_lowercase();
        let (num, unit) = s.split_at(s.chars().position(|c| !c.is_numeric())?);
        let num: u64 = num.parse().ok()?;
        let multiplier = match unit {
            "b" => 1,
            "kb" | "k" => 1024,
            "mb" | "m" => 1024 * 1024,
            "gb" | "g" => 1024 * 1024 * 1024,
            "tb" | "t" => 1024 * 1024 * 1024 * 1024,
            _ => return None,
        };
        Some(num.saturating_mul(multiplier))
    }

    pub fn origin_timeout(&self) -> Duration {
        Duration::from_secs(self.origin.timeout)
    }

    pub fn heartbeat_interval(&self) -> Duration {
        Duration::from_secs(self.control_plane.heartbeat_interval)
    }

    pub fn rate_limit_window(&self) -> Duration {
        Duration::from_secs(self.security.rate_limit_window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size() {
        assert_eq!(Settings::parse_size("100b"), Some(100));
        assert_eq!(Settings::parse_size("4k"), Some(4 * 1024));
        assert_eq!(Settings::parse_size("4kb"), Some(4 * 1024));
        assert_eq!(Settings::parse_size("2m"), Some(2 * 1024 * 1024));
        assert_eq!(Settings::parse_size("2mb"), Some(2 * 1024 * 1024));
        assert_eq!(Settings::parse_size("1g"), Some(1024 * 1024 * 1024));
        assert_eq!(Settings::parse_size("1gb"), Some(1024 * 1024 * 1024));
        assert_eq!(Settings::parse_size("invalid"), None);
    }
}
