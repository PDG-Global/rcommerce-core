//! Redis caching layer for performance optimization
//!
//! This module provides Redis-based caching for:
//! - WebSocket session storage
//! - Rate limiting data persistence
//! - Message caching
//! - Pub/Sub for WebSocket broadcasting
//! - Token blacklisting
//!
//! ## Security Features
//!
//! - TLS/SSL support for encrypted connections
//! - Connection pooling to prevent resource exhaustion
//! - Authentication support
//! - Key prefixing to prevent collisions
//! - Timeout configuration
//! - Automatic reconnection on failures
//!
//! ## Performance Characteristics
//!
//! - Connection pooling: Reduces connection overhead
//! - Pipeline support: Batch operations for efficiency
//! - Async operations: Non-blocking Redis calls
//! - Cluster support: Horizontal scaling
//! - TTL support: Automatic key expiration

pub mod config;
pub mod connection;
pub mod pubsub;
pub mod rate_limit;
pub mod session;
pub mod token;

// Re-export main types
pub use config::{CacheConfig, RedisConfig, WebSocketSessionConfig};
pub use connection::{RedisConnection, RedisPool};
pub use pubsub::{RedisPubSub, Subscription};
pub use rate_limit::{RateLimitInfo, RedisRateLimiter};
pub use session::{SessionStore, WebSocketSession};
pub use token::{BlacklistedToken, TokenBlacklist};

/// Cache result type alias
pub type CacheResult<T> = Result<T, CacheError>;

/// Cache-specific error types
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("Redis connection error: {0}")]
    ConnectionError(String),

    #[error("Operation failed: {0}")]
    OperationError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Deserialization error: {0}")]
    DeserializationError(String),

    #[error("Key not found: {0}")]
    NotFound(String),

    #[error("TTL expired for key: {0}")]
    Expired(String),

    #[error("Pool exhausted")]
    PoolExhausted,

    #[error("Timeout waiting for connection")]
    Timeout,
}

impl From<CacheError> for crate::Error {
    fn from(err: CacheError) -> Self {
        crate::Error::Cache(err.to_string())
    }
}

/// Connection state tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Connected and ready
    Connected,

    /// Disconnected but reconnecting
    Reconnecting,

    /// Failed, manual intervention needed
    Failed,

    /// Pool exhausted
    Exhausted,
}

/// Cache key prefix to avoid collisions
#[derive(Debug, Clone)]
pub struct KeyPrefix {
    prefix: String,
}

impl KeyPrefix {
    /// Create a new key prefix
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }

    /// Prefix a key
    pub fn key(&self, key: impl AsRef<str>) -> String {
        format!("{}:{}", self.prefix, key.as_ref())
    }
}

impl Default for KeyPrefix {
    fn default() -> Self {
        Self::new("rcommerce")
    }
}

/// Cache namespace for different data types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheNamespace {
    /// WebSocket session data
    WebSocketSession,

    /// Rate limiting data
    RateLimit,

    /// Token blacklist
    TokenBlacklist,

    /// Message queue
    MessageQueue,

    /// API response cache
    ApiResponse,

    /// Session data
    Session,

    /// Statistics cache
    Statistics,
}

impl CacheNamespace {
    /// Get the string prefix for this namespace
    pub fn prefix(&self) -> &'static str {
        match self {
            CacheNamespace::WebSocketSession => "ws:session",
            CacheNamespace::RateLimit => "rate:limit",
            CacheNamespace::TokenBlacklist => "token:blacklist",
            CacheNamespace::MessageQueue => "msg:queue",
            CacheNamespace::ApiResponse => "api:cache",
            CacheNamespace::Session => "session",
            CacheNamespace::Statistics => "stats",
        }
    }

    /// Create a prefixed key
    pub fn key(&self, key: impl AsRef<str>) -> String {
        format!("{}:{}", self.prefix(), key.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_prefix() {
        let prefix = KeyPrefix::new("test");
        assert_eq!(prefix.key("user:123"), "test:user:123");
    }

    #[test]
    fn test_cache_namespace() {
        assert_eq!(
            CacheNamespace::WebSocketSession.key("conn:123"),
            "ws:session:conn:123"
        );

        assert_eq!(
            CacheNamespace::RateLimit.key("ip:192.168.1.1"),
            "rate:limit:ip:192.168.1.1"
        );
    }
}
