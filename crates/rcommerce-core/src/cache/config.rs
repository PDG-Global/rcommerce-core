//! Redis cache configuration

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Main cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Enable Redis caching
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Redis connection configuration
    #[serde(default)]
    pub redis: RedisConfig,

    /// WebSocket session storage
    #[serde(default)]
    pub websocket_sessions: WebSocketSessionConfig,

    /// Rate limiting storage
    #[serde(default)]
    pub rate_limiting: RateLimitCacheConfig,

    /// Token blacklist
    #[serde(default)]
    pub token_blacklist: TokenBlacklistConfig,

    /// API response caching
    #[serde(default)]
    pub api_cache: ApiCacheConfig,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            redis: RedisConfig::default(),
            websocket_sessions: WebSocketSessionConfig::default(),
            rate_limiting: RateLimitCacheConfig::default(),
            token_blacklist: TokenBlacklistConfig::default(),
            api_cache: ApiCacheConfig::default(),
        }
    }
}

impl CacheConfig {
    /// Development configuration (local Redis, permissive)
    pub fn development() -> Self {
        Self {
            enabled: true,
            redis: RedisConfig::development(),
            websocket_sessions: WebSocketSessionConfig::development(),
            ..Self::default()
        }
    }

    /// Production configuration (clustered, secure)
    pub fn production() -> Self {
        Self {
            enabled: true,
            redis: RedisConfig::production(),
            websocket_sessions: WebSocketSessionConfig::production(),
            rate_limiting: RateLimitCacheConfig::production(),
            token_blacklist: TokenBlacklistConfig::production(),
            api_cache: ApiCacheConfig::production(),
        }
    }
}

/// Redis connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConfig {
    /// Redis server URL (redis://host:port/db)
    pub url: String,

    /// Use TLS/SSL for connection
    #[serde(default = "default_false")]
    pub use_tls: bool,

    /// Verify TLS certificate
    #[serde(default = "default_true")]
    pub verify_certificate: bool,

    /// Connection pool size
    #[serde(default = "default_pool_size")]
    pub pool_size: usize,

    /// Connection timeout
    #[serde(default = "default_timeout")]
    pub connect_timeout_ms: u64,

    /// Read timeout
    #[serde(default = "default_timeout")]
    pub read_timeout_ms: u64,

    /// Write timeout
    #[serde(default = "default_timeout")]
    pub write_timeout_ms: u64,

    /// Retry failed connections
    #[serde(default = "default_true")]
    pub retry_on_failure: bool,

    /// Max retry attempts
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// Retry delay in ms
    #[serde(default = "default_retry_delay")]
    pub retry_delay_ms: u64,

    /// Key prefix to avoid collisions
    #[serde(default = "default_key_prefix")]
    pub key_prefix: String,

    /// Default TTL for keys (seconds)
    #[serde(default = "default_ttl")]
    pub default_ttl_secs: u64,

    /// Redis Cluster support
    #[serde(default = "default_false")]
    pub cluster_enabled: bool,

    /// Cluster nodes (if cluster_enabled)
    #[serde(default)]
    pub cluster_nodes: Vec<String>,

    /// Redis Sentinel support
    #[serde(default = "default_false")]
    pub sentinel_enabled: bool,

    /// Sentinel nodes
    #[serde(default)]
    pub sentinel_nodes: Vec<String>,

    /// Sentinel service name
    #[serde(default = "default_sentinel_service")]
    pub sentinel_service: String,

    /// Authentication password
    #[serde(default)]
    pub password: Option<String>,

    /// Database number (0-15)
    #[serde(default)]
    pub database: u8,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: "redis://127.0.0.1:6379/0".to_string(),
            use_tls: false,
            verify_certificate: true,
            pool_size: 20,
            connect_timeout_ms: 5000,
            read_timeout_ms: 5000,
            write_timeout_ms: 5000,
            retry_on_failure: true,
            max_retries: 3,
            retry_delay_ms: 1000,
            key_prefix: "rcommerce".to_string(),
            default_ttl_secs: 3600, // 1 hour
            cluster_enabled: false,
            cluster_nodes: vec![],
            sentinel_enabled: false,
            sentinel_nodes: vec![],
            sentinel_service: "mymaster".to_string(),
            password: None,
            database: 0,
        }
    }
}

impl RedisConfig {
    /// Development configuration (local Redis)
    pub fn development() -> Self {
        Self {
            url: "redis://127.0.0.1:6379/0".to_string(),
            pool_size: 5, // Smaller pool for dev
            connect_timeout_ms: 2000,
            ..Self::default()
        }
    }

    /// Production configuration (HA Redis)
    pub fn production() -> Self {
        Self {
            url: "redis://redis-cluster:6379/0".to_string(),
            use_tls: true,
            verify_certificate: true,
            pool_size: 50, // Larger pool for production
            connect_timeout_ms: 3000,
            retry_on_failure: true,
            max_retries: 5,
            retry_delay_ms: 500,
            default_ttl_secs: 7200, // 2 hours
            key_prefix: "rcommerce:prod".to_string(),
            ..Self::default()
        }
    }

    /// Get connect timeout as Duration
    pub fn connect_timeout(&self) -> Duration {
        Duration::from_millis(self.connect_timeout_ms)
    }

    /// Get read timeout as Duration
    pub fn read_timeout(&self) -> Duration {
        Duration::from_millis(self.read_timeout_ms)
    }

    /// Get write timeout as Duration
    pub fn write_timeout(&self) -> Duration {
        Duration::from_millis(self.write_timeout_ms)
    }

    /// Get retry delay as Duration
    pub fn retry_delay(&self) -> Duration {
        Duration::from_millis(self.retry_delay_ms)
    }

    /// Get default TTL as Duration
    pub fn default_ttl(&self) -> Duration {
        Duration::from_secs(self.default_ttl_secs)
    }
}

/// WebSocket session cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketSessionConfig {
    /// Enable WebSocket session caching
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Session TTL (seconds)
    #[serde(default = "default_session_ttl")]
    pub session_ttl_secs: u64,

    /// Cache connection metadata
    #[serde(default = "default_true")]
    pub cache_metadata: bool,

    /// Cache subscriptions
    #[serde(default = "default_true")]
    pub cache_subscriptions: bool,

    /// Restore sessions on reconnect
    #[serde(default = "default_true")]
    pub restore_on_reconnect: bool,
}

impl Default for WebSocketSessionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            session_ttl_secs: 7200, // 2 hours
            cache_metadata: true,
            cache_subscriptions: true,
            restore_on_reconnect: true,
        }
    }
}

impl WebSocketSessionConfig {
    /// Development configuration
    pub fn development() -> Self {
        Self {
            session_ttl_secs: 3600, // 1 hour for dev
            ..Self::default()
        }
    }

    /// Production configuration
    pub fn production() -> Self {
        Self {
            session_ttl_secs: 14400, // 4 hours for production
            ..Self::default()
        }
    }

    /// Get session TTL as Duration
    pub fn session_ttl(&self) -> Duration {
        Duration::from_secs(self.session_ttl_secs)
    }
}

/// Rate limiting cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitCacheConfig {
    /// Enable rate limiting cache
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Cache TTL (seconds)
    #[serde(default = "default_rate_limit_ttl")]
    pub ttl_secs: u64,

    /// Window precision (seconds)
    #[serde(default = "default_window_precision")]
    pub window_precision_secs: u64,

    /// Sync with Redis
    #[serde(default = "default_true")]
    pub sync_with_redis: bool,
}

impl Default for RateLimitCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl_secs: 3600,           // 1 hour
            window_precision_secs: 1, // 1 second precision
            sync_with_redis: true,
        }
    }
}

impl RateLimitCacheConfig {
    /// Production configuration
    pub fn production() -> Self {
        Self {
            ttl_secs: 7200, // 2 hours
            window_precision_secs: 1,
            ..Self::default()
        }
    }

    /// Get TTL as Duration
    pub fn ttl(&self) -> Duration {
        Duration::from_secs(self.ttl_secs)
    }

    /// Get window precision as Duration
    pub fn window_precision(&self) -> Duration {
        Duration::from_secs(self.window_precision_secs)
    }
}

/// Token blacklist cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBlacklistConfig {
    /// Enable token blacklist
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Blacklist TTL (seconds)
    #[serde(default = "default_blacklist_ttl")]
    pub ttl_secs: u64,

    /// Check on each request
    #[serde(default = "default_true")]
    pub check_on_each_request: bool,
}

impl Default for TokenBlacklistConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl_secs: 86400, // 24 hours
            check_on_each_request: true,
        }
    }
}

impl TokenBlacklistConfig {
    /// Production configuration
    pub fn production() -> Self {
        Self {
            ttl_secs: 604800, // 7 days for production
            ..Self::default()
        }
    }

    /// Get TTL as Duration
    pub fn ttl(&self) -> Duration {
        Duration::from_secs(self.ttl_secs)
    }
}

/// API response cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiCacheConfig {
    /// Enable API caching
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Default TTL for cached responses (seconds)
    #[serde(default = "default_api_cache_ttl")]
    pub default_ttl_secs: u64,

    /// Cache successful responses only
    #[serde(default = "default_true")]
    pub cache_success_only: bool,

    /// Cache 200 OK responses
    #[serde(default = "default_true")]
    pub cache_200: bool,

    /// Cache 404 responses
    #[serde(default = "default_false")]
    pub cache_404: bool,

    /// Cache size limit (MB)
    #[serde(default = "default_cache_size_limit")]
    pub size_limit_mb: u64,

    /// Max key size
    #[serde(default = "default_max_key_size")]
    pub max_key_size_bytes: usize,

    /// Compression threshold (bytes)
    #[serde(default = "default_compression_threshold")]
    pub compression_threshold: usize,

    /// Enable compression
    #[serde(default = "default_true")]
    pub enable_compression: bool,
}

impl Default for ApiCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_ttl_secs: 300, // 5 minutes
            cache_success_only: true,
            cache_200: true,
            cache_404: false,
            size_limit_mb: 100,          // 100MB total
            max_key_size_bytes: 1024,    // 1KB max key size
            compression_threshold: 1024, // 1KB
            enable_compression: true,
        }
    }
}

impl ApiCacheConfig {
    /// Production configuration
    pub fn production() -> Self {
        Self {
            default_ttl_secs: 600, // 10 minutes
            size_limit_mb: 500,    // 500MB
            ..Self::default()
        }
    }

    /// Get TTL as Duration
    pub fn default_ttl(&self) -> Duration {
        Duration::from_secs(self.default_ttl_secs)
    }

    /// Get size limit in bytes
    pub fn size_limit_bytes(&self) -> u64 {
        self.size_limit_mb * 1024 * 1024
    }
}

// Default value helper functions
fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}
fn default_pool_size() -> usize {
    20
}
fn default_timeout() -> u64 {
    5000
}
fn default_max_retries() -> u32 {
    3
}
fn default_retry_delay() -> u64 {
    1000
}
fn default_key_prefix() -> String {
    "rcommerce".to_string()
}
fn default_ttl() -> u64 {
    3600
}
fn default_sentinel_service() -> String {
    "mymaster".to_string()
}
fn default_session_ttl() -> u64 {
    7200
}
fn default_rate_limit_ttl() -> u64 {
    3600
}
fn default_window_precision() -> u64 {
    1
}
fn default_blacklist_ttl() -> u64 {
    86400
}
fn default_api_cache_ttl() -> u64 {
    300
}
fn default_cache_size_limit() -> u64 {
    100
}
fn default_max_key_size() -> usize {
    1024
}
fn default_compression_threshold() -> usize {
    1024
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redis_config_default() {
        let config = RedisConfig::default();
        assert_eq!(config.url, "redis://127.0.0.1:6379/0");
        assert_eq!(config.pool_size, 20);
        assert_eq!(config.connect_timeout(), Duration::from_millis(5000));
    }

    #[test]
    fn test_redis_config_development() {
        let config = RedisConfig::development();
        assert_eq!(config.pool_size, 5);
        assert_eq!(config.connect_timeout(), Duration::from_millis(2000));
    }

    #[test]
    fn test_redis_config_production() {
        let config = RedisConfig::production();
        assert!(config.use_tls);
        assert_eq!(config.pool_size, 50);
        assert!(config.verify_certificate);
    }

    #[test]
    fn test_cache_config() {
        let config = CacheConfig::default();
        assert!(config.enabled);
        assert!(config.websocket_sessions.enabled);
        assert!(config.rate_limiting.enabled);
    }

    #[test]
    fn test_websocket_session_config() {
        let config = WebSocketSessionConfig::default();
        assert_eq!(config.session_ttl(), Duration::from_secs(7200));

        let prod = WebSocketSessionConfig::production();
        assert_eq!(prod.session_ttl(), Duration::from_secs(14400));
    }

    #[test]
    fn test_rate_limit_cache_config() {
        let config = RateLimitCacheConfig::default();
        assert_eq!(config.ttl(), Duration::from_secs(3600));
        assert!(config.sync_with_redis);
    }

    #[test]
    fn test_token_blacklist_config() {
        let config = TokenBlacklistConfig::default();
        assert_eq!(config.ttl(), Duration::from_secs(86400));
        assert!(config.check_on_each_request);
    }

    #[test]
    fn test_api_cache_config() {
        let config = ApiCacheConfig::default();
        assert_eq!(config.default_ttl(), Duration::from_secs(300));
        assert_eq!(config.size_limit_bytes(), 100 * 1024 * 1024);
        assert!(config.enable_compression);
    }
}
