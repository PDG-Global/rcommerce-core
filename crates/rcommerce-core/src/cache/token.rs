//! Token blacklist for invalidated tokens
//!
//! This module provides Redis-based token blacklisting for logout,
//! token refresh, and security revocation.

use crate::cache::{CacheResult, RedisPool};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::info;
use uuid::Uuid;

/// Blacklisted token information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlacklistedToken {
    /// Token ID
    pub token_id: Uuid,

    /// User ID
    pub user_id: Uuid,

    /// Token type (jwt, websocket, etc.)
    pub token_type: String,

    /// Revocation reason
    pub reason: String,

    /// Blacklisted timestamp
    pub blacklisted_at: i64,

    /// Expiration timestamp
    pub expires_at: i64,

    /// IP address that used the token
    pub ip_address: Option<String>,
}

impl BlacklistedToken {
    /// Create a new blacklisted token
    pub fn new(
        token_id: Uuid,
        user_id: Uuid,
        token_type: String,
        reason: String,
        expires_at: i64,
        ip_address: Option<String>,
    ) -> Self {
        Self {
            token_id,
            user_id,
            token_type,
            reason,
            blacklisted_at: chrono::Utc::now().timestamp(),
            expires_at,
            ip_address,
        }
    }

    /// Check if token is still valid (not expired)
    pub fn is_active(&self) -> bool {
        chrono::Utc::now().timestamp() < self.expires_at
    }

    /// Get time until expiration
    pub fn time_until_expiration(&self) -> Duration {
        let now = chrono::Utc::now().timestamp();
        if self.expires_at > now {
            Duration::from_secs((self.expires_at - now) as u64)
        } else {
            Duration::from_secs(0)
        }
    }
}

/// Token blacklist manager
pub struct TokenBlacklist {
    /// Redis pool
    pool: RedisPool,

    /// Default TTL for blacklisted tokens
    #[allow(dead_code)]
    default_ttl: Duration,
}

impl TokenBlacklist {
    /// Create a new token blacklist
    pub fn new(pool: RedisPool, default_ttl: Duration) -> Self {
        info!("Creating token blacklist with TTL: {:?}", default_ttl);

        Self { pool, default_ttl }
    }

    /// Blacklist a token
    pub async fn blacklist(&self, token: BlacklistedToken) -> CacheResult<()> {
        let conn = self.pool.get().await?;

        // Generate blacklist key
        let key = format!("blacklist:token:{}", token.token_id);

        // Serialize token info
        let data = serde_json::to_vec(&token)
            .map_err(|e| crate::cache::CacheError::SerializationError(e.to_string()))?;

        // Calculate TTL
        let ttl = token.time_until_expiration();
        let ttl_secs = ttl.as_secs();

        // Store in Redis with TTL
        conn.setex(&key, ttl_secs, &data).await?;

        // Also add to user's blacklist index
        let user_key = format!("blacklist:user:{}", token.user_id);
        conn.sadd(&user_key, token.token_id.to_string()).await?;
        conn.expire(&user_key, ttl_secs).await?;

        // Add to token type index
        let type_key = format!("blacklist:type:{}", token.token_type);
        conn.sadd(&type_key, token.token_id.to_string()).await?;
        conn.expire(&type_key, ttl_secs).await?;

        info!(
            "Blacklisted token: token_id={}, user_id={}, type={}, reason={}",
            token.token_id, token.user_id, token.token_type, token.reason
        );

        Ok(())
    }

    /// Check if token is blacklisted
    pub async fn is_blacklisted(&self, token_id: &Uuid) -> CacheResult<bool> {
        let conn = self.pool.get().await?;

        let key = format!("blacklist:token:{}", token_id);

        match conn.get(&key).await {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Get blacklisted token info
    pub async fn get_blacklisted_token(
        &self,
        token_id: &Uuid,
    ) -> CacheResult<Option<BlacklistedToken>> {
        let conn = self.pool.get().await?;

        let key = format!("blacklist:token:{}", token_id);

        match conn.get(&key).await? {
            Some(data) => {
                let token: BlacklistedToken = serde_json::from_slice(&data)
                    .map_err(|e| crate::cache::CacheError::DeserializationError(e.to_string()))?;

                // Check if still active
                if token.is_active() {
                    Ok(Some(token))
                } else {
                    // Clean up expired token
                    conn.del(&key).await?;
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Remove token from blacklist (unblacklist)
    pub async fn unblacklist(&self, token_id: &Uuid) -> CacheResult<bool> {
        let conn = self.pool.get().await?;

        // Get token first to clean up indexes
        if let Some(token) = self.get_blacklisted_token(token_id).await? {
            // Remove from user's index
            let user_key = format!("blacklist:user:{}", token.user_id);
            let _ = conn.srem(&user_key, token_id.to_string()).await?;

            // Remove from type index
            let type_key = format!("blacklist:type:{}", token.token_type);
            let _ = conn.srem(&type_key, token_id.to_string()).await?;
        }

        // Remove token
        let key = format!("blacklist:token:{}", token_id);
        let deleted = conn.del(&key).await?;

        if deleted {
            info!("Unblacklisted token: {}", token_id);
        }

        Ok(deleted)
    }

    /// Get all blacklisted tokens for a user
    pub async fn get_user_blacklisted_tokens(
        &self,
        user_id: &Uuid,
    ) -> CacheResult<Vec<BlacklistedToken>> {
        let conn = self.pool.get().await?;

        let user_key = format!("blacklist:user:{}", user_id);

        let token_ids: Vec<String> = conn.smembers(&user_key).await?;

        let mut tokens = Vec::new();
        for token_id_str in token_ids {
            if let Ok(token_id) = Uuid::parse_str(&token_id_str) {
                if let Some(token) = self.get_blacklisted_token(&token_id).await? {
                    tokens.push(token);
                }
            }
        }

        Ok(tokens)
    }

    /// Cleanup expired blacklist entries
    pub async fn cleanup_expired(&self) -> CacheResult<u64> {
        // This is a simplified cleanup - in production, use Redis expiration notifications
        // or a background job with SCAN

        let conn = self.pool.get().await?;

        // Find all blacklist keys
        let pattern = "blacklist:token:*".to_string();
        let keys: Vec<String> = conn.keys(&pattern).await?;

        let mut cleaned = 0;

        for key in keys {
            if let Some(token_id_str) = key.strip_prefix("blacklist:token:") {
                if let Ok(token_id) = Uuid::parse_str(token_id_str) {
                    if let Some(token) = self.get_blacklisted_token(&token_id).await? {
                        if !token.is_active() {
                            // Token expired, clean it up
                            self.unblacklist(&token_id).await?;
                            cleaned += 1;
                        }
                    }
                }
            }
        }

        if cleaned > 0 {
            info!("Cleaned up {} expired blacklisted tokens", cleaned);
        }

        Ok(cleaned)
    }

    /// Get blacklist statistics
    pub async fn stats(&self) -> CacheResult<BlacklistStats> {
        let conn = self.pool.get().await?;

        // Find all blacklist keys (this is expensive, use sparingly)
        let pattern = "blacklist:token:*".to_string();
        let keys: Vec<String> = conn.keys(&pattern).await?;

        let mut total = 0;
        let mut active = 0;
        let mut expired = 0;

        for key in keys {
            if let Some(token_id_str) = key.strip_prefix("blacklist:token:") {
                if let Ok(token_id) = Uuid::parse_str(token_id_str) {
                    if let Some(token) = self.get_blacklisted_token(&token_id).await? {
                        total += 1;
                        if token.is_active() {
                            active += 1;
                        } else {
                            expired += 1;
                        }
                    }
                }
            }
        }

        Ok(BlacklistStats {
            total_tokens: total,
            active_tokens: active,
            expired_tokens: expired,
            is_enabled: true,
        })
    }
}

/// Blacklist statistics
#[derive(Debug, Default, Clone)]
pub struct BlacklistStats {
    /// Total number of blacklisted tokens
    pub total_tokens: u64,

    /// Number of active (not expired) tokens
    pub active_tokens: u64,

    /// Number of expired tokens
    pub expired_tokens: u64,

    /// Whether blacklisting is enabled
    pub is_enabled: bool,
}

impl BlacklistStats {
    /// Format as human-readable string
    pub fn format(&self) -> String {
        if !self.is_enabled {
            return "Token blacklist: disabled".to_string();
        }

        format!(
            "Token blacklist: {} total ({} active, {} expired)",
            self.total_tokens, self.active_tokens, self.expired_tokens
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blacklisted_token() {
        let token_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let expires_at = chrono::Utc::now().timestamp() + 3600;

        let token = BlacklistedToken::new(
            token_id,
            user_id,
            "jwt".to_string(),
            "User logout".to_string(),
            expires_at,
            Some("192.168.1.1".to_string()),
        );

        assert_eq!(token.token_id, token_id);
        assert_eq!(token.user_id, user_id);
        assert!(token.is_active());

        let ttl = token.time_until_expiration();
        assert!(ttl.as_secs() > 3500); // Should be ~3600 seconds
    }

    #[test]
    fn test_blacklist_stats() {
        let stats = BlacklistStats {
            total_tokens: 100,
            active_tokens: 80,
            expired_tokens: 20,
            is_enabled: true,
        };

        let formatted = stats.format();
        assert!(formatted.contains("100 total"));
        assert!(formatted.contains("80 active"));
        assert!(formatted.contains("20 expired"));
    }

    #[test]
    fn test_expired_token() {
        let token_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let expires_at = chrono::Utc::now().timestamp() - 3600; // Expired 1 hour ago

        let token = BlacklistedToken {
            token_id,
            user_id,
            token_type: "jwt".to_string(),
            reason: "Expired".to_string(),
            blacklisted_at: chrono::Utc::now().timestamp(),
            expires_at,
            ip_address: None,
        };

        assert!(!token.is_active());
        assert_eq!(token.time_until_expiration(), Duration::from_secs(0));
    }
}
