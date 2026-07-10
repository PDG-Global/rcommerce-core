//! WebSocket rate limiting to prevent abuse
//!
//! This module provides rate limiting for:
//! - Connection attempts (per IP)
//! - Messages (per connection)

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tracing::{warn, debug, info};
use uuid::Uuid;

/// Error types for rate limiting
#[derive(Debug, Clone, thiserror::Error)]
pub enum RateLimitError {
    #[error("Rate limit exceeded for {resource}. Retry after {retry_after:?}")]
    RateLimited {
        resource: String,
        retry_after: Duration,
    },
    
    #[error("Too many concurrent {resource}. Max: {max}")]
    TooManyConcurrent {
        resource: String,
        max: u32,
    },
    
    #[error("Blocklisted IP: {0}")]
    BlocklistedIp(String),
}

/// Tracks rate limit usage for a single identifier
#[derive(Debug, Clone)]
struct RateLimitTracker {
    /// Count of actions in current window
    count: u32,
    /// Max allowed in window
    limit: u32,
    /// When current window started
    window_start: Instant,
    /// Window duration
    window_duration: Duration,
    /// Number of concurrent actions
    concurrent: u32,
    /// Max concurrent allowed
    max_concurrent: u32,
    /// Total count (for analytics)
    total_count: u64,
}

impl RateLimitTracker {
    /// Create a new tracker
    fn new(limit: u32, window_duration: Duration, max_concurrent: u32) -> Self {
        let now = Instant::now();
        Self {
            count: 0,
            limit,
            window_start: now,
            window_duration,
            concurrent: 0,
            max_concurrent,
            total_count: 0,
        }
    }
    
    /// Check if action is allowed
    fn check_allowed(&mut self) -> Result<(), RateLimitError> {
        let now = Instant::now();
        
        // Reset window if expired
        if now.duration_since(self.window_start) >= self.window_duration {
            self.count = 0;
            self.window_start = now;
        }
        
        // Check concurrent limit
        if self.concurrent >= self.max_concurrent {
            return Err(RateLimitError::TooManyConcurrent {
                resource: "connections".to_string(),
                max: self.max_concurrent,
            });
        }
        
        // Check rate limit
        if self.count >= self.limit {
            let retry_after = self.window_duration - now.duration_since(self.window_start);
            return Err(RateLimitError::RateLimited {
                resource: "requests".to_string(),
                retry_after,
            });
        }
        
        // Allow action
        self.count += 1;
        self.concurrent += 1;
        self.total_count += 1;
        
        Ok(())
    }
    
    /// Decrement concurrent count (call when action completes)
    fn finish(&mut self) {
        if self.concurrent > 0 {
            self.concurrent -= 1;
        }
    }
    
    /// Get statistics
    fn stats(&self) -> RateLimitStats {
        RateLimitStats {
            current: self.count,
            limit: self.limit,
            concurrent: self.concurrent,
            max_concurrent: self.max_concurrent,
            total: self.total_count,
            window_start: self.window_start,
            time_until_reset: self.window_duration.saturating_sub(self.window_start.elapsed()),
        }
    }
}

/// Statistics for rate limiting
#[derive(Debug, Clone)]
pub struct RateLimitStats {
    pub current: u32,
    pub limit: u32,
    pub concurrent: u32,
    pub max_concurrent: u32,
    pub total: u64,
    pub window_start: Instant,
    pub time_until_reset: Duration,
}

impl RateLimitStats {
    /// Calculate percentage used
    pub fn usage_percent(&self) -> f32 {
        if self.limit == 0 {
            0.0
        } else {
            (self.current as f32 / self.limit as f32) * 100.0
        }
    }
    
    /// Check if approaching limit
    pub fn is_near_limit(&self, threshold: f32) -> bool {
        self.usage_percent() >= threshold
    }
}

/// Rate limiter for connection attempts
#[derive(Clone)]
pub struct ConnectionRateLimiter {
    /// Trackers per IP address
    trackers: Arc<RwLock<HashMap<String, RateLimitTracker>>>,
    /// Max connections per minute per IP
    connections_per_minute: u32,
    /// Max concurrent connections per IP
    max_concurrent: u32,
    /// Blocklisted IPs
    blocklist: Arc<Vec<String>>,
}

impl ConnectionRateLimiter {
    /// Create a new connection rate limiter
    pub fn new(connections_per_minute: u32, max_concurrent: u32) -> Self {
        Self {
            trackers: Arc::new(RwLock::new(HashMap::new())),
            connections_per_minute,
            max_concurrent,
            blocklist: Arc::new(vec![]),
        }
    }
    
    /// Create with blocklist
    pub fn with_blocklist(connections_per_minute: u32, max_concurrent: u32, blocklist: Vec<String>) -> Self {
        Self {
            trackers: Arc::new(RwLock::new(HashMap::new())),
            connections_per_minute,
            max_concurrent,
            blocklist: Arc::new(blocklist),
        }
    }
    
    /// Check if IP can create new connection
    pub async fn check_connection_allowed(&self, ip: &str) -> Result<(), RateLimitError> {
        // Check blocklist
        if self.blocklist.contains(&ip.to_string()) {
            return Err(RateLimitError::BlocklistedIp(ip.to_string()));
        }
        
        // Get or create tracker
        let mut trackers = self.trackers.write().await;
        let tracker = trackers.entry(ip.to_string())
            .or_insert_with(|| {
                RateLimitTracker::new(
                    self.connections_per_minute,
                    Duration::from_secs(60), // 1 minute window
                    self.max_concurrent,
                )
            });
        
        // Check if allowed
        tracker.check_allowed()
    }
    
    /// Record connection closed
    pub async fn connection_closed(&self, ip: &str) {
        let mut trackers = self.trackers.write().await;
        if let Some(tracker) = trackers.get_mut(ip) {
            tracker.finish();
        }
    }
    
    /// Clean up old trackers
    pub async fn cleanup(&self) {
        let mut trackers = self.trackers.write().await;
        let now = Instant::now();
        let inactive_threshold = Duration::from_secs(3600); // 1 hour
        
        trackers.retain(|_, tracker| {
            tracker.stats().time_until_reset > Duration::from_secs(0)
        });
        
        info!("Cleaned up {} old connection trackers", trackers.len());
    }
    
    /// Get statistics for an IP
    pub async fn get_stats(&self, ip: &str) -> Option<RateLimitStats> {
        let trackers = self.trackers.read().await;
        trackers.get(ip).map(|t| t.stats())
    }
    
    /// Check if IP is rate limited
    pub async fn is_rate_limited(&self, ip: &str) -> bool {
        self.get_stats(ip).await.map_or(false, |stats| {
            stats.current >= stats.limit
        })
    }
}

/// Rate limiter for messages per connection
pub struct MessageRateLimiter {
    /// Current window count
    count: u32,
    /// Max messages per minute
    limit: u32,
    /// Window start time
    window_start: Instant,
    /// Message size limit
    max_message_size: usize,
}

impl Default for MessageRateLimiter {
    fn default() -> Self {
        Self {
            count: 0,
            limit: 100, // Default limit
            window_start: Instant::now(),
            max_message_size: 1024 * 1024, // 1MB default
        }
    }
}

impl MessageRateLimiter {
    /// Create a new message rate limiter
    pub fn new(limit: u32, max_message_size: usize) -> Self {
        Self {
            count: 0,
            limit,
            window_start: Instant::now(),
            max_message_size,
        }
    }
    
    /// Check if message is allowed
    pub fn check_allowed(&mut self) -> Result<(), RateLimitError> {
        let now = Instant::now();
        
        // Reset window if expired
        if now.duration_since(self.window_start) >= Duration::from_secs(60) {
            self.count = 0;
            self.window_start = now;
        }
        
        // Check rate limit
        if self.count >= self.limit {
            let retry_after = Duration::from_secs(60) - now.duration_since(self.window_start);
            return Err(RateLimitError::RateLimited {
                resource: "messages".to_string(),
                retry_after,
            });
        }
        
        self.count += 1;
        Ok(())
    }
    
    /// Check message size
    pub fn check_message_size(&self, size: usize) -> Result<(), RateLimitError> {
        if size > self.max_message_size {
            return Err(RateLimitError::RateLimited {
                resource: "message_size".to_string(),
                retry_after: Duration::from_secs(0),
            });
        }
        Ok(())
    }
    
    /// Get current stats
    pub fn stats(&self) -> (u32, u32, f32) {
        let usage_percent = if self.limit == 0 {
            0.0
        } else {
            (self.count as f32 / self.limit as f32) * 100.0
        };
        (self.count, self.limit, usage_percent)
    }
    
    /// Get time until window resets
    pub fn time_until_reset(&self) -> Duration {
        let elapsed = self.window_start.elapsed();
        Duration::from_secs(60).saturating_sub(elapsed)
    }
}

/// Global rate limiter registry for managing multiple limiters
pub struct RateLimitRegistry {
    /// Connection rate limiters per IP
    connection_limiters: Arc<RwLock<HashMap<String, Arc<ConnectionRateLimiter>>>>,
    /// Message rate limiters per connection
    message_limiters: Arc<RwLock<HashMap<Uuid, Arc<std::sync::Mutex<MessageRateLimiter>>>>>,
}

impl RateLimitRegistry {
    /// Create a new rate limit registry
    pub fn new() -> Self {
        Self {
            connection_limiters: Arc::new(RwLock::new(HashMap::new())),
            message_limiters: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Get or create connection rate limiter for IP
    pub async fn get_connection_limiter(
        &self,
        ip: &str,
        config: (u32, u32, Vec<String>),
    ) -> Arc<ConnectionRateLimiter> {
        let (per_minute, max_concurrent, blocklist) = config;
        
        let limiters = self.connection_limiters.read().await;
        if let Some(limiter) = limiters.get(ip) {
            return limiter.clone();
        }
        drop(limiters);
        
        // Create new limiter
        let limiter = Arc::new(ConnectionRateLimiter::with_blocklist(
            per_minute,
            max_concurrent,
            blocklist,
        ));
        
        let mut limiters = self.connection_limiters.write().await;
        limiters.insert(ip.to_string(), limiter.clone());
        
        limiter
    }
    
    /// Get or create message rate limiter for connection
    pub async fn get_message_limiter(
        &self,
        connection_id: Uuid,
        config: (u32, usize),
    ) -> Arc<std::sync::Mutex<MessageRateLimiter>> {
        let (limit, max_size) = config;
        
        let limiters = self.message_limiters.read().await;
        if let Some(limiter) = limiters.get(&connection_id) {
            return limiter.clone();
        }
        drop(limiters);
        
        // Create new limiter
        let limiter = Arc::new(std::sync::Mutex::new(MessageRateLimiter::new(
            limit,
            max_size,
        )));
        
        let mut limiters = self.message_limiters.write().await;
        limiters.insert(connection_id, limiter.clone());
        
        limiter
    }
    
    /// Remove message limiter when connection closes
    pub async fn remove_message_limiter(&self, connection_id: Uuid) {
        let mut limiters = self.message_limiters.write().await;
        limiters.remove(&connection_id);
    }
    
    /// Clean up old limiters
    pub async fn cleanup(&self) {
        // Clean up connection limiters
        let mut conn_limiters = self.connection_limiters.write().await;
        conn_limiters.retain(|_, limiter| {
            // Keep if still has active connections
            Arc::strong_count(limiter) > 1
        });
        let removed_conns = conn_limiters.len();
        drop(conn_limiters);
        
        // Clean up message limiters for closed connections
        let mut msg_limiters = self.message_limiters.write().await;
        msg_limiters.retain(|_, limiter| {
            Arc::strong_count(limiter) > 1
        });
        let removed_msgs = msg_limiters.len();
        drop(msg_limiters);
        
        debug!(
            "RateLimitRegistry cleanup: removed {} connection limiters, {} message limiters",
            removed_conns,
            removed_msgs
        );
    }
}

impl Default for RateLimitRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_rate_limit_tracker() {
        let mut tracker = RateLimitTracker::new(
            10, // limit
            Duration::from_secs(60), // window
            5,  // max concurrent
        );
        
        // Should allow 10 requests
        for _ in 0..10 {
            assert!(tracker.check_allowed().is_ok());
        }
        
        // 11th should fail
        assert!(tracker.check_allowed().is_err());
        
        // Finish some requests
        for _ in 0..3 {
            tracker.finish();
        }
        
        // Should still be rate limited (count, not concurrent)
        assert!(tracker.check_allowed().is_err());
    }
    
    #[tokio::test]
    async fn test_connection_rate_limiter() {
        let limiter = ConnectionRateLimiter::new(5, 2, vec![]);
        let ip = "192.168.1.1";
        
        // Should allow 5 connections
        for _ in 0..5 {
            assert!(limiter.check_connection_allowed(ip).await.is_ok());
        }
        
        // 6th should fail
        assert!(limiter.check_connection_allowed(ip).await.is_err());
        
        // Close one connection
        limiter.connection_closed(ip).await;
        
        // Should allow one more (concurrent limit)
        assert!(limiter.check_connection_allowed(ip).await.is_ok());
    }
    
    #[tokio::test]
    async fn test_blocklist() {
        let limiter = ConnectionRateLimiter::with_blocklist(
            10,
            10,
            vec!["192.168.1.100".to_string()],
        );
        
        assert!(limiter.check_connection_allowed("192.168.1.1").await.is_ok());
        assert!(matches!(
            limiter.check_connection_allowed("192.168.1.100").await.unwrap_err(),
            RateLimitError::BlocklistedIp(_)
        ));
    }
    
    #[test]
    fn test_message_rate_limiter() {
        let mut limiter = MessageRateLimiter::new(5, 1024 * 1024);
        
        // Should allow 5 messages
        for _ in 0..5 {
            assert!(limiter.check_allowed().is_ok());
        }
        
        // 6th should fail
        assert!(limiter.check_allowed().is_err());
        
        // Check stats
        let (current, limit, percent) = limiter.stats();
        assert_eq!(current, 5);
        assert_eq!(limit, 5);
        assert_eq!(percent, 100.0);
    }
    
    #[test]
    fn test_message_size_check() {
        let limiter = MessageRateLimiter::new(100, 1024); // 1KB limit
        
        assert!(limiter.check_message_size(512).is_ok());
        assert!(limiter.check_message_size(2048).is_err()); // Too large
    }
    
    #[tokio::test]
    async fn test_rate_limit_registry() {
        let registry = RateLimitRegistry::new();
        let ip = "192.168.1.1";
        let conn_id = Uuid::new_v4();
        
        // Get connection limiter
        let conn_limiter = registry.get_connection_limiter(
            ip,
            (10, 5, vec![]),
        ).await;
        
        assert!(conn_limiter.check_connection_allowed(ip).await.is_ok());
        
        // Get message limiter
        let msg_limiter = registry.get_message_limiter(
            conn_id,
            (10, 1024 * 1024),
        ).await;
        
        let mut limiter = msg_limiter.lock().unwrap();
        assert!(limiter.check_allowed().is_ok());
        drop(limiter);
        
        // Cleanup
        registry.remove_message_limiter(conn_id).await;
    }
}