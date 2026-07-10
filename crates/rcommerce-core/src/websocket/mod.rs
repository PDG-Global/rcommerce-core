//! WebSocket module for real-time features
//!
//! NOTE: Full implementation stubbed for initial release.
//! Core product/order/customer functionality works without this.

pub mod broadcast;
pub mod config;
pub mod connection;
pub mod message;
pub mod pubsub;

pub use config::WebSocketConfig;
pub use message::{MessageCategory, MessagePayload, MessageType, WebSocketMessage};

use uuid::Uuid;

/// Connection ID type
pub type ConnectionId = Uuid;

/// User ID type  
pub type UserId = Uuid;

/// Topic type for subscriptions
pub type Topic = String;

/// Stub WebSocket manager for initial release
#[derive(Clone)]
pub struct WebSocketManager;

impl WebSocketManager {
    pub fn new(_config: WebSocketConfig) -> Self {
        Self
    }

    pub async fn run(&self) {
        // Stub - does nothing in initial release
        tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
    }
}

/// Stub connection handler
pub struct WebSocketConnection;

impl WebSocketConnection {
    pub fn set_authenticated(&mut self, _user_id: UserId) {
        // Stub
    }

    pub fn get_subscriptions(&self) -> Vec<String> {
        vec![]
    }
}

/// Stub broadcast local
#[derive(Clone)]
pub struct BroadcastLocal;

impl BroadcastLocal {
    pub fn subscriber_count(&self, _topic: &str) -> usize {
        0
    }

    pub fn broadcast_to_topic(&self, _topic: &str, _message: WebSocketMessage) -> usize {
        0
    }

    pub fn receiver(&self) -> tokio::sync::mpsc::UnboundedReceiver<WebSocketMessage> {
        let (_, rx) = tokio::sync::mpsc::unbounded_channel();
        rx
    }
}

/// Stub subscription
pub struct Subscription {
    pub id: Uuid,
    pub topic: String,
    pub connection_id: ConnectionId,
}

impl Subscription {
    pub fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }
}

/// Stub subscribe error
#[derive(Debug)]
pub enum SubscribeError {
    NotAuthenticated,
}

/// Stub message rate limiter
#[derive(Default)]
pub struct MessageRateLimiter;

/// Rate limit check result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitResult {
    /// Rate limit check passed
    Allowed,
    /// Rate limit exceeded
    Limited,
}

impl MessageRateLimiter {
    pub fn new() -> Self {
        Self
    }

    pub fn check_rate_limit(&self, _msg_type: MessageType) -> RateLimitResult {
        RateLimitResult::Allowed
    }
}

// Re-export types that are used elsewhere
pub use connection::AuthToken;
