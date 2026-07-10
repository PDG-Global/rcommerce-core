//! Middleware modules for the R Commerce API

use axum::{
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use crate::state::AppState;
use rcommerce_core::config::TlsConfig;
use rcommerce_core::services::AuthService;

pub mod api_key_auth;
pub mod scopes;

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

/// Rate limiting middleware
pub async fn rate_limit_middleware(
    State(_state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // TODO: Implement proper rate limiting with Redis
    // For now, pass through all requests
    Ok(next.run(request).await)
}

/// Admin-only middleware
pub async fn admin_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
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
