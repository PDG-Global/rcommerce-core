//! WebSocket connection management (Simplified stub for initial release)

use uuid::Uuid;

pub type ConnectionId = Uuid;
pub type UserId = Uuid;

#[derive(Clone, Debug)]
pub struct AuthToken {
    pub user_id: UserId,
    pub token: String,
    pub claims: serde_json::Value,
}

impl AuthToken {
    pub fn validate(_token: &str) -> Option<Self> {
        None // Stub - always invalid in initial release
    }
}

/// Stub connection for initial release
pub struct WebSocketConnection {
    #[allow(dead_code)]
    id: ConnectionId,
    user_id: Option<UserId>,
    subscriptions: Vec<String>,
}

impl WebSocketConnection {
    pub fn new(id: ConnectionId) -> Self {
        Self {
            id,
            user_id: None,
            subscriptions: vec![],
        }
    }

    pub fn set_authenticated(&mut self, user_id: UserId) {
        self.user_id = Some(user_id);
    }

    pub fn get_subscriptions(&self) -> &[String] {
        &self.subscriptions
    }

    pub fn is_authenticated(&self) -> bool {
        self.user_id.is_some()
    }
}
