//! WebSocket message types with strong typing
//!
//! This module defines all WebSocket message types used in the system.
//! Each message type is strongly typed and validated.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

/// Type of WebSocket message
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    /// Connection established
    Connect,

    /// Authentication request/response
    Auth,

    /// Ping/pong keep-alive
    Ping,
    Pong,

    /// Text message
    Text,

    /// Subscribe to topic
    Subscribe,

    /// Unsubscribe from topic
    Unsubscribe,

    /// Order update notification
    OrderUpdate,

    /// Inventory change notification
    InventoryUpdate,

    /// Payment status update
    PaymentUpdate,

    /// Customer notification
    CustomerNotification,

    /// Admin broadcast
    AdminBroadcast,

    /// Error message
    Error,

    /// Success confirmation
    Success,

    /// Custom application message
    Custom,
}

impl MessageType {
    /// Get message category for rate limiting
    pub fn category(&self) -> MessageCategory {
        match self {
            MessageType::Connect | MessageType::Auth => MessageCategory::Control,
            MessageType::Ping | MessageType::Pong => MessageCategory::KeepAlive,
            MessageType::Text => MessageCategory::Application,
            MessageType::Subscribe | MessageType::Unsubscribe => MessageCategory::Subscription,
            MessageType::OrderUpdate
            | MessageType::InventoryUpdate
            | MessageType::PaymentUpdate
            | MessageType::CustomerNotification => MessageCategory::Notification,
            MessageType::AdminBroadcast => MessageCategory::Broadcast,
            MessageType::Error | MessageType::Success => MessageCategory::System,
            MessageType::Custom => MessageCategory::Application,
        }
    }

    /// Check if message requires authentication
    pub fn requires_auth(&self) -> bool {
        matches!(
            self,
            MessageType::Subscribe | MessageType::Unsubscribe | MessageType::Custom
        )
    }

    /// Check if message is rate limited
    pub fn is_rate_limited(&self) -> bool {
        !matches!(
            self.category(),
            MessageCategory::Control | MessageCategory::KeepAlive | MessageCategory::System
        )
    }
}

/// Message category for rate limiting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageCategory {
    Control,      // Connection/auth
    KeepAlive,    // Ping/pong
    Subscription, // Subscribe/unsubscribe
    Notification, // Order/inventory/payment updates
    Broadcast,    // Admin broadcasts
    System,       // Errors/success
    Application,  // Custom messages
}

/// WebSocket message with strong typing and validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketMessage {
    /// Message type identifier
    pub message_type: MessageType,

    /// Unique message ID for correlation
    pub message_id: Uuid,

    /// Timestamp when message was created
    pub timestamp: DateTime<Utc>,

    /// Message payload (type-specific)
    pub payload: MessagePayload,
}

impl WebSocketMessage {
    /// Create a new message
    pub fn new(message_type: MessageType, payload: MessagePayload) -> Self {
        Self {
            message_type,
            message_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            payload,
        }
    }

    /// Create a ping message
    pub fn ping() -> Self {
        Self::new(MessageType::Ping, MessagePayload::Ping)
    }

    /// Create a pong message
    pub fn pong() -> Self {
        Self::new(MessageType::Pong, MessagePayload::Pong)
    }

    /// Create a text message
    pub fn text(content: impl Into<String>) -> Self {
        Self::new(
            MessageType::Text,
            MessagePayload::Text {
                content: content.into(),
            },
        )
    }

    /// Create an error message
    pub fn error(code: String, message: String) -> Self {
        Self::new(
            MessageType::Error,
            MessagePayload::Error {
                code,
                message,
                timestamp: Utc::now(),
            },
        )
    }

    /// Create a success confirmation
    pub fn success(operation: String, details: JsonValue) -> Self {
        Self::new(
            MessageType::Success,
            MessagePayload::Success {
                operation,
                details,
                timestamp: Utc::now(),
            },
        )
    }

    /// Create auth request
    pub fn auth_request(token: String) -> Self {
        Self::new(MessageType::Auth, MessagePayload::AuthRequest { token })
    }

    /// Create auth response
    pub fn auth_response(success: bool, user_id: Option<Uuid>, message: Option<String>) -> Self {
        Self::new(
            MessageType::Auth,
            MessagePayload::AuthResponse {
                success,
                user_id,
                message,
            },
        )
    }

    /// Create subscribe request
    pub fn subscribe(topic: String) -> Self {
        Self::new(MessageType::Subscribe, MessagePayload::Subscribe { topic })
    }

    /// Create unsubscribe request
    pub fn unsubscribe(topic: String) -> Self {
        Self::new(
            MessageType::Unsubscribe,
            MessagePayload::Unsubscribe { topic },
        )
    }

    /// Create order update notification
    pub fn order_update(order_id: Uuid, status: String, details: JsonValue) -> Self {
        Self::new(
            MessageType::OrderUpdate,
            MessagePayload::OrderUpdate {
                order_id,
                status,
                details,
                timestamp: Utc::now(),
            },
        )
    }

    /// Create inventory update notification
    pub fn inventory_update(product_id: Uuid, stock: i32, variant: Option<String>) -> Self {
        Self::new(
            MessageType::InventoryUpdate,
            MessagePayload::InventoryUpdate {
                product_id,
                stock,
                variant,
                timestamp: Utc::now(),
            },
        )
    }

    /// Validate message content
    pub fn validate(&self) -> std::result::Result<(), ValidationError> {
        // Check message size
        let size = self.estimated_size();
        if size > 1024 * 1024 {
            // 1MB max
            return Err(ValidationError::MessageTooLarge(size));
        }

        // Validate payload based on type
        match (&self.message_type, &self.payload) {
            (MessageType::Auth, MessagePayload::AuthRequest { token }) => {
                if token.is_empty() {
                    return Err(ValidationError::MissingField("token"));
                }
            }
            (MessageType::Subscribe, MessagePayload::Subscribe { topic }) => {
                if topic.is_empty() {
                    return Err(ValidationError::MissingField("topic"));
                }
                if topic.len() > 200 {
                    return Err(ValidationError::FieldTooLong("topic", 200));
                }
            }
            (MessageType::Custom, MessagePayload::Custom { data }) => {
                if data.to_string().len() > 1024 * 100 {
                    // 100KB for custom
                    return Err(ValidationError::MessageTooLarge(data.to_string().len()));
                }
            }
            _ => {} // Other combinations are valid
        }

        Ok(())
    }

    /// Calculate estimated message size in bytes
    pub fn estimated_size(&self) -> usize {
        let payload_size = match &self.payload {
            MessagePayload::Empty => 0,
            MessagePayload::Ping | MessagePayload::Pong => 4, // "ping" or "pong"
            MessagePayload::Text { content } => content.len(),
            MessagePayload::Error { code, message, .. } => code.len() + message.len(),
            MessagePayload::Success {
                operation, details, ..
            } => {
                let details_str = details.to_string();
                operation.len() + details_str.len()
            }
            MessagePayload::AuthRequest { token } => token.len(),
            MessagePayload::AuthResponse { message, .. } => message.as_ref().map_or(0, |m| m.len()),
            MessagePayload::Subscribe { topic } | MessagePayload::Unsubscribe { topic } => {
                topic.len()
            }
            MessagePayload::OrderUpdate {
                status, details, ..
            } => status.len() + details.to_string().len(),
            MessagePayload::InventoryUpdate { variant, .. } => {
                variant.as_ref().map_or(0, |v| v.len()) + 32 // UUID + stock
            }
            MessagePayload::PaymentUpdate { status, amount, .. } => {
                status.len() + amount.to_string().len() + 16 // + UUID
            }
            MessagePayload::CustomerNotification {
                message_type,
                message,
                data,
                ..
            } => {
                message_type.len() + message.len() + data.to_string().len() + 16
                // + UUID
            }
            MessagePayload::AdminBroadcast { message, data, .. } => {
                message.len() + data.to_string().len()
            }
            MessagePayload::Custom { data } => data.to_string().len(),
        };

        // Base overhead: UUID + timestamp + type
        16 + 8 + 4 + payload_size
    }

    /// Check if this is a high-priority message
    pub fn is_high_priority(&self) -> bool {
        matches!(
            self.message_type,
            MessageType::Connect
                | MessageType::Error
                | MessageType::Auth
                | MessageType::Ping
                | MessageType::Pong
        )
    }
}

/// Payload for WebSocket messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum MessagePayload {
    /// Empty payload (for ping/pong)
    Empty,

    /// Ping keep-alive
    Ping,

    /// Pong keep-alive response
    Pong,

    /// Text message
    Text { content: String },

    /// Error response
    Error {
        code: String,
        message: String,
        timestamp: DateTime<Utc>,
    },

    /// Success confirmation
    Success {
        operation: String,
        details: JsonValue,
        timestamp: DateTime<Utc>,
    },

    /// Authentication request
    AuthRequest { token: String },

    /// Authentication response
    AuthResponse {
        success: bool,
        user_id: Option<Uuid>,
        message: Option<String>,
    },

    /// Subscribe to topic
    Subscribe { topic: String },

    /// Unsubscribe from topic
    Unsubscribe { topic: String },

    /// Order status update
    OrderUpdate {
        order_id: Uuid,
        status: String,
        details: JsonValue,
        timestamp: DateTime<Utc>,
    },

    /// Inventory change notification
    InventoryUpdate {
        product_id: Uuid,
        stock: i32,
        variant: Option<String>,
        timestamp: DateTime<Utc>,
    },

    /// Payment status update
    PaymentUpdate {
        payment_id: Uuid,
        status: String,
        amount: JsonValue,
        timestamp: DateTime<Utc>,
    },

    /// Customer notification
    CustomerNotification {
        notification_id: Uuid,
        message_type: String,
        message: String,
        data: JsonValue,
        timestamp: DateTime<Utc>,
    },

    /// Admin broadcast
    AdminBroadcast {
        message: String,
        data: JsonValue,
        timestamp: DateTime<Utc>,
    },

    /// Custom application message
    Custom { data: JsonValue },
}

impl MessagePayload {
    /// Get message type name
    pub fn type_name(&self) -> &'static str {
        match self {
            MessagePayload::Empty => "empty",
            MessagePayload::Ping => "ping",
            MessagePayload::Pong => "pong",
            MessagePayload::Text { .. } => "text",
            MessagePayload::Error { .. } => "error",
            MessagePayload::Success { .. } => "success",
            MessagePayload::AuthRequest { .. } => "auth_request",
            MessagePayload::AuthResponse { .. } => "auth_response",
            MessagePayload::Subscribe { .. } => "subscribe",
            MessagePayload::Unsubscribe { .. } => "unsubscribe",
            MessagePayload::OrderUpdate { .. } => "order_update",
            MessagePayload::InventoryUpdate { .. } => "inventory_update",
            MessagePayload::PaymentUpdate { .. } => "payment_update",
            MessagePayload::CustomerNotification { .. } => "customer_notification",
            MessagePayload::AdminBroadcast { .. } => "admin_broadcast",
            MessagePayload::Custom { .. } => "custom",
        }
    }
}

/// Message validation errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum ValidationError {
    #[error("Message too large: {0} bytes (max 1MB)")]
    MessageTooLarge(usize),

    #[error("Missing required field: {0}")]
    MissingField(&'static str),

    #[error("Field too long: {0} (max {1} characters)")]
    FieldTooLong(&'static str, usize),

    #[error("Invalid message format: {0}")]
    InvalidFormat(String),

    #[error("Invalid message type for payload")]
    InvalidType,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_types() {
        assert_eq!(MessageType::Ping.category(), MessageCategory::KeepAlive);
        assert!(!MessageType::Auth.requires_auth());
        assert!(MessageType::Subscribe.requires_auth());
        assert!(!MessageType::Ping.is_rate_limited());
        assert!(MessageType::Custom.is_rate_limited());
    }

    #[test]
    fn test_message_creation() {
        let msg = WebSocketMessage::ping();
        assert_eq!(msg.message_type, MessageType::Ping);
        assert!(matches!(msg.payload, MessagePayload::Ping));

        let msg = WebSocketMessage::order_update(
            Uuid::new_v4(),
            "shipped".to_string(),
            serde_json::json!({"tracking": "12345"}),
        );
        assert_eq!(msg.message_type, MessageType::OrderUpdate);
    }

    #[test]
    fn test_message_validation() {
        let msg = WebSocketMessage::auth_request("valid-token".to_string());
        assert!(msg.validate().is_ok());

        let msg = WebSocketMessage::auth_request("".to_string());
        assert!(matches!(
            msg.validate().unwrap_err(),
            ValidationError::MissingField("token")
        ));
    }

    #[test]
    fn test_message_size() {
        let msg = WebSocketMessage::ping();
        let size = msg.estimated_size();
        assert!(size > 0);
        assert!(size < 100); // Ping should be small
    }

    #[test]
    fn test_priority_messages() {
        assert!(WebSocketMessage::ping().is_high_priority());
        assert!(WebSocketMessage::error("TEST".to_string(), "Test".to_string()).is_high_priority());
        assert!(!WebSocketMessage::subscribe("topic".to_string()).is_high_priority());
    }
}

/// Topic for pub/sub messaging
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Topic(String);

impl Topic {
    /// Create a new topic
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Get topic name as string
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Orders topic
    pub fn orders() -> Self {
        Self("orders".to_string())
    }

    /// Inventory topic
    pub fn inventory() -> Self {
        Self("inventory".to_string())
    }
}

impl std::fmt::Display for Topic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for Topic {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Topic {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}
