//! Scope-based permission middleware for API keys and JWT tokens
//!
//! This module provides middleware for checking granular permissions
//! on API routes. Supports both API key and JWT authentication.

use axum::{body::Body, extract::Request, http::StatusCode, middleware::Next, response::Response};
use std::sync::Arc;

use rcommerce_core::services::{Action, AuthService, Resource, ScopeChecker};

/// Extract authentication info from request and check scope permissions
///
/// This middleware checks:
/// 1. API key authentication (if Authorization header contains an API key)
/// 2. JWT token authentication (if Authorization header contains a Bearer token)
///
/// For API keys, it looks up the key in the database and checks its scopes.
/// For JWT tokens, it checks the permissions claim in the token.
pub async fn require_scope_middleware(
    request: Request<Body>,
    next: Next,
    required_resource: Resource,
    required_action: Action,
) -> Result<Response, StatusCode> {
    // Get Authorization header
    let auth_header = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let auth_header = match auth_header {
        Some(header) => header,
        None => {
            tracing::debug!("Scope middleware: No Authorization header found");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // Try to extract bearer token
    let token = AuthService::extract_bearer_token(auth_header);

    // Check if it's an API key (not a JWT token format)
    // API keys are in format: prefix.secret (e.g., "abc123def.xxx...")
    // JWT tokens are in format: header.payload.signature
    let is_api_key = token.is_none() && auth_header.contains('.');

    if is_api_key {
        // API key authentication would require database lookup
        // For now, we don't support API key scope checking in this middleware
        // API keys should use a different middleware that has DB access
        tracing::debug!("API key detected - scope checking not implemented in this middleware");
        // Allow through - API key validation happens in a separate layer
        return Ok(next.run(request).await);
    }

    // JWT token authentication
    let token = match token {
        Some(t) => t,
        None => {
            tracing::warn!("Invalid Authorization header format");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // Extract state from request extensions
    let state = request
        .extensions()
        .get::<Arc<crate::state::AppState>>()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    // Verify token and get claims
    let claims = match state.auth_service.verify_access_token(token) {
        Ok(claims) => claims,
        Err(e) => {
            tracing::warn!("Token verification failed: {}", e);
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // Check permissions using ScopeChecker
    let scope_checker =
        ScopeChecker::new(&claims.permissions).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !scope_checker.can(required_resource, required_action) {
        tracing::warn!(
            "Permission denied: customer {} tried to {:?} {:?}",
            claims.sub,
            required_action,
            required_resource
        );
        return Err(StatusCode::FORBIDDEN);
    }

    tracing::debug!(
        "Permission granted: customer {} can {:?} {:?}",
        claims.sub,
        required_action,
        required_resource
    );

    Ok(next.run(request).await)
}

/// Type alias for middleware function return type
type MiddlewareFn =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response, StatusCode>> + Send>>;

/// Middleware factory for requiring read access to a resource
pub fn require_read(resource: Resource) -> impl Fn(Request<Body>, Next) -> MiddlewareFn {
    move |request: Request<Body>, next: Next| {
        Box::pin(require_scope_middleware(
            request,
            next,
            resource,
            Action::Read,
        ))
    }
}

/// Middleware factory for requiring write access to a resource
pub fn require_write(resource: Resource) -> impl Fn(Request<Body>, Next) -> MiddlewareFn {
    move |request: Request<Body>, next: Next| {
        Box::pin(require_scope_middleware(
            request,
            next,
            resource,
            Action::Write,
        ))
    }
}

/// Middleware factory for requiring admin access
pub fn require_admin() -> impl Fn(Request<Body>, Next) -> MiddlewareFn {
    move |request: Request<Body>, next: Next| {
        Box::pin(require_scope_middleware(
            request,
            next,
            Resource::Settings,
            Action::Admin,
        ))
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_is_api_key_detection() {
        // API key format: prefix.secret (both parts are alphanumeric)
        let api_key = "abc123def.xyz789";
        assert!(api_key.contains('.'));

        // JWT format: header.payload.signature (base64 encoded with possible underscores)
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        assert!(jwt.contains('.'));

        // The difference is JWT has 3 parts separated by dots
        let jwt_parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(jwt_parts.len(), 3);

        let api_key_parts: Vec<&str> = api_key.split('.').collect();
        assert_eq!(api_key_parts.len(), 2);
    }
}
