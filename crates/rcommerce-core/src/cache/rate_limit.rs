//! Redis-backed rate limiting for WebSocket connections
//!
//! This provides distributed rate limiting that works across multiple server instances

use crate::cache::{CacheNamespace, CacheResult, RedisConfig, RedisPool};
use std::time::{Duration, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// Redis-backed rate limiter
pub struct RedisRateLimiter {
    /// Redis pool
    pool: RedisPool,

    /// Configuration
    config: RedisConfig,

    /// Default TTL for rate limit keys
    #[allow(dead_code)]
    default_ttl: Duration,

    /// Window precision
    #[allow(dead_code)]
    window_precision: Duration,
}

impl RedisRateLimiter {
    /// Create a new Redis rate limiter
    pub async fn new(pool: RedisPool, config: RedisConfig) -> CacheResult<Self> {
        let default_ttl = config.default_ttl();
        let window_precision = Duration::from_secs(1);

        Ok(Self {
            pool,
            config,
            default_ttl,
            window_precision,
        })
    }

    /// Create rate limit key
    fn rate_limit_key(&self, namespace: CacheNamespace, identifier: &str, window: &str) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let window_start = match window {
            "minute" => now - (now % 60),
            "hour" => now - (now % 3600),
            "day" => now - (now % 86400),
            _ => now,
        };

        format!(
            "{}:{}/{}/{}/{}",
            self.config.key_prefix,
            namespace.prefix(),
            identifier,
            window,
            window_start
        )
    }

    /// Check if request is allowed under rate limit
    pub async fn check_rate_limit(
        &self,
        namespace: CacheNamespace,
        identifier: &str,
        window: &str, // "minute", "hour", "day"
        limit: u64,
    ) -> CacheResult<bool> {
        let conn = self.pool.get().await?;
        let key = self.rate_limit_key(namespace, identifier, window);

        // Use Redis INCR to atomically increment counter
        let current = conn.incr(&key).await? as u64;

        // Set TTL on first increment
        if current == 1 {
            let ttl = match window {
                "minute" => 60,
                "hour" => 3600,
                "day" => 86400,
                _ => 60,
            };

            conn.expire(&key, ttl).await?;
        }

        let allowed = current <= limit;

        if !allowed {
            warn!(
                "Rate limit exceeded: namespace={}, identifier={}, window={}, current={}, limit={}",
                namespace.prefix(),
                identifier,
                window,
                current,
                limit
            );
        } else {
            debug!(
                "Rate limit check: namespace={}, identifier={}, window={}, current={}/{}",
                namespace.prefix(),
                identifier,
                window,
                current,
                limit
            );
        }

        Ok(allowed)
    }

    /// Get current rate limit usage
    pub async fn get_usage(
        &self,
        namespace: CacheNamespace,
        identifier: &str,
        window: &str,
    ) -> CacheResult<Option<u64>> {
        let conn = self.pool.get().await?;
        let key = self.rate_limit_key(namespace, identifier, window);

        // Get current value, return None if key doesn't exist
        match conn.get(&key).await? {
            Some(data) => {
                let count = String::from_utf8_lossy(&data).parse::<u64>().ok();
                Ok(count)
            }
            None => Ok(None),
        }
    }

    /// Reset rate limit counter
    pub async fn reset(
        &self,
        namespace: CacheNamespace,
        identifier: &str,
        window: &str,
    ) -> CacheResult<bool> {
        let conn = self.pool.get().await?;
        let key = self.rate_limit_key(namespace, identifier, window);

        conn.del(&key).await
    }

    /// Block an identifier (add to blocklist)
    pub async fn block(&self, identifier: &str, ttl: Duration) -> CacheResult<()> {
        let conn = self.pool.get().await?;
        let key = format!("{}:blocklist:{}", self.config.key_prefix, identifier);

        conn.setex(&key, ttl.as_secs(), b"1").await?;

        warn!("Blocked identifier: {}", identifier);

        Ok(())
    }

    /// Unblock an identifier
    pub async fn unblock(&self, identifier: &str) -> CacheResult<bool> {
        let conn = self.pool.get().await?;
        let key = format!("{}:blocklist:{}", self.config.key_prefix, identifier);

        let deleted = conn.del(&key).await?;

        if deleted {
            info!("Unblocked identifier: {}", identifier);
        }

        Ok(deleted)
    }

    /// Check if identifier is blocked
    pub async fn is_blocked(&self, identifier: &str) -> CacheResult<bool> {
        let conn = self.pool.get().await?;
        let key = format!("{}:blocklist:{}", self.config.key_prefix, identifier);

        conn.exists(&key).await
    }

    /// Check multiple rate limits at once
    pub async fn check_multiple_limits(
        &self,
        namespace: CacheNamespace,
        identifier: &str,
        limits: &[(&str, u64)], // [(window, limit), ...]
    ) -> CacheResult<Vec<bool>> {
        let mut results = Vec::with_capacity(limits.len());

        for (window, limit) in limits {
            let allowed = self
                .check_rate_limit(namespace, identifier, window, *limit)
                .await?;
            results.push(allowed);
        }

        Ok(results)
    }

    /// Get rate limit info for multiple windows
    pub async fn get_multiple_usage(
        &self,
        namespace: CacheNamespace,
        identifier: &str,
        windows: &[&str],
    ) -> CacheResult<Vec<Option<u64>>> {
        let mut results = Vec::with_capacity(windows.len());

        for window in windows {
            let usage = self.get_usage(namespace, identifier, window).await?;
            results.push(usage);
        }

        Ok(results)
    }

    /// Execute rate limited operation with retry
    pub async fn execute_rate_limited<F, Fut, T>(
        &self,
        namespace: CacheNamespace,
        identifier: &str,
        window: &str,
        limit: u64,
        operation: F,
    ) -> CacheResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = CacheResult<T>>,
    {
        // Check rate limit
        if !self
            .check_rate_limit(namespace, identifier, window, limit)
            .await?
        {
            return Err(crate::cache::CacheError::OperationError(
                "Rate limit exceeded".to_string(),
            ));
        }

        // Execute operation
        operation().await
    }
}

/// Rate limit info for a specific window
#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    /// Current count
    pub current: u64,

    /// Limit
    pub limit: u64,

    /// Window type
    pub window: String,

    /// Is rate limited
    pub is_limited: bool,

    /// Time until reset (seconds)
    pub time_until_reset: u64,
}

impl RateLimitInfo {
    /// Calculate percentage used
    pub fn usage_percent(&self) -> f64 {
        if self.limit == 0 {
            0.0
        } else {
            (self.current as f64 / self.limit as f64) * 100.0
        }
    }

    /// Check if approaching limit
    pub fn is_near_limit(&self, threshold: f64) -> bool {
        self.usage_percent() >= threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::RedisConfig;

    #[tokio::test]
    async fn test_rate_limit_key_generation() {
        let config = RedisConfig::default();
        let pool = RedisPool::new(config).await;

        if let Ok(pool) = pool {
            let limiter = RedisRateLimiter::new(pool, RedisConfig::default())
                .await
                .unwrap();
            let key = limiter.rate_limit_key(CacheNamespace::RateLimit, "192.168.1.1", "minute");

            assert!(key.contains("rate:limit"));
            assert!(key.contains("192.168.1.1"));
            assert!(key.contains("minute"));
        }
    }

    #[tokio::test]
    async fn test_check_rate_limit() {
        let config = RedisConfig::default();
        let pool = RedisPool::new(config).await;

        if let Ok(pool) = pool {
            // Unique identifier per run: the rate-limit key embeds the current
            // time bucket, and the previous hardcoded "delete first" key never
            // matched it, so shared state from earlier runs made the usage
            // assertion below flaky.
            let identifier = format!(
                "10.255.{}.{}",
                std::process::id() % 256,
                uuid::Uuid::new_v4().simple()
            );

            let limiter = RedisRateLimiter::new(pool, RedisConfig::default())
                .await
                .unwrap();

            // First request should be allowed
            let allowed = limiter
                .check_rate_limit(CacheNamespace::RateLimit, &identifier, "minute", 5)
                .await
                .unwrap();
            assert!(allowed);

            // Get usage
            let usage = limiter
                .get_usage(CacheNamespace::RateLimit, &identifier, "minute")
                .await
                .unwrap();
            assert_eq!(usage, Some(1));
        }
    }

    #[tokio::test]
    async fn test_block_unblock() {
        let config = RedisConfig::default();
        let pool = RedisPool::new(config).await;

        if let Ok(pool) = pool {
            let limiter = RedisRateLimiter::new(pool, RedisConfig::default())
                .await
                .unwrap();

            // Block
            limiter
                .block("192.168.1.100", Duration::from_secs(60))
                .await
                .unwrap();

            // Check blocked
            let is_blocked = limiter.is_blocked("192.168.1.100").await.unwrap();
            assert!(is_blocked);

            // Unblock
            limiter.unblock("192.168.1.100").await.unwrap();

            // Check unblocked
            let is_blocked = limiter.is_blocked("192.168.1.100").await.unwrap();
            assert!(!is_blocked);
        }
    }
}
