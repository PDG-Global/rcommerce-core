//! API Key Authentication Middleware
//!
//! This middleware validates API keys from the Authorization header
//! and checks their scopes against required permissions.
//!
//! API keys are expected in the format: `Authorization: Bearer <prefix>.<secret>`
//! or simply: `Authorization: <prefix>.<secret>`

use axum::{
    body::Body, extract::Request, http::StatusCode, middleware::Next, response::Response, Extension,
};
use std::sync::Arc;

use rcommerce_core::{
    repository::{ApiKeyRepository, PostgresApiKeyRepository},
    services::{Action, AuthService, Resource, ScopeChecker},
};

/// Client IP recorded on the API key's `last_used` audit column.
///
/// Prefers the `ClientIp` extension inserted by the trusted-proxy-aware
/// client-IP middleware. Without it, falls back to the raw TCP peer — never
/// to a forwarding header, which a client can forge at will.
fn audit_client_ip(request: &Request<Body>) -> Option<String> {
    use axum::extract::ConnectInfo;
    if let Some(client_ip) = request.extensions().get::<crate::middleware::ClientIp>() {
        return Some(client_ip.ip().to_string());
    }
    request
        .extensions()
        .get::<ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0.ip().to_string())
}

/// API key authentication result
#[derive(Debug, Clone)]
pub struct ApiKeyAuth {
    pub key_id: uuid::Uuid,
    pub customer_id: Option<uuid::Uuid>,
    pub scopes: Vec<String>,
    pub name: String,
}

impl ApiKeyAuth {
    /// Check if this API key has permission for a specific resource and action
    pub fn can(&self, resource: Resource, action: Action) -> bool {
        match ScopeChecker::new(&self.scopes) {
            Ok(checker) => checker.can(resource, action),
            Err(_) => false,
        }
    }

    /// Check if this API key has read access to a resource
    pub fn can_read(&self, resource: Resource) -> bool {
        self.can(resource, Action::Read)
    }

    /// Check if this API key has write access to a resource
    pub fn can_write(&self, resource: Resource) -> bool {
        self.can(resource, Action::Write)
    }

    /// Check if this API key has admin access
    pub fn is_admin(&self) -> bool {
        match ScopeChecker::new(&self.scopes) {
            Ok(checker) => checker.is_admin(),
            Err(_) => false,
        }
    }
}

/// API key authentication middleware
///
/// Validates the API key from the Authorization header and adds
/// the ApiKeyAuth to the request extensions for downstream handlers.
///
/// Expected header format:
/// - `Authorization: Bearer <prefix>.<secret>` (standard Bearer format)
/// - `Authorization: <prefix>.<secret>` (direct key format)
pub async fn api_key_auth_middleware(
    Extension(repo): Extension<Arc<PostgresApiKeyRepository>>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Get Authorization header
    let auth_header = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let auth_header = match auth_header {
        Some(header) => header,
        None => {
            tracing::debug!("API key auth: No Authorization header found");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // Extract the API key (handle both "Bearer <key>" and "<key>" formats)
    let api_key = extract_api_key(auth_header);

    let api_key = match api_key {
        Some(key) => key,
        None => {
            tracing::debug!("API key auth: Invalid Authorization header format");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // Validate the API key
    let key_record = match repo.verify_key(&api_key).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            tracing::warn!("API key auth: Invalid or revoked API key");
            return Err(StatusCode::UNAUTHORIZED);
        }
        Err(e) => {
            tracing::error!("API key auth: Database error: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Get client IP for the audit trail
    let client_ip = audit_client_ip(&request);

    // Update last used timestamp (fire and forget)
    let repo_clone = repo.clone();
    let key_id = key_record.id;
    let ip_for_update = client_ip.clone();
    tokio::spawn(async move {
        if let Err(e) = repo_clone
            .update_last_used(key_id, ip_for_update.as_deref())
            .await
        {
            tracing::warn!("Failed to update API key last_used: {}", e);
        }
    });

    tracing::debug!(
        "API key auth: Key '{}' authenticated successfully with scopes: {:?}",
        key_record.name,
        key_record.scopes
    );

    // Create auth context and add to request extensions
    let auth = ApiKeyAuth {
        key_id: key_record.id,
        customer_id: key_record.customer_id,
        scopes: key_record.scopes,
        name: key_record.name,
    };

    request.extensions_mut().insert(auth);

    Ok(next.run(request).await)
}

/// Extract API key from Authorization header
///
/// Handles formats:
/// - `Bearer <prefix>.<secret>`
/// - `<prefix>.<secret>`
fn extract_api_key(auth_header: &str) -> Option<String> {
    // Try Bearer token format first
    if let Some(key) = AuthService::extract_bearer_token(auth_header) {
        // Check if it looks like an API key (has exactly one dot, not JWT format)
        let parts: Vec<&str> = key.split('.').collect();
        if parts.len() == 2 && !looks_like_jwt(key) {
            return Some(key.to_string());
        }
    }

    // Try direct key format (no "Bearer" prefix)
    let parts: Vec<&str> = auth_header.split('.').collect();
    if parts.len() == 2 && !looks_like_jwt(auth_header) {
        return Some(auth_header.to_string());
    }

    None
}

/// Check if a string looks like a JWT token
///
/// JWT tokens have 3 parts separated by dots (header.payload.signature)
/// and are base64url encoded
fn looks_like_jwt(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();

    // JWT has exactly 3 parts
    if parts.len() != 3 {
        return false;
    }

    // Each part should be non-empty and contain only base64url characters
    for part in parts {
        if part.is_empty() {
            return false;
        }
        // Base64url uses A-Z, a-z, 0-9, -, _
        // JWT parts might also have padding =
        if !part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '=')
        {
            return false;
        }
    }

    true
}

/// Combined authentication middleware that supports both JWT and API keys
///
/// This middleware tries to authenticate using:
/// 1. API key if the Authorization header contains a key in format `prefix.secret`
/// 2. JWT token if the Authorization header contains a Bearer token
///
/// The appropriate auth context is added to request extensions.
pub async fn combined_auth_middleware(
    Extension(repo): Extension<Arc<PostgresApiKeyRepository>>,
    Extension(auth_service): Extension<Arc<rcommerce_core::services::AuthService>>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Get Authorization header
    let auth_header = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let auth_header = match auth_header {
        Some(header) => header,
        None => {
            tracing::debug!("Combined auth: No Authorization header found");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // Try to extract as API key first
    if let Some(api_key) = extract_api_key(auth_header) {
        tracing::debug!("Combined auth: Attempting API key authentication");

        match repo.verify_key(&api_key).await {
            Ok(Some(record)) => {
                // Get client IP for the audit trail
                let client_ip = audit_client_ip(&request);

                // Update last used timestamp (fire and forget)
                let repo_clone = repo.clone();
                let key_id = record.id;
                let ip_for_update = client_ip;
                tokio::spawn(async move {
                    if let Err(e) = repo_clone
                        .update_last_used(key_id, ip_for_update.as_deref())
                        .await
                    {
                        tracing::warn!("Failed to update API key last_used: {}", e);
                    }
                });

                // Create auth context
                let key_name = record.name.clone();
                let auth = ApiKeyAuth {
                    key_id: record.id,
                    customer_id: record.customer_id,
                    scopes: record.scopes.clone(),
                    name: record.name,
                };

                request.extensions_mut().insert(auth);

                tracing::debug!(
                    "Combined auth: API key '{}' authenticated with scopes: {:?}",
                    key_name,
                    record.scopes
                );

                return Ok(next.run(request).await);
            }
            Ok(None) => {
                // Not a valid API key, try JWT below
                tracing::debug!("Combined auth: API key not found, trying JWT");
            }
            Err(e) => {
                tracing::error!("Combined auth: Database error: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }

    // Try JWT authentication
    if let Some(token) = AuthService::extract_bearer_token(auth_header) {
        tracing::debug!("Combined auth: Attempting JWT authentication");

        match auth_service.verify_access_token(token) {
            Ok(claims) => {
                // Create JWT auth context
                let auth = JwtAuth {
                    customer_id: claims.sub,
                    email: claims.email,
                    permissions: claims.permissions,
                };

                request.extensions_mut().insert(auth);

                tracing::debug!(
                    "Combined auth: JWT authenticated for customer: {}",
                    claims.sub
                );

                return Ok(next.run(request).await);
            }
            Err(e) => {
                tracing::warn!("Combined auth: JWT verification failed: {}", e);
                return Err(StatusCode::UNAUTHORIZED);
            }
        }
    }

    tracing::warn!("Combined auth: No valid authentication found");
    Err(StatusCode::UNAUTHORIZED)
}

/// JWT authentication context (for combined auth)
#[derive(Debug, Clone)]
pub struct JwtAuth {
    pub customer_id: uuid::Uuid,
    pub email: String,
    pub permissions: Vec<String>,
}

impl JwtAuth {
    /// Check if this JWT has permission for a specific resource and action
    pub fn can(&self, resource: Resource, action: Action) -> bool {
        match ScopeChecker::new(&self.permissions) {
            Ok(checker) => checker.can(resource, action),
            Err(_) => false,
        }
    }

    /// Check if this JWT has read access to a resource
    pub fn can_read(&self, resource: Resource) -> bool {
        self.can(resource, Action::Read)
    }

    /// Check if this JWT has write access to a resource
    pub fn can_write(&self, resource: Resource) -> bool {
        self.can(resource, Action::Write)
    }

    /// Check if this JWT has admin access
    pub fn is_admin(&self) -> bool {
        match ScopeChecker::new(&self.permissions) {
            Ok(checker) => checker.is_admin(),
            Err(_) => false,
        }
    }
}

/// Authentication context enum that can hold either API key or JWT auth
#[derive(Debug, Clone)]
pub enum AuthContext {
    ApiKey(ApiKeyAuth),
    Jwt(JwtAuth),
}

impl AuthContext {
    /// Get the customer ID if available
    pub fn customer_id(&self) -> Option<uuid::Uuid> {
        match self {
            AuthContext::ApiKey(auth) => auth.customer_id,
            AuthContext::Jwt(auth) => Some(auth.customer_id),
        }
    }

    /// Check if this auth has permission for a specific resource and action
    pub fn can(&self, resource: Resource, action: Action) -> bool {
        match self {
            AuthContext::ApiKey(auth) => auth.can(resource, action),
            AuthContext::Jwt(auth) => auth.can(resource, action),
        }
    }

    /// Check if this auth has read access to a resource
    pub fn can_read(&self, resource: Resource) -> bool {
        self.can(resource, Action::Read)
    }

    /// Check if this auth has write access to a resource
    pub fn can_write(&self, resource: Resource) -> bool {
        self.can(resource, Action::Write)
    }

    /// Check if this auth has admin access
    pub fn is_admin(&self) -> bool {
        match self {
            AuthContext::ApiKey(auth) => auth.is_admin(),
            AuthContext::Jwt(auth) => auth.is_admin(),
        }
    }

    /// Get scopes/permissions
    pub fn scopes(&self) -> &[String] {
        match self {
            AuthContext::ApiKey(auth) => &auth.scopes,
            AuthContext::Jwt(auth) => &auth.permissions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_api_key() {
        // Bearer format
        assert_eq!(
            extract_api_key("Bearer abc123.def456"),
            Some("abc123.def456".to_string())
        );

        // Direct format
        assert_eq!(
            extract_api_key("abc123.def456"),
            Some("abc123.def456".to_string())
        );

        // JWT should not be extracted as API key
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        assert_eq!(extract_api_key(&format!("Bearer {}", jwt)), None);
        assert_eq!(extract_api_key(jwt), None);

        // Invalid formats
        assert_eq!(extract_api_key("Bearer token"), None);
        assert_eq!(extract_api_key("invalid"), None);
    }

    #[test]
    fn test_looks_like_jwt() {
        // Valid JWT format
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        assert!(looks_like_jwt(jwt));

        // API key format (not JWT)
        assert!(!looks_like_jwt("abc123.def456"));

        // Single part
        assert!(!looks_like_jwt("token"));

        // Four parts
        assert!(!looks_like_jwt("a.b.c.d"));
    }

    #[test]
    fn test_api_key_auth_can() {
        let auth = ApiKeyAuth {
            key_id: uuid::Uuid::new_v4(),
            customer_id: None,
            scopes: vec!["products:read".to_string(), "orders:write".to_string()],
            name: "Test Key".to_string(),
        };

        assert!(auth.can_read(Resource::Products));
        assert!(!auth.can_write(Resource::Products));
        assert!(auth.can_read(Resource::Orders));
        assert!(auth.can_write(Resource::Orders));
        assert!(!auth.can_read(Resource::Customers));
    }

    #[test]
    fn test_jwt_auth_can() {
        let auth = JwtAuth {
            customer_id: uuid::Uuid::new_v4(),
            email: "test@example.com".to_string(),
            permissions: vec!["read".to_string(), "write".to_string()],
        };

        assert!(auth.can_read(Resource::Products));
        assert!(auth.can_write(Resource::Products));
        assert!(auth.can_read(Resource::Orders));
        assert!(auth.can_write(Resource::Orders));
    }
}
