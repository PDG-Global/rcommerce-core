//! WebSocket authentication and security
//!
//! This module provides authentication and security features for WebSocket connections.

use crate::{Result, Error};
use uuid::Uuid;
use std::collections::HashSet;
use tracing::{debug, warn};

/// Authentication token for WebSocket connections
#[derive(Debug, Clone)]
pub struct AuthToken {
    /// User ID
    pub user_id: Uuid,
    
    /// Token string
    pub token: String,
    
    /// Expiration time (Unix timestamp)
    pub expires_at: i64,
    
    /// Scopes/permissions
    pub scopes: Vec<String>,
}

impl AuthToken {
    /// Create a new auth token
    pub fn new(user_id: Uuid, token: String, expires_in_seconds: i64) -> Self {
        let expires_at = chrono::Utc::now().timestamp() + expires_in_seconds;
        Self {
            user_id,
            token,
            expires_at,
            scopes: vec!["websocket".to_string()],
        }
    }
    
    /// Check if token is expired
    pub fn is_expired(&self) -> bool {
        chrono::Utc::now().timestamp() > self.expires_at
    }
    
    /// Check if token has required scope
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.contains(&scope.to_string())
    }
    
    /// Validate token format
    pub fn validate(&self) -> std::result::Result<(), AuthError> {
        if self.token.is_empty() {
            return Err(AuthError::InvalidToken("Token cannot be empty".to_string()));
        }
        
        if self.token.len() < 20 {
            return Err(AuthError::InvalidToken("Token too short".to_string()));
        }
        
        if self.is_expired() {
            return Err(AuthError::TokenExpired);
        }
        
        Ok(())
    }
}

/// Validated authentication info
#[derive(Debug, Clone)]
pub struct ValidatedAuth {
    pub user_id: Uuid,
    pub connection_id: Uuid,
    pub scopes: Vec<String>,
}

/// Authentication error types
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Invalid token: {0}")]
    InvalidToken(String),
    
    #[error("Token expired")]
    TokenExpired,
    
    #[error("Missing required scope: {0}")]
    MissingScope(String),
    
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
}

impl From<AuthError> for Error {
    fn from(err: AuthError) -> Self {
        Error::Unauthorized(err.to_string())
    }
}

/// Origin validator for CSRF protection
#[derive(Debug, Clone)]
pub struct OriginValidator {
    /// Allowed origins
    allowed_origins: HashSet<String>,
    
    /// Whether to validate origins
    enabled: bool,
}

impl OriginValidator {
    /// Create a new origin validator
    pub fn new(allowed_origins: Vec<String>, enabled: bool) -> Self {
        Self {
            allowed_origins: allowed_origins.into_iter().collect(),
            enabled,
        }
    }
    
    /// Validate an origin header
    pub fn validate_origin(&self, origin: &str) -> std::result::Result<(), OriginError> {
        if !self.enabled {
            debug!("Origin validation disabled");
            return Ok(());
        }
        
        if self.allowed_origins.is_empty() {
            warn!("Origin validation enabled but no allowed origins configured");
            return Ok(()); // Allow all if none configured (but warn)
        }
        
        // Normalize origin
        let normalized = self.normalize_origin(origin);
        
        if self.allowed_origins.contains(&normalized) {
            debug!("Origin validation passed: {}", origin);
            Ok(())
        } else {
            warn!("Origin validation failed: {}", origin);
            Err(OriginError::InvalidOrigin {
                origin: origin.to_string(),
                allowed: self.allowed_origins.iter().cloned().collect(),
            })
        }
    }
    
    /// Normalize origin URL
    fn normalize_origin(&self, origin: &str) -> String {
        // Remove trailing slashes
        origin.trim_end_matches('/').to_lowercase()
    }
    
    /// Check if origin is in development mode (localhost)
    pub fn is_development_origin(&self, origin: &str) -> bool {
        origin.contains("localhost") || origin.contains("127.0.0.1") || origin.contains("::1")
    }
}

/// Origin validation errors
#[derive(Debug, thiserror::Error)]
pub enum OriginError {
    #[error("Invalid Origin: {origin}. Allowed: {allowed:?}")]
    InvalidOrigin {
        origin: String,
        allowed: Vec<String>,
    },
    
    #[error("Missing origin header")]
    MissingOrigin,
}

impl From<OriginError> for Error {
    fn from(err: OriginError) -> Self {
        Error::Validation(err.to_string())
    }
}

/// CSRF token validator
#[derive(Debug, Clone)]
pub struct CsrfValidator {
    /// Secret key for token validation
    secret: String,
}

impl CsrfValidator {
    /// Create a new CSRF validator
    pub fn new(secret: String) -> Self {
        Self { secret }
    }
    
    /// Generate a CSRF token
    pub fn generate_token(&self, user_id: &Uuid) -> String {
        use sha2::{Sha256, Digest};
        use hex::encode;
        
        let mut hasher = Sha256::new();
        hasher.update(self.secret.as_bytes());
        hasher.update(user_id.as_bytes());
        hasher.update(chrono::Utc::now().timestamp().to_string().as_bytes());
        
        encode(hasher.finalize())
    }
    
    /// Validate a CSRF token
    pub fn validate_token(&self, token: &str, user_id: &Uuid) -> bool {
        // For WebSocket, we don't validate timestamp strictly
        // Just check if it matches a recently generated token
        // In production, you'd want more sophisticated validation
        
        let expected = self.generate_token(user_id);
        // In a real implementation, you'd check against stored tokens
        // or use a time-based validation window
        
        // This is a simple placeholder
        token.len() == expected.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_auth_token() {
        let user_id = Uuid::new_v4();
        let token = AuthToken::new(user_id, "valid-token-string".to_string(), 3600);
        
        assert_eq!(token.user_id, user_id);
        assert!(!token.is_expired());
        assert!(token.has_scope("websocket"));
        assert!(token.validate().is_ok());
    }
    
    #[test]
    fn test_invalid_token() {
        let user_id = Uuid::new_v4();
        let token = AuthToken::new(user_id, "".to_string(), 3600);
        
        assert!(matches!(
            token.validate().unwrap_err(),
            AuthError::InvalidToken(_)
        ));
        
        let token = AuthToken::new(user_id, "short".to_string(), 3600);
        assert!(matches!(
            token.validate().unwrap_err(),
            AuthError::InvalidToken(_)
        ));
    }
    
    #[test]
    fn test_expired_token() {
        let user_id = Uuid::new_v4();
        let token = AuthToken::new(user_id, "valid-token-string".to_string(), -1); // Already expired
        
        assert!(token.is_expired());
        assert!(matches!(token.validate().unwrap_err(), AuthError::TokenExpired));
    }
    
    #[test]
    fn test_origin_validator() {
        let validator = OriginValidator::new(
            vec!["https://rcommerce.app".to_string()],
            true,
        );
        
        assert!(validator.validate_origin("https://rcommerce.app").is_ok());
        assert!(validator.validate_origin("https://evil.com").is_err());
        
        // Test disabled
        let disabled = OriginValidator::new(vec![], false);
        assert!(disabled.validate_origin("any-origin").is_ok());
    }
    
    #[test]
    fn test_csrf_validator() {
        let csrf = CsrfValidator::new("secret-key".to_string());
        let user_id = Uuid::new_v4();
        
        let token = csrf.generate_token(&user_id);
        assert!(!token.is_empty());
        
        // Validate (in real code, you'd have better validation)
        // This is just a smoke test
        assert!(token.len() > 0);
    }
}