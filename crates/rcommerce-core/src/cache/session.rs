//! WebSocket session storage using Redis
//!
//! This module provides persistent storage for WebSocket connections
//! using Redis, enabling session restoration after disconnections
//! and horizontal scaling.

use crate::cache::{CacheError, CacheResult, RedisPool, WebSocketSessionConfig};
use crate::websocket::{ConnectionId, UserId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tracing::{debug, info};
use uuid::Uuid;

/// WebSocket session data stored in Redis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketSession {
    /// Connection ID
    pub connection_id: ConnectionId,

    /// User ID (if authenticated)
    pub user_id: Option<UserId>,

    /// Client IP address
    pub client_ip: String,

    /// Connected timestamp (Unix)
    pub connected_at: i64,

    /// Last activity timestamp (Unix)
    pub last_activity: i64,

    /// Is authenticated
    pub is_authenticated: bool,

    /// Subscribed topics
    pub subscriptions: HashSet<String>,

    /// Metadata
    pub metadata: serde_json::Value,

    /// Version for optimistic locking
    pub version: u32,
}

impl WebSocketSession {
    /// Create a new session
    pub fn new(connection_id: ConnectionId, client_ip: String) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            connection_id,
            user_id: None,
            client_ip,
            connected_at: now,
            last_activity: now,
            is_authenticated: false,
            subscriptions: HashSet::new(),
            metadata: serde_json::json!({}),
            version: 1,
        }
    }

    /// Update last activity timestamp
    pub fn update_activity(&mut self) {
        self.last_activity = chrono::Utc::now().timestamp();
    }

    /// Set authentication
    pub fn set_authenticated(&mut self, user_id: UserId) {
        self.user_id = Some(user_id);
        self.is_authenticated = true;
        self.update_activity();
    }

    /// Subscribe to a topic
    pub fn subscribe(&mut self, topic: String) {
        self.subscriptions.insert(topic);
        self.update_activity();
    }

    /// Unsubscribe from a topic
    pub fn unsubscribe(&mut self, topic: &str) {
        self.subscriptions.remove(topic);
        self.update_activity();
    }

    /// Check if subscribed to a topic
    pub fn is_subscribed(&self, topic: &str) -> bool {
        self.subscriptions.contains(topic)
    }

    /// Compute session score for cleanup (lower is older/staler)
    pub fn compute_score(&self) -> i64 {
        // Last activity + (inverse of version) as tiebreaker
        self.last_activity - (self.version as i64)
    }
}

/// WebSocket session store
pub struct SessionStore {
    /// Redis pool
    pool: RedisPool,

    /// Configuration
    config: WebSocketSessionConfig,

    /// Namespace for keys
    namespace: String,
}

impl SessionStore {
    /// Create a new session store
    pub async fn new(pool: RedisPool, config: WebSocketSessionConfig) -> Self {
        info!("Creating WebSocket session store");

        Self {
            pool,
            config,
            namespace: "ws:session".to_string(),
        }
    }

    /// Save a session
    pub async fn save(&self, session: &WebSocketSession) -> CacheResult<()> {
        if !self.config.enabled {
            debug!("WebSocket session storage disabled");
            return Ok(());
        }

        let conn = self.pool.get().await?;

        // Serialize session
        let data = serde_json::to_vec(session)
            .map_err(|e| CacheError::SerializationError(e.to_string()))?;

        // Generate key
        let key = self.session_key(&session.connection_id);

        // Save with TTL
        conn.setex(&key, self.config.session_ttl_secs, &data)
            .await?;

        debug!(
            "Saved session: key={}, connection_id={}",
            key, session.connection_id
        );

        // Also save to user's sessions if authenticated
        if let Some(user_id) = session.user_id {
            let user_key = self.user_sessions_key(&user_id);
            conn.sadd(&user_key, session.connection_id.to_string())
                .await?;
            conn.expire(&user_key, self.config.session_ttl_secs).await?;
        }

        // Save to IP's sessions
        let ip_key = self.ip_sessions_key(&session.client_ip);
        conn.sadd(&ip_key, session.connection_id.to_string())
            .await?;
        conn.expire(&ip_key, self.config.session_ttl_secs).await?;

        Ok(())
    }

    /// Load a session by connection ID
    pub async fn load(
        &self,
        connection_id: &ConnectionId,
    ) -> CacheResult<Option<WebSocketSession>> {
        if !self.config.enabled {
            return Ok(None);
        }

        let conn = self.pool.get().await?;
        let key = self.session_key(connection_id);

        match conn.get(&key).await? {
            Some(data) => {
                let session: WebSocketSession = serde_json::from_slice(&data)
                    .map_err(|e| CacheError::DeserializationError(e.to_string()))?;

                debug!(
                    "Loaded session: key={}, connection_id={}",
                    key, connection_id
                );
                Ok(Some(session))
            }
            None => Ok(None),
        }
    }

    /// Delete a session
    pub async fn delete(&self, connection_id: &ConnectionId) -> CacheResult<bool> {
        if !self.config.enabled {
            return Ok(false);
        }

        let conn = self.pool.get().await?;

        // Load session first to clean up indexes
        if let Some(session) = self.load(connection_id).await? {
            // Remove from user's sessions
            if let Some(user_id) = session.user_id {
                let user_key = self.user_sessions_key(&user_id);
                let _ = conn.srem(&user_key, connection_id.to_string()).await?;
            }

            // Remove from IP's sessions
            let ip_key = self.ip_sessions_key(&session.client_ip);
            let _ = conn.srem(&ip_key, connection_id.to_string()).await?;
        }

        // Delete session
        let key = self.session_key(connection_id);
        let deleted = conn.del(&key).await?;

        debug!(
            "Deleted session: key={}, connection_id={}",
            key, connection_id
        );

        Ok(deleted)
    }

    /// Get all sessions for a user
    pub async fn get_user_sessions(&self, user_id: &UserId) -> CacheResult<Vec<WebSocketSession>> {
        if !self.config.enabled {
            return Ok(vec![]);
        }

        let conn = self.pool.get().await?;
        let user_key = self.user_sessions_key(user_id);

        // Get all connection IDs for the user
        let connection_ids: Vec<String> = conn.smembers(&user_key).await?;

        // Load each session
        let mut sessions = Vec::new();
        for conn_id_str in connection_ids {
            if let Ok(conn_id) = Uuid::parse_str(&conn_id_str) {
                if let Some(session) = self.load(&conn_id).await? {
                    sessions.push(session);
                }
            }
        }

        debug!("Loaded {} sessions for user: {}", sessions.len(), user_id);

        Ok(sessions)
    }

    /// Get all sessions for an IP
    pub async fn get_ip_sessions(&self, ip: &str) -> CacheResult<Vec<WebSocketSession>> {
        if !self.config.enabled {
            return Ok(vec![]);
        }

        let conn = self.pool.get().await?;
        let ip_key = self.ip_sessions_key(ip);

        // Get all connection IDs for the IP
        let connection_ids: Vec<String> = conn.smembers(&ip_key).await?;

        // Load each session
        let mut sessions = Vec::new();
        for conn_id_str in connection_ids {
            if let Ok(conn_id) = Uuid::parse_str(&conn_id_str) {
                if let Some(session) = self.load(&conn_id).await? {
                    sessions.push(session);
                }
            }
        }

        debug!("Loaded {} sessions for IP: {}", sessions.len(), ip);

        Ok(sessions)
    }

    /// Get all active session IDs
    pub async fn get_all_session_ids(&self) -> CacheResult<Vec<ConnectionId>> {
        if !self.config.enabled {
            return Ok(vec![]);
        }

        // Note: This is expensive, use sparingly
        // In production, use SCAN instead of KEYS

        let conn = self.pool.get().await?;
        let pattern = format!("{}:*", self.namespace);

        let keys: Vec<String> = conn.keys(&pattern).await?;

        let mut session_ids = Vec::new();
        for key in keys {
            if let Some(conn_id_str) = key.strip_prefix(&format!("{}/", self.namespace)) {
                if let Ok(conn_id) = Uuid::parse_str(conn_id_str) {
                    session_ids.push(conn_id);
                }
            }
        }

        Ok(session_ids)
    }

    /// Cleanup expired sessions
    pub async fn cleanup_expired(&self) -> CacheResult<u64> {
        if !self.config.enabled {
            return Ok(0);
        }

        let session_ids = self.get_all_session_ids().await?;
        let mut cleaned = 0;

        for session_id in session_ids {
            if let Some(session) = self.load(&session_id).await? {
                // Check if expired based on TTL
                let ttl = (chrono::Utc::now().timestamp() - session.last_activity).abs();
                if ttl > self.config.session_ttl_secs as i64 {
                    self.delete(&session_id).await?;
                    cleaned += 1;
                }
            }
        }

        if cleaned > 0 {
            info!("Cleaned up {} expired WebSocket sessions", cleaned);
        }

        Ok(cleaned)
    }

    /// Get session statistics
    pub async fn stats(&self) -> CacheResult<SessionStats> {
        if !self.config.enabled {
            return Ok(SessionStats::default());
        }

        let session_ids = self.get_all_session_ids().await?;
        let mut total = 0;
        let mut authenticated = 0;
        let mut subscriptions = 0;

        for session_id in session_ids {
            if let Some(session) = self.load(&session_id).await? {
                total += 1;
                if session.is_authenticated {
                    authenticated += 1;
                }
                subscriptions += session.subscriptions.len();
            }
        }

        Ok(SessionStats {
            total_sessions: total,
            authenticated_sessions: authenticated,
            total_subscriptions: subscriptions,
            is_enabled: true,
        })
    }

    /// Generate session key
    fn session_key(&self, connection_id: &ConnectionId) -> String {
        format!("{}/{}", self.namespace, connection_id)
    }

    /// Generate user sessions key
    fn user_sessions_key(&self, user_id: &UserId) -> String {
        format!("{}/user:{}/sessions", self.namespace, user_id)
    }

    /// Generate IP sessions key
    fn ip_sessions_key(&self, ip: &str) -> String {
        format!("{}/ip:{}/sessions", self.namespace, ip)
    }
}

/// Session statistics
#[derive(Debug, Default, Clone)]
pub struct SessionStats {
    /// Total number of sessions
    pub total_sessions: usize,

    /// Number of authenticated sessions
    pub authenticated_sessions: usize,

    /// Total number of subscriptions across all sessions
    pub total_subscriptions: usize,

    /// Whether session caching is enabled
    pub is_enabled: bool,
}

impl SessionStats {
    /// Format as human-readable string
    pub fn format(&self) -> String {
        if !self.is_enabled {
            return "Session cache: disabled".to_string();
        }

        format!(
            "Session cache: {} sessions ({} authenticated), {} subscriptions",
            self.total_sessions, self.authenticated_sessions, self.total_subscriptions
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_session() {
        let conn_id = ConnectionId::new_v4();
        let mut session = WebSocketSession::new(conn_id, "192.168.1.1".to_string());

        assert_eq!(session.connection_id, conn_id);
        assert!(!session.is_authenticated);
        assert_eq!(session.subscriptions.len(), 0);

        // Authenticate
        let user_id = UserId::new_v4();
        session.set_authenticated(user_id);
        assert!(session.is_authenticated);
        assert_eq!(session.user_id, Some(user_id));

        // Subscribe to topics
        session.subscribe("orders".to_string());
        session.subscribe("inventory".to_string());
        assert_eq!(session.subscriptions.len(), 2);
        assert!(session.is_subscribed("orders"));

        // Unsubscribe
        session.unsubscribe("orders");
        assert_eq!(session.subscriptions.len(), 1);
        assert!(!session.is_subscribed("orders"));
    }

    #[test]
    fn test_session_stats() {
        let stats = SessionStats {
            total_sessions: 10,
            authenticated_sessions: 5,
            total_subscriptions: 25,
            is_enabled: true,
        };

        let formatted = stats.format();
        assert!(formatted.contains("10 sessions"));
        assert!(formatted.contains("5 authenticated"));
        assert!(formatted.contains("25 subscriptions"));
    }
}
