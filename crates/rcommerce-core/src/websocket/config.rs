//! WebSocket configuration and constants

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for WebSocket server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    /// Enable WebSocket support
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Maximum number of concurrent connections
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,

    /// Maximum connections per IP address
    #[serde(default = "default_max_per_ip")]
    pub max_connections_per_ip: usize,

    /// Maximum connections per user
    #[serde(default = "default_max_per_user")]
    pub max_connections_per_user: usize,

    /// Enable origin validation
    #[serde(default = "default_true")]
    pub validate_origin: bool,

    /// Allowed origins (if validate_origin is true)
    #[serde(default)]
    pub allowed_origins: Vec<String>,

    /// Require authentication
    #[serde(default = "default_true")]
    pub require_auth: bool,

    /// Enable CSRF protection
    #[serde(default = "default_true")]
    pub csrf_protection: bool,

    /// Maximum message size in bytes
    #[serde(default = "default_max_message_size")]
    pub max_message_size: usize,

    /// Enable message rate limiting
    #[serde(default = "default_true")]
    pub enable_message_rate_limit: bool,

    /// Maximum messages per minute per connection
    #[serde(default = "default_messages_per_minute")]
    pub max_messages_per_minute: u32,

    /// Enable connection rate limiting
    #[serde(default = "default_true")]
    pub enable_connection_rate_limit: bool,

    /// Maximum connections per minute per IP
    #[serde(default = "default_connections_per_minute")]
    pub max_connections_per_minute: u32,

    /// Enable ping/pong keep-alive
    #[serde(default = "default_true")]
    pub enable_ping_pong: bool,

    /// Ping interval in seconds
    #[serde(default = "default_ping_interval")]
    pub ping_interval_secs: u64,

    /// Connection timeout if no pong received
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout_secs: u64,

    /// Maximum subscription topics per connection
    #[serde(default = "default_max_subscriptions")]
    pub max_subscriptions_per_connection: usize,

    /// Enable broadcasting
    #[serde(default = "default_true")]
    pub enable_broadcasting: bool,

    /// Broadcast channel buffer size
    #[serde(default = "default_broadcast_buffer")]
    pub broadcast_buffer_size: usize,

    /// Enable compression for large messages
    #[serde(default = "default_false")]
    pub enable_compression: bool,

    /// Compression threshold in bytes
    #[serde(default = "default_compression_threshold")]
    pub compression_threshold: usize,

    /// Enable message queuing when connection is slow
    #[serde(default = "default_true")]
    pub enable_backpressure: bool,

    /// Maximum queued messages per connection
    #[serde(default = "default_max_queued_messages")]
    pub max_queued_messages: usize,

    /// Enable metrics collection
    #[serde(default = "default_true")]
    pub enable_metrics: bool,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_connections: 10000,
            max_connections_per_ip: 10,
            max_connections_per_user: 3,
            validate_origin: true,
            allowed_origins: vec![],
            require_auth: true,
            csrf_protection: true,
            max_message_size: 1024 * 1024, // 1MB
            enable_message_rate_limit: true,
            max_messages_per_minute: 100,
            enable_connection_rate_limit: true,
            max_connections_per_minute: 10,
            enable_ping_pong: true,
            ping_interval_secs: 30,
            connection_timeout_secs: 60,
            max_subscriptions_per_connection: 50,
            enable_broadcasting: true,
            broadcast_buffer_size: 1000,
            enable_compression: false,
            compression_threshold: 1024 * 10, // 10KB
            enable_backpressure: true,
            max_queued_messages: 100,
            enable_metrics: true,
        }
    }
}

impl WebSocketConfig {
    /// Get ping interval as Duration
    pub fn ping_interval(&self) -> Duration {
        Duration::from_secs(self.ping_interval_secs)
    }

    /// Get connection timeout as Duration
    pub fn connection_timeout(&self) -> Duration {
        Duration::from_secs(self.connection_timeout_secs)
    }

    /// Check if origin is allowed
    pub fn is_origin_allowed(&self, origin: &str) -> bool {
        if !self.validate_origin {
            return true; // Origin validation disabled
        }

        if self.allowed_origins.is_empty() {
            // If no allowed origins specified, allow all (but log warning)
            tracing::warn!(
                "Origin validation enabled but no allowed_origins configured. Allowing origin: {}",
                origin
            );
            return true;
        }

        self.allowed_origins.contains(&origin.to_string())
    }
}

// Default value helper functions
fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_max_connections() -> usize {
    10000
}
fn default_max_per_ip() -> usize {
    10
}
fn default_max_per_user() -> usize {
    3
}
fn default_max_message_size() -> usize {
    1024 * 1024
} // 1MB
fn default_messages_per_minute() -> u32 {
    100
}
fn default_connections_per_minute() -> u32 {
    10
}
fn default_ping_interval() -> u64 {
    30
}
fn default_connection_timeout() -> u64 {
    60
}
fn default_max_subscriptions() -> usize {
    50
}
fn default_broadcast_buffer() -> usize {
    1000
}
fn default_compression_threshold() -> usize {
    1024 * 10
} // 10KB
fn default_max_queued_messages() -> usize {
    100
}

/// Predefined configurations for common scenarios
impl WebSocketConfig {
    /// Development configuration (very permissive)
    pub fn development() -> Self {
        Self {
            max_connections: 100,
            max_connections_per_ip: 100,
            max_connections_per_user: 100,
            validate_origin: false,
            require_auth: false,
            csrf_protection: false,
            enable_message_rate_limit: false,
            enable_connection_rate_limit: false,
            ..Self::default()
        }
    }

    /// Production configuration (secure defaults)
    pub fn production() -> Self {
        Self::default()
    }

    /// High scale configuration (for large deployments)
    pub fn high_scale() -> Self {
        Self {
            max_connections: 100000,
            max_connections_per_ip: 50,
            max_connections_per_user: 5,
            broadcast_buffer_size: 10000,
            max_queued_messages: 1000,
            ..Self::default()
        }
    }

    /// Secure configuration (maximum security)
    pub fn secure() -> Self {
        Self {
            max_connections_per_ip: 5,
            max_connections_per_user: 2,
            validate_origin: true,
            allowed_origins: vec![], // Must be explicitly configured
            require_auth: true,
            csrf_protection: true,
            max_message_size: 1024 * 100, // 100KB max
            max_messages_per_minute: 50,
            max_connections_per_minute: 5,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = WebSocketConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_connections, 10000);
        assert_eq!(config.max_connections_per_ip, 10);
        assert_eq!(config.ping_interval(), Duration::from_secs(30));
    }

    #[test]
    fn test_development_config() {
        let config = WebSocketConfig::development();
        assert!(!config.validate_origin);
        assert!(!config.require_auth);
        assert!(!config.enable_message_rate_limit);
    }

    #[test]
    fn test_secure_config() {
        let config = WebSocketConfig::secure();
        assert_eq!(config.max_connections_per_ip, 5);
        assert_eq!(config.max_message_size, 1024 * 100);
        assert_eq!(config.max_messages_per_minute, 50);
    }

    #[test]
    fn test_high_scale_config() {
        let config = WebSocketConfig::high_scale();
        assert_eq!(config.max_connections, 100000);
        assert_eq!(config.broadcast_buffer_size, 10000);
    }

    #[test]
    fn test_origin_validation() {
        let config = WebSocketConfig {
            validate_origin: true,
            allowed_origins: vec!["https://rcommerce.app".to_string()],
            ..Default::default()
        };

        assert!(config.is_origin_allowed("https://rcommerce.app"));
        assert!(!config.is_origin_allowed("https://evil.com"));
    }

    #[test]
    fn test_origin_validation_disabled() {
        let config = WebSocketConfig {
            validate_origin: false,
            ..Default::default()
        };

        assert!(config.is_origin_allowed("https://any-origin.com"));
    }
}
