//! Middleware modules for the R Commerce API

use axum::{
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use crate::state::AppState;
use rcommerce_core::config::{RateLimitConfig, TlsConfig};
use rcommerce_core::services::AuthService;

pub mod api_key_auth;
pub mod scopes;
pub mod wc_auth;

pub use api_key_auth::{
    api_key_auth_middleware, combined_auth_middleware, ApiKeyAuth, AuthContext, JwtAuth,
};

/// Rate limiter for auth endpoints (in-memory, per-IP)
#[derive(Clone)]
pub struct AuthRateLimiter {
    /// Max attempts per window
    max_attempts: u32,
    /// Window duration in seconds
    window_secs: u64,
    /// Store: IP -> (attempts, first_attempt_time)
    store: Arc<Mutex<HashMap<String, (u32, Instant)>>>,
}

impl AuthRateLimiter {
    pub fn new(max_attempts: u32, window_secs: u64) -> Self {
        Self {
            max_attempts,
            window_secs,
            store: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Check if the request is allowed and increment counter
    pub async fn check_and_increment(&self, ip: &str) -> bool {
        let mut store = self.store.lock().await;
        let now = Instant::now();
        let window = Duration::from_secs(self.window_secs);

        match store.get_mut(ip) {
            Some((attempts, first_attempt)) => {
                // Check if window has expired
                if now.duration_since(*first_attempt) > window {
                    // Reset window
                    *attempts = 1;
                    *first_attempt = now;
                    true
                } else if *attempts < self.max_attempts {
                    // Increment attempts
                    *attempts += 1;
                    true
                } else {
                    // Rate limit exceeded
                    false
                }
            }
            None => {
                // First attempt from this IP
                store.insert(ip.to_string(), (1, now));
                true
            }
        }
    }

    /// Clean up expired entries (call periodically)
    pub async fn cleanup(&self) {
        let mut store = self.store.lock().await;
        let now = Instant::now();
        let window = Duration::from_secs(self.window_secs);
        store.retain(|_, (_, first_attempt)| now.duration_since(*first_attempt) <= window);
    }
}

/// Auth rate limiting middleware - limits login/register attempts per IP
pub async fn auth_rate_limit_middleware(
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract state from request extensions
    let state = request
        .extensions()
        .get::<Arc<AppState>>()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Extract connection info from request extensions
    let addr = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0)
        .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 0)));

    // Get client IP (use X-Forwarded-For if behind proxy, otherwise use socket addr)
    let ip = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| addr.ip().to_string());

    // Check rate limit (5 attempts per minute)
    if !state.auth_rate_limiter.check_and_increment(&ip).await {
        tracing::warn!("Auth rate limit exceeded for IP: {}", ip);
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    Ok(next.run(request).await)
}

/// Authentication middleware - validates JWT tokens
/// Adds JwtAuth to request extensions for downstream handlers
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    tracing::debug!("Auth middleware checking request");

    // Get Authorization header
    let auth_header = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let token = match auth_header {
        Some(header) => {
            tracing::debug!("Found Authorization header");
            AuthService::extract_bearer_token(header).ok_or_else(|| {
                tracing::warn!("Invalid Authorization header format");
                StatusCode::UNAUTHORIZED
            })?
        }
        None => {
            tracing::warn!("No Authorization header found");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    tracing::debug!("Extracted bearer token");

    // Verify token
    match state.auth_service.verify_token(token) {
        Ok(claims) => {
            tracing::debug!("Token verified for customer: {}", claims.sub);

            // Create JWT auth context and add to request extensions
            let auth = JwtAuth {
                customer_id: claims.sub,
                email: claims.email,
                permissions: claims.permissions,
            };

            request.extensions_mut().insert(auth);

            Ok(next.run(request).await)
        }
        Err(e) => {
            tracing::warn!("Token verification failed: {}", e);
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

/// Optional authentication middleware - authenticates if a valid token is provided,
/// but does not fail if no token is present. Useful for endpoints that support both
/// guest and authenticated customers (e.g. checkout).
pub async fn optional_auth_middleware(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    // Try to authenticate from Authorization header, but ignore failures
    if let Some(header) = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
    {
        if let Some(token) = AuthService::extract_bearer_token(header) {
            if let Ok(claims) = state.auth_service.verify_token(token) {
                let auth = JwtAuth {
                    customer_id: claims.sub,
                    email: claims.email,
                    permissions: claims.permissions,
                };
                request.extensions_mut().insert(Some(auth));
                return next.run(request).await;
            }
        }
    }

    request.extensions_mut().insert::<Option<JwtAuth>>(None);
    next.run(request).await
}

/// Outcome of a rate limit check for a single client
pub enum RateLimitCheck {
    /// Request allowed; carries the number of requests remaining in the window
    Allowed(u32),
    /// Client IP is on the blocklist
    Blocked,
    /// Rate limit exceeded; carries the seconds until the client may retry
    Limited(u64),
}

/// In-memory sliding-window rate limiter (per client IP)
///
/// Tracks request timestamps per client key over a fixed window and rejects
/// requests once `max_requests` have been recorded within the window.
/// Not shared across server instances; see `[rate_limiting] use_redis` in the
/// configuration for the distributed variant (not yet implemented).
#[derive(Clone)]
pub struct RateLimiter {
    max_requests: u32,
    window: Duration,
    allowlist: Vec<String>,
    blocklist: Vec<String>,
    expose_headers: bool,
    store: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
}

impl RateLimiter {
    /// Create a limiter from the `[rate_limiting]` configuration section
    pub fn new(config: &RateLimitConfig) -> Self {
        Self {
            max_requests: config.requests_per_minute.max(1),
            window: Duration::from_secs(60),
            allowlist: config.allowlist.clone(),
            blocklist: config.blocklist.clone(),
            expose_headers: config.expose_headers,
            store: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Max requests allowed per window (for X-RateLimit-Limit header)
    pub fn max_requests(&self) -> u32 {
        self.max_requests
    }

    /// Whether to expose X-RateLimit-* headers on responses
    pub fn expose_headers(&self) -> bool {
        self.expose_headers
    }

    /// Check whether a request from `key` (a client IP) is allowed
    pub async fn check(&self, key: &str) -> RateLimitCheck {
        if self.blocklist.iter().any(|ip| ip == key) {
            return RateLimitCheck::Blocked;
        }
        if self.allowlist.iter().any(|ip| ip == key) {
            return RateLimitCheck::Allowed(self.max_requests);
        }

        let mut store = self.store.lock().await;
        let now = Instant::now();

        let timestamps = store.entry(key.to_string()).or_default();
        timestamps.retain(|t| now.duration_since(*t) < self.window);

        if timestamps.len() as u32 >= self.max_requests {
            let oldest = timestamps[0];
            let retry_after = self
                .window
                .saturating_sub(now.duration_since(oldest))
                .as_secs()
                + 1;
            return RateLimitCheck::Limited(retry_after);
        }

        timestamps.push(now);
        RateLimitCheck::Allowed(self.max_requests - timestamps.len() as u32)
    }

    /// Drop entries with no activity inside the current window
    pub async fn cleanup(&self) {
        let mut store = self.store.lock().await;
        let now = Instant::now();
        store.retain(|_, timestamps| {
            timestamps.retain(|t| now.duration_since(*t) < self.window);
            !timestamps.is_empty()
        });
    }
}

/// Resolve the client IP, honoring X-Forwarded-For when behind a reverse proxy
fn client_ip(request: &Request<Body>) -> String {
    let addr = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0)
        .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 0)));

    request
        .headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| addr.ip().to_string())
}

/// Rate limiting middleware - enforces the `[rate_limiting]` config per client IP
pub async fn rate_limit_middleware(
    State(limiter): State<RateLimiter>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let ip = client_ip(&request);

    match limiter.check(&ip).await {
        RateLimitCheck::Allowed(remaining) => {
            let mut response = next.run(request).await;
            if limiter.expose_headers() {
                let headers = response.headers_mut();
                if let Ok(v) = HeaderValue::from_str(&limiter.max_requests().to_string()) {
                    headers.insert("X-RateLimit-Limit", v);
                }
                if let Ok(v) = HeaderValue::from_str(&remaining.to_string()) {
                    headers.insert("X-RateLimit-Remaining", v);
                }
            }
            response
        }
        RateLimitCheck::Blocked => {
            tracing::warn!("Rate limiter: blocked IP attempted access: {}", ip);
            StatusCode::FORBIDDEN.into_response()
        }
        RateLimitCheck::Limited(retry_after) => {
            tracing::warn!("Rate limit exceeded for IP: {}", ip);
            let mut response = (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "error": "rate_limit_exceeded",
                    "message": "Too many requests. Please retry later.",
                    "retry_after": retry_after,
                })),
            )
                .into_response();
            if let Ok(v) = HeaderValue::from_str(&retry_after.to_string()) {
                response.headers_mut().insert("Retry-After", v);
            }
            response
        }
    }
}

/// Admin-only middleware
pub async fn admin_middleware(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Get Authorization header
    let auth_header = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let token = match auth_header {
        Some(header) => {
            tracing::debug!("Admin middleware: Found Authorization header");
            AuthService::extract_bearer_token(header).ok_or(StatusCode::UNAUTHORIZED)?
        }
        None => {
            tracing::debug!("Admin middleware: No Authorization header found");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // Verify token
    let claims = state
        .auth_service
        .verify_token(token)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Check for admin permission
    if !claims.permissions.contains(&"admin".to_string()) {
        return Err(StatusCode::FORBIDDEN);
    }

    // Admin handlers read the caller from the request, as behind auth_middleware
    request.extensions_mut().insert(JwtAuth {
        customer_id: claims.sub,
        email: claims.email,
        permissions: claims.permissions,
    });

    Ok(next.run(request).await)
}

/// Security headers middleware
///
/// This middleware adds security headers to all responses:
/// - X-Content-Type-Options: nosniff
/// - X-Frame-Options: DENY
/// - X-XSS-Protection: 1; mode=block
/// - Referrer-Policy: strict-origin-when-cross-origin
/// - Strict-Transport-Security (when TLS is enabled)
pub async fn security_headers_middleware(
    State(tls_config): State<Option<TlsConfig>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    // Prevent MIME type sniffing
    headers.insert(
        "X-Content-Type-Options",
        HeaderValue::from_static("nosniff"),
    );

    // Prevent clickjacking
    headers.insert("X-Frame-Options", HeaderValue::from_static("DENY"));

    // XSS protection (legacy but still useful)
    headers.insert(
        "X-XSS-Protection",
        HeaderValue::from_static("1; mode=block"),
    );

    // Referrer policy
    headers.insert(
        "Referrer-Policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );

    // HSTS (only when TLS is enabled)
    if let Some(tls_config) = tls_config {
        if tls_config.enabled {
            let hsts_value = tls_config
                .hsts
                .as_ref()
                .map(|h| h.header_value())
                .unwrap_or_else(|| "max-age=31536000; includeSubDomains".to_string());

            if let Ok(value) = HeaderValue::from_str(&hsts_value) {
                headers.insert("Strict-Transport-Security", value);
            }
        }
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(requests_per_minute: u32) -> RateLimitConfig {
        RateLimitConfig {
            requests_per_minute,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_allows_within_limit() {
        let limiter = RateLimiter::new(&test_config(3));

        let mut remaining_expectations = [2u32, 1, 0];
        for expected in remaining_expectations.iter_mut() {
            match limiter.check("10.0.0.1").await {
                RateLimitCheck::Allowed(remaining) => assert_eq!(remaining, *expected),
                RateLimitCheck::Blocked => panic!("expected Allowed, got Blocked"),
                RateLimitCheck::Limited(_) => panic!("expected Allowed, got Limited"),
            }
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_blocks_over_limit() {
        let limiter = RateLimiter::new(&test_config(2));

        assert!(matches!(
            limiter.check("10.0.0.1").await,
            RateLimitCheck::Allowed(_)
        ));
        assert!(matches!(
            limiter.check("10.0.0.1").await,
            RateLimitCheck::Allowed(_)
        ));
        match limiter.check("10.0.0.1").await {
            RateLimitCheck::Limited(retry_after) => {
                assert!(retry_after > 0 && retry_after <= 60);
            }
            _ => panic!("expected Limited"),
        }

        // A different IP is not affected
        assert!(matches!(
            limiter.check("10.0.0.2").await,
            RateLimitCheck::Allowed(_)
        ));
    }

    #[tokio::test]
    async fn test_rate_limiter_blocklist_and_allowlist() {
        let config = RateLimitConfig {
            requests_per_minute: 1,
            blocklist: vec!["192.0.2.1".to_string()],
            allowlist: vec!["192.0.2.2".to_string()],
            ..Default::default()
        };
        let limiter = RateLimiter::new(&config);

        assert!(matches!(
            limiter.check("192.0.2.1").await,
            RateLimitCheck::Blocked
        ));

        // Allowlisted IP is never limited, even past the request limit
        assert!(matches!(
            limiter.check("192.0.2.2").await,
            RateLimitCheck::Allowed(_)
        ));
        assert!(matches!(
            limiter.check("192.0.2.2").await,
            RateLimitCheck::Allowed(_)
        ));
    }

    #[tokio::test]
    async fn test_rate_limiter_cleanup() {
        let limiter = RateLimiter::new(&test_config(1));

        assert!(matches!(
            limiter.check("10.0.0.1").await,
            RateLimitCheck::Allowed(_)
        ));
        assert_eq!(limiter.store.lock().await.len(), 1);

        // Simulate an expired window by backdating the stored timestamp
        {
            let mut store = limiter.store.lock().await;
            store
                .get_mut("10.0.0.1")
                .unwrap()
                .iter_mut()
                .for_each(|t| *t = *t - Duration::from_secs(120));
        }

        limiter.cleanup().await;
        assert_eq!(limiter.store.lock().await.len(), 0);

        // IP can request again after cleanup
        assert!(matches!(
            limiter.check("10.0.0.1").await,
            RateLimitCheck::Allowed(_)
        ));
    }
}
