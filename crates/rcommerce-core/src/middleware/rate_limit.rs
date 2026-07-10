//! Rate limiting middleware for API endpoint protection
//!
//! This module provides configurable rate limiting to prevent API abuse,
//! DDoS attacks, and ensure fair usage across clients.

use crate::{Error, Result};
use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

/// Configuration for rate limiting
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Enable rate limiting
    pub enabled: bool,

    /// Maximum requests per minute from a single IP
    pub requests_per_minute: u32,

    /// Maximum requests per hour from a single IP
    pub requests_per_hour: u32,

    /// Maximum requests per day from a single IP
    pub requests_per_day: u32,

    /// Maximum concurrent connections per IP
    pub max_concurrent_per_ip: u32,

    /// Enable API key rate limiting (more permissive)
    pub api_key_limiting: bool,

    /// Maximum requests per minute with valid API key
    pub api_key_requests_per_minute: u32,

    /// Blocklist of IP addresses
    pub blocklist: Vec<String>,

    /// Allowlist of trusted IP addresses
    pub allowlist: Vec<String>,

    /// Enable DDoS protection mode (stricter limits under attack)
    pub ddos_protection: bool,

    /// Response headers to include rate limit info
    pub expose_headers: bool,

    /// Store rate limit data in Redis (true) or memory (false)
    pub use_redis: bool,

    /// Redis connection string
    pub redis_url: Option<String>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            requests_per_minute: 60,
            requests_per_hour: 1000,
            requests_per_day: 10000,
            max_concurrent_per_ip: 10,
            api_key_limiting: true,
            api_key_requests_per_minute: 1000,
            blocklist: vec![],
            allowlist: vec![],
            ddos_protection: true,
            expose_headers: true,
            use_redis: false,
            redis_url: None,
        }
    }
}

/// Rate limit tracker for a single identifier (IP or API key)
#[derive(Debug, Clone)]
struct RateLimitTracker {
    /// Request counts per time window
    minute_count: u32,
    hour_count: u32,
    day_count: u32,

    /// When the current windows started
    minute_window_start: Instant,
    hour_window_start: Instant,
    day_window_start: Instant,

    /// Concurrent request count
    concurrent_count: u32,

    /// Total requests ever made (for analytics)
    total_count: u64,

    /// First request time (for analytics)
    first_request: Instant,

    /// Last request time
    last_request: Instant,

    /// Is this client currently rate limited?
    is_limited: bool,

    /// When the rate limit expires
    limit_expires_at: Option<Instant>,
}

impl RateLimitTracker {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            minute_count: 0,
            hour_count: 0,
            day_count: 0,
            minute_window_start: now,
            hour_window_start: now,
            day_window_start: now,
            concurrent_count: 0,
            total_count: 0,
            first_request: now,
            last_request: now,
            is_limited: false,
            limit_expires_at: None,
        }
    }

    /// Record a new request and check if rate limited
    fn check_request(
        &mut self,
        config: &RateLimitConfig,
    ) -> std::result::Result<(), RateLimitError> {
        let now = Instant::now();

        // Reset windows if they've expired
        if now.duration_since(self.minute_window_start) >= Duration::from_secs(60) {
            self.minute_count = 0;
            self.minute_window_start = now;
        }

        if now.duration_since(self.hour_window_start) >= Duration::from_secs(3600) {
            self.hour_count = 0;
            self.hour_window_start = now;
        }

        if now.duration_since(self.day_window_start) >= Duration::from_secs(86400) {
            self.day_count = 0;
            self.day_window_start = now;
        }

        // Check if currently rate limited
        if self.is_limited {
            if let Some(expires_at) = self.limit_expires_at {
                if now < expires_at {
                    return Err(RateLimitError::RateLimited {
                        retry_after: expires_at.duration_since(now).as_secs(),
                    });
                } else {
                    // Rate limit expired, reset
                    self.is_limited = false;
                    self.limit_expires_at = None;
                }
            }
        }

        // Check rate limits
        if self.minute_count >= config.requests_per_minute {
            self.set_rate_limit(now, Duration::from_secs(60));
            return Err(RateLimitError::RateLimited { retry_after: 60 });
        }

        if self.hour_count >= config.requests_per_hour {
            self.set_rate_limit(now, Duration::from_secs(3600));
            return Err(RateLimitError::RateLimited { retry_after: 3600 });
        }

        if self.day_count >= config.requests_per_day {
            self.set_rate_limit(now, Duration::from_secs(86400));
            return Err(RateLimitError::RateLimited { retry_after: 86400 });
        }

        if self.concurrent_count >= config.max_concurrent_per_ip {
            return Err(RateLimitError::TooManyConcurrent);
        }

        // Record the request
        self.minute_count += 1;
        self.hour_count += 1;
        self.day_count += 1;
        self.concurrent_count += 1;
        self.total_count += 1;
        self.last_request = now;

        Ok(())
    }

    /// Set rate limit status
    fn set_rate_limit(&mut self, now: Instant, duration: Duration) {
        self.is_limited = true;
        self.limit_expires_at = Some(now + duration);
    }

    /// Mark request as completed (reduce concurrent count)
    fn finish_request(&mut self) {
        if self.concurrent_count > 0 {
            self.concurrent_count -= 1;
        }
    }

    /// Get rate limit headers
    fn get_headers(&self) -> Vec<(&'static str, String)> {
        let mut headers = vec![];

        // X-RateLimit-Limit: Maximum requests per minute
        headers.push(("X-RateLimit-Limit", self.minute_count.to_string()));

        // X-RateLimit-Remaining: Requests remaining in current window
        let remaining = 60u32.saturating_sub(self.minute_count);
        headers.push(("X-RateLimit-Remaining", remaining.to_string()));

        // X-RateLimit-Reset: Unix timestamp when window resets
        let reset_timestamp = self.minute_window_start.elapsed().as_secs() + 60;
        headers.push(("X-RateLimit-Reset", reset_timestamp.to_string()));

        // Retry-After: Seconds to wait before retrying (if rate limited)
        if self.is_limited {
            if let Some(expires_at) = self.limit_expires_at {
                let retry_after = expires_at.duration_since(Instant::now()).as_secs();
                headers.push(("Retry-After", retry_after.to_string()));
            }
        }

        headers
    }
}

/// Rate limiting error types
#[derive(Debug, Clone)]
pub enum RateLimitError {
    /// Client is rate limited
    RateLimited {
        /// Seconds to wait before retrying
        retry_after: u64,
    },
    /// Too many concurrent requests
    TooManyConcurrent,
    /// IP address is blocked
    IpBlocked,
    /// DDoS protection activated
    DDoSProtectionActive,
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RateLimitError::RateLimited { retry_after } => {
                write!(
                    f,
                    "Rate limit exceeded. Retry after {} seconds.",
                    retry_after
                )
            }
            RateLimitError::TooManyConcurrent => {
                write!(f, "Too many concurrent requests. Please wait.")
            }
            RateLimitError::IpBlocked => {
                write!(f, "IP address is blocked.")
            }
            RateLimitError::DDoSProtectionActive => {
                write!(f, "DDoS protection is active. Requests restricted.")
            }
        }
    }
}

impl std::error::Error for RateLimitError {}

/// In-memory rate limiter
#[derive(Clone)]
pub struct RateLimiter {
    config: RateLimitConfig,
    trackers: Arc<RwLock<HashMap<String, RateLimitTracker>>>,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            trackers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a request is allowed
    pub async fn check_request(
        &self,
        identifier: &str,
        is_api_key: bool,
    ) -> Result<Vec<(&'static str, String)>> {
        // Check blocklist first
        if self.config.blocklist.contains(&identifier.to_string()) {
            return Err(Error::RateLimit(RateLimitError::IpBlocked));
        }

        // Check allowlist (skip rate limiting)
        if self.config.allowlist.contains(&identifier.to_string()) {
            return Ok(vec![]);
        }

        // Apply API key rate limits if applicable
        let effective_config = if is_api_key && self.config.api_key_limiting {
            RateLimitConfig {
                requests_per_minute: self.config.api_key_requests_per_minute,
                ..self.config.clone()
            }
        } else {
            self.config.clone()
        };

        // Get or create tracker
        let mut trackers = self.trackers.write().await;
        let tracker = trackers
            .entry(identifier.to_string())
            .or_insert_with(RateLimitTracker::new);

        // Check the request
        match tracker.check_request(&effective_config) {
            Ok(()) => {
                let headers = if self.config.expose_headers {
                    tracker.get_headers()
                } else {
                    vec![]
                };
                Ok(headers)
            }
            Err(RateLimitError::RateLimited { retry_after }) => {
                Err(Error::RateLimit(RateLimitError::RateLimited {
                    retry_after,
                }))
            }
            Err(RateLimitError::TooManyConcurrent) => {
                Err(Error::RateLimit(RateLimitError::TooManyConcurrent))
            }
            Err(RateLimitError::IpBlocked) => Err(Error::RateLimit(RateLimitError::IpBlocked)),
            Err(RateLimitError::DDoSProtectionActive) => {
                Err(Error::RateLimit(RateLimitError::DDoSProtectionActive))
            }
        }
    }

    /// Record request completion
    pub async fn finish_request(&self, identifier: &str) {
        let mut trackers = self.trackers.write().await;
        if let Some(tracker) = trackers.get_mut(identifier) {
            tracker.finish_request();
        }
    }

    /// Clean up old trackers
    pub async fn cleanup(&self) {
        let mut trackers = self.trackers.write().await;
        let now = std::time::Instant::now();
        let inactive_threshold = Duration::from_secs(3600); // 1 hour

        trackers.retain(|_, tracker| now.duration_since(tracker.last_request) < inactive_threshold);
    }

    /// Get current statistics for an identifier
    pub async fn get_stats(&self, identifier: &str) -> Option<RateLimitStats> {
        let trackers = self.trackers.read().await;
        trackers.get(identifier).map(|tracker| RateLimitStats {
            total_requests: tracker.total_count,
            current_minute: tracker.minute_count,
            current_hour: tracker.hour_count,
            current_day: tracker.day_count,
            concurrent_requests: tracker.concurrent_count,
            first_request: tracker.first_request,
            last_request: tracker.last_request,
            is_rate_limited: tracker.is_limited,
        })
    }
}

/// Rate limit statistics
#[derive(Debug, Clone)]
pub struct RateLimitStats {
    pub total_requests: u64,
    pub current_minute: u32,
    pub current_hour: u32,
    pub current_day: u32,
    pub concurrent_requests: u32,
    pub first_request: Instant,
    pub last_request: Instant,
    pub is_rate_limited: bool,
}

/// Rate limiting middleware
///
/// This middleware applies rate limiting to all incoming requests.
/// It tracks requests per IP address and optionally per API key.
pub async fn rate_limit_middleware(
    State(rate_limiter): State<RateLimiter>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    request: Request,
    next: Next,
) -> Result<Response> {
    let ip = addr.ip().to_string();

    // Check if request has API key (from header or query param)
    let has_api_key = check_for_api_key(&request);

    // Check rate limit
    match rate_limiter.check_request(&ip, has_api_key).await {
        Ok(headers) => {
            // Clone the rate limiter for post-request cleanup
            let rate_limiter_clone = rate_limiter.clone();
            let ip_clone = ip.clone();

            // Process request
            let mut response = next.run(request).await;

            // Add rate limit headers
            for (key, value) in headers {
                let header_name: axum::http::HeaderName = key.parse().unwrap();
                let header_value: axum::http::HeaderValue = value.parse().unwrap();
                response.headers_mut().insert(header_name, header_value);
            }

            // Record request completion
            rate_limiter_clone.finish_request(&ip_clone).await;

            Ok(response)
        }
        Err(e) => {
            // Convert to appropriate HTTP error
            let (status, body) = match e {
                Error::RateLimit(rate_err) => {
                    let status = match rate_err {
                        RateLimitError::IpBlocked => StatusCode::FORBIDDEN,
                        RateLimitError::DDoSProtectionActive => StatusCode::SERVICE_UNAVAILABLE,
                        _ => StatusCode::TOO_MANY_REQUESTS,
                    };
                    (status, rate_err.to_string())
                }
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal error".to_string(),
                ),
            };

            Err(Error::HttpError(status, body))
        }
    }
}

/// Check if request contains an API key
pub fn check_for_api_key(request: &Request) -> bool {
    // Check Authorization header
    if let Some(auth_header) = request.headers().get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if auth_str.starts_with("Bearer ") || auth_str.starts_with("ApiKey ") {
                return true;
            }
        }
    }

    // Check X-API-Key header
    if request.headers().contains_key("x-api-key") {
        return true;
    }

    // Could also check query parameters if needed
    // For now, just check headers

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert!(config.enabled);
        assert_eq!(config.requests_per_minute, 60);
        assert_eq!(config.requests_per_hour, 1000);
        assert_eq!(config.requests_per_day, 10000);
    }

    #[test]
    fn test_rate_limit_tracker() {
        let config = RateLimitConfig::default();
        let mut tracker = RateLimitTracker::new();

        // First request should succeed
        assert!(tracker.check_request(&config).is_ok());
        assert_eq!(tracker.minute_count, 1);
        assert_eq!(tracker.concurrent_count, 1);
        assert_eq!(tracker.total_count, 1);

        // Finish the request
        tracker.finish_request();
        assert_eq!(tracker.concurrent_count, 0);
    }

    #[test]
    fn test_rate_limit_exceeded() {
        let config = RateLimitConfig {
            requests_per_minute: 2, // Very low limit for testing
            ..Default::default()
        };

        let mut tracker = RateLimitTracker::new();

        // First two requests should succeed
        assert!(tracker.check_request(&config).is_ok());
        assert!(tracker.check_request(&config).is_ok());

        // Third request should fail
        let result = tracker.check_request(&config);
        assert!(result.is_err());

        match result.unwrap_err() {
            RateLimitError::RateLimited { retry_after } => {
                assert_eq!(retry_after, 60);
            }
            _ => panic!("Expected RateLimited error"),
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_basic() {
        let config = RateLimitConfig::default();
        let limiter = RateLimiter::new(config);

        let result = limiter.check_request("192.168.1.1", false).await;
        assert!(result.is_ok());

        let headers = result.unwrap();
        assert!(!headers.is_empty());

        // Check for expected headers
        let header_names: Vec<_> = headers.iter().map(|(k, _)| *k).collect();
        assert!(header_names.contains(&"X-RateLimit-Limit"));
        assert!(header_names.contains(&"X-RateLimit-Remaining"));
        assert!(header_names.contains(&"X-RateLimit-Reset"));
    }

    #[test]
    fn test_blocklist() {
        let config = RateLimitConfig {
            blocklist: vec!["192.168.1.100".to_string()],
            enabled: true,
            ..Default::default()
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let limiter = RateLimiter::new(config);
            let result = limiter.check_request("192.168.1.100", false).await;

            assert!(result.is_err());
            match result.unwrap_err() {
                Error::RateLimit(RateLimitError::IpBlocked) => {
                    // Expected
                }
                _ => panic!("Expected IpBlocked error"),
            }
        });
    }

    #[test]
    fn test_check_for_api_key() {
        // Test with Bearer token
        let request = Request::builder()
            .uri("/test")
            .header("authorization", "Bearer secret-token")
            .body(axum::body::Body::empty())
            .unwrap();
        assert!(check_for_api_key(&request));

        // Test with ApiKey
        let request = Request::builder()
            .uri("/test")
            .header("authorization", "ApiKey secret-token")
            .body(axum::body::Body::empty())
            .unwrap();
        assert!(check_for_api_key(&request));

        // Test with X-API-Key header
        let request = Request::builder()
            .uri("/test")
            .header("x-api-key", "secret-token")
            .body(axum::body::Body::empty())
            .unwrap();
        assert!(check_for_api_key(&request));

        // Test without API key
        let request = Request::builder()
            .uri("/test")
            .body(axum::body::Body::empty())
            .unwrap();
        assert!(!check_for_api_key(&request));
    }
}
