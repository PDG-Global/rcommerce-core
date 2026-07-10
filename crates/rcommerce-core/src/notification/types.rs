//! Notification types and structures

use crate::notification::NotificationChannel;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A message to be sent through a notification channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationMessage {
    pub template_id: String,
    pub recipient: String,
    pub subject: Option<String>,
    pub body: String,
    pub variables: TemplateVariables,
}

impl NotificationMessage {
    pub fn new(
        template_id: impl Into<String>,
        recipient: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            template_id: template_id.into(),
            recipient: recipient.into(),
            subject: None,
            body: body.into(),
            variables: TemplateVariables::new(),
        }
    }
}

/// Result of a notification delivery attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationResult {
    pub success: bool,
    pub channel: String,
    pub message_id: Option<String>,
    pub error: Option<String>,
}

/// Variables for template substitution
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TemplateVariables {
    inner: std::collections::HashMap<String, String>,
}

impl TemplateVariables {
    pub fn new() -> Self {
        Self {
            inner: std::collections::HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.inner.insert(key.into(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.inner.get(key)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.inner.iter()
    }
}

/// Delivery status of a notification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "delivery_status", rename_all = "snake_case")]
pub enum DeliveryStatus {
    Pending,
    Sent,
    Delivered,
    Failed,
    Bounced,
}

/// A single delivery attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryAttempt {
    pub channel: String,
    pub attempted_at: chrono::DateTime<chrono::Utc>,
    pub status: DeliveryStatus,
    pub error: Option<String>,
}

impl DeliveryAttempt {
    pub fn new(channel: impl Into<String>, status: DeliveryStatus) -> Self {
        Self {
            channel: channel.into(),
            attempted_at: chrono::Utc::now(),
            status,
            error: None,
        }
    }

    pub fn mark_sent(&mut self) {
        self.status = DeliveryStatus::Sent;
    }

    pub fn mark_delivered(&mut self) {
        self.status = DeliveryStatus::Delivered;
    }
}

/// Priority levels for notifications
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, sqlx::Type,
)]
#[sqlx(type_name = "notification_priority", rename_all = "snake_case")]
#[derive(Default)]
pub enum NotificationPriority {
    Low,
    #[default]
    Normal,
    High,
    Urgent,
}

/// A notification entity
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Notification {
    pub id: Uuid,
    pub channel: NotificationChannel,
    pub recipient: String,
    pub subject: String,
    pub body: String,
    pub html_body: Option<String>,
    pub priority: NotificationPriority,
    pub status: DeliveryStatus,
    pub attempt_count: i32,
    pub max_attempts: i32,
    pub error_message: Option<String>,
    pub metadata: serde_json::Value,
    pub scheduled_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Notification {
    pub fn new(
        channel: NotificationChannel,
        recipient: String,
        subject: String,
        body: String,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            channel,
            recipient,
            subject,
            body,
            html_body: None,
            priority: NotificationPriority::default(),
            status: DeliveryStatus::Pending,
            attempt_count: 0,
            max_attempts: 3,
            error_message: None,
            metadata: serde_json::Value::Null,
            scheduled_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_priority(mut self, priority: NotificationPriority) -> Self {
        self.priority = priority;
        self.updated_at = chrono::Utc::now();
        self
    }

    pub fn with_html_body(mut self, html_body: String) -> Self {
        self.html_body = Some(html_body);
        self.updated_at = chrono::Utc::now();
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = metadata;
        self.updated_at = chrono::Utc::now();
        self
    }

    pub fn schedule(mut self, schedule_time: chrono::DateTime<chrono::Utc>) -> Self {
        self.scheduled_at = Some(schedule_time);
        self.updated_at = chrono::Utc::now();
        self
    }

    pub fn mark_delivered(&mut self) {
        self.status = DeliveryStatus::Delivered;
        self.updated_at = chrono::Utc::now();
    }

    pub fn mark_failed(&mut self, error: String) {
        self.status = DeliveryStatus::Failed;
        self.error_message = Some(error);
        self.updated_at = chrono::Utc::now();
    }

    pub fn should_retry(&self) -> bool {
        self.status == DeliveryStatus::Failed && self.attempt_count < self.max_attempts
    }
}

/// Recipient information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipient {
    pub id: Uuid,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub webhook_url: Option<String>,
    pub preferences: NotificationPreferences,
}

impl Recipient {
    /// Create a new email recipient
    pub fn email(email: String, _name: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            email: Some(email),
            phone: None,
            webhook_url: None,
            preferences: NotificationPreferences {
                email_enabled: true,
                ..Default::default()
            },
        }
    }

    /// Get the primary notification channel for this recipient
    /// Returns Email if email is available, SMS if phone is available and email is not,
    /// Webhook if webhook_url is available and neither email nor phone is available
    pub fn primary_channel(&self) -> NotificationChannel {
        if self.email.is_some() && self.preferences.email_enabled {
            NotificationChannel::Email
        } else if self.phone.is_some() && self.preferences.sms_enabled {
            NotificationChannel::Sms
        } else if self.webhook_url.is_some() && self.preferences.webhook_enabled {
            NotificationChannel::Webhook
        } else {
            // Default to email if available, regardless of preference
            if self.email.is_some() {
                NotificationChannel::Email
            } else if self.phone.is_some() {
                NotificationChannel::Sms
            } else if self.webhook_url.is_some() {
                NotificationChannel::Webhook
            } else {
                NotificationChannel::Email // Fallback
            }
        }
    }
}

/// Notification preferences for a recipient
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationPreferences {
    pub email_enabled: bool,
    pub sms_enabled: bool,
    pub push_enabled: bool,
    pub webhook_enabled: bool,
    pub quiet_hours_start: Option<chrono::NaiveTime>,
    pub quiet_hours_end: Option<chrono::NaiveTime>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_creation() {
        let notification = Notification::new(
            NotificationChannel::Email,
            "user@example.com".to_string(),
            "Test subject".to_string(),
            "Test message".to_string(),
        );

        assert_eq!(notification.priority, NotificationPriority::Normal);
        assert!(notification.scheduled_at.is_none());
    }

    #[test]
    fn test_template_variables() {
        let mut vars = TemplateVariables::new();
        vars.insert("name", "John");
        vars.insert("order_id", "12345");

        assert_eq!(vars.get("name"), Some(&"John".to_string()));
        assert!(vars.contains_key("order_id"));
    }
}
