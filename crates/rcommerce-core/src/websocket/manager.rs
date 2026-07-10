//! WebSocket connection manager

use crate::websocket::{WebSocketConnection, ConnectionId, UserId, WebSocketMessage};
use crate::cache::{RedisPool};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::{info, error};

/// WebSocket manager for handling multiple connections
pub struct WebSocketManager {
    /// Active connections
    connections: RwLock<HashMap<ConnectionId, WebSocketConnection>>,
    
    /// Redis pool for session storage
    redis_pool: Option<RedisPool>,
}

impl WebSocketManager {
    /// Create new WebSocket manager
    pub fn new(redis_pool: Option<RedisPool>) -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            redis_pool,
        }
    }
    
    /// Add a new connection
    pub async fn add_connection(&self, connection: WebSocketConnection) {
        let conn_id = connection.id;
        self.connections.write().await.insert(conn_id, connection);
        info!("Added WebSocket connection: id={}", conn_id);
    }
    
    /// Remove a connection
    pub async fn remove_connection(&self, conn_id: &ConnectionId) -> Option<WebSocketConnection> {
        let removed = self.connections.write().await.remove(conn_id);
        if removed.is_some() {
            info!("Removed WebSocket connection: id={}", conn_id);
        }
        removed
    }
    
    /// Get connection by ID
    pub async fn get_connection(&self, conn_id: &ConnectionId) -> Option<WebSocketConnection> {
        self.connections.read().await.get(conn_id).cloned()
    }
    
    /// Get all connections for a user
    pub async fn get_user_connections(&self, user_id: &UserId) -> Vec<WebSocketConnection> {
        self.connections.read().await
            .values()
            .filter(|conn| conn.user_id == Some(*user_id))
            .cloned()
            .collect()
    }
    
    /// Get connection count
    pub async fn connection_count(&self) -> usize {
        self.connections.read().await.len()
    }
    
    /// Broadcast message to all connections
    pub async fn broadcast(&self, message: &str) -> Result<usize, Box<dyn std::error::Error>> {
        let connections = self.connections.read().await;
        let mut sent = 0;
        
        for conn in connections.values() {
            // Send message to each connection
            match conn.send(WebSocketMessage::text(message.to_string())) {
                Ok(_) => sent += 1,
                Err(e) => error!("Failed to send to connection {}: {}", conn.id, e),
            }
        }
        
        Ok(sent)
    }
}
