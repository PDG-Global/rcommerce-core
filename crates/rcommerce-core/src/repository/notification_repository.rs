//! Notification repository for database operations

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{
    notification::types::{DeliveryStatus, Notification},
    notification::NotificationChannel,
    Error, Result,
};

/// Repository trait for notification operations
#[async_trait]
pub trait NotificationRepository: Send + Sync {
    /// Get a notification by ID
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Notification>>;

    /// Create a new notification
    async fn create(&self, notification: &Notification) -> Result<Notification>;

    /// Update a notification
    async fn update(&self, notification: &Notification) -> Result<Notification>;

    /// Update notification status
    async fn update_status(&self, id: Uuid, status: DeliveryStatus) -> Result<Notification>;

    /// Mark notification as delivered
    async fn mark_delivered(&self, id: Uuid) -> Result<Notification>;

    /// Mark notification as failed
    async fn mark_failed(&self, id: Uuid, error: &str) -> Result<Notification>;

    /// Increment attempt count
    async fn increment_attempt(&self, id: Uuid) -> Result<Notification>;

    /// Delete a notification
    async fn delete(&self, id: Uuid) -> Result<bool>;

    /// List notifications with filtering
    async fn list(
        &self,
        status: Option<DeliveryStatus>,
        channel: Option<NotificationChannel>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Notification>>;

    /// Get pending notifications (ready to be sent)
    async fn get_pending(&self, limit: i64) -> Result<Vec<Notification>>;

    /// Get scheduled notifications that are due
    async fn get_due(&self, limit: i64) -> Result<Vec<Notification>>;

    /// Get notifications by recipient
    async fn get_by_recipient(&self, recipient: &str, limit: i64) -> Result<Vec<Notification>>;

    /// Get failed notifications that should be retried
    async fn get_retryable(&self, limit: i64) -> Result<Vec<Notification>>;

    /// Count notifications by status
    async fn count_by_status(&self, status: DeliveryStatus) -> Result<i64>;

    /// Clean up old delivered notifications
    async fn cleanup_old(&self, before: DateTime<Utc>) -> Result<u64>;
}

/// PostgreSQL implementation of NotificationRepository
pub struct PostgresNotificationRepository {
    db: sqlx::PgPool,
}

impl PostgresNotificationRepository {
    /// Create a new PostgreSQL notification repository
    pub fn new(db: sqlx::PgPool) -> Self {
        Self { db }
    }
}

#[async_trait]
impl NotificationRepository for PostgresNotificationRepository {
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Notification>> {
        let notification =
            sqlx::query_as::<_, Notification>("SELECT * FROM notifications WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.db)
                .await
                .map_err(|e| Error::Other(format!("Failed to fetch notification: {}", e)))?;

        Ok(notification)
    }

    async fn create(&self, notification: &Notification) -> Result<Notification> {
        let notification = sqlx::query_as::<_, Notification>(
            r#"
            INSERT INTO notifications (
                id, channel, recipient, subject, body, html_body,
                priority, status, attempt_count, max_attempts,
                error_message, metadata, scheduled_at, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            RETURNING *
            "#,
        )
        .bind(notification.id)
        .bind(notification.channel)
        .bind(&notification.recipient)
        .bind(&notification.subject)
        .bind(&notification.body)
        .bind(&notification.html_body)
        .bind(notification.priority)
        .bind(notification.status)
        .bind(notification.attempt_count)
        .bind(notification.max_attempts)
        .bind(&notification.error_message)
        .bind(&notification.metadata)
        .bind(notification.scheduled_at)
        .bind(notification.created_at)
        .bind(notification.updated_at)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to create notification: {}", e)))?;

        Ok(notification)
    }

    async fn update(&self, notification: &Notification) -> Result<Notification> {
        let notification = sqlx::query_as::<_, Notification>(
            r#"
            UPDATE notifications 
            SET channel = $1,
                recipient = $2,
                subject = $3,
                body = $4,
                html_body = $5,
                priority = $6,
                status = $7,
                attempt_count = $8,
                max_attempts = $9,
                error_message = $10,
                metadata = $11,
                scheduled_at = $12,
                updated_at = $13
            WHERE id = $14
            RETURNING *
            "#,
        )
        .bind(notification.channel)
        .bind(&notification.recipient)
        .bind(&notification.subject)
        .bind(&notification.body)
        .bind(&notification.html_body)
        .bind(notification.priority)
        .bind(notification.status)
        .bind(notification.attempt_count)
        .bind(notification.max_attempts)
        .bind(&notification.error_message)
        .bind(&notification.metadata)
        .bind(notification.scheduled_at)
        .bind(Utc::now())
        .bind(notification.id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to update notification: {}", e)))?;

        Ok(notification)
    }

    async fn update_status(&self, id: Uuid, status: DeliveryStatus) -> Result<Notification> {
        let notification = sqlx::query_as::<_, Notification>(
            "UPDATE notifications SET status = $1, updated_at = NOW() WHERE id = $2 RETURNING *",
        )
        .bind(status)
        .bind(id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to update notification status: {}", e)))?;

        Ok(notification)
    }

    async fn mark_delivered(&self, id: Uuid) -> Result<Notification> {
        let notification = sqlx::query_as::<_, Notification>(
            r#"
            UPDATE notifications 
            SET status = 'delivered', updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to mark notification delivered: {}", e)))?;

        Ok(notification)
    }

    async fn mark_failed(&self, id: Uuid, error: &str) -> Result<Notification> {
        let notification = sqlx::query_as::<_, Notification>(
            r#"
            UPDATE notifications 
            SET status = 'failed', error_message = $1, updated_at = NOW()
            WHERE id = $2
            RETURNING *
            "#,
        )
        .bind(error)
        .bind(id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to mark notification failed: {}", e)))?;

        Ok(notification)
    }

    async fn increment_attempt(&self, id: Uuid) -> Result<Notification> {
        let notification = sqlx::query_as::<_, Notification>(
            r#"
            UPDATE notifications 
            SET attempt_count = attempt_count + 1, updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to increment attempt: {}", e)))?;

        Ok(notification)
    }

    async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM notifications WHERE id = $1")
            .bind(id)
            .execute(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to delete notification: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    async fn list(
        &self,
        status: Option<DeliveryStatus>,
        channel: Option<NotificationChannel>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Notification>> {
        let mut query = String::from("SELECT * FROM notifications WHERE 1=1");

        if status.is_some() {
            query.push_str(" AND status = $1");
        }
        if channel.is_some() {
            query.push_str(&format!(
                " AND channel = ${}",
                if status.is_some() { 2 } else { 1 }
            ));
        }

        query.push_str(&format!(
            " ORDER BY created_at DESC LIMIT ${} OFFSET ${}",
            if status.is_some() && channel.is_some() {
                3
            } else if status.is_some() || channel.is_some() {
                2
            } else {
                1
            },
            if status.is_some() && channel.is_some() {
                4
            } else if status.is_some() || channel.is_some() {
                3
            } else {
                2
            }
        ));

        let mut q = sqlx::query_as::<_, Notification>(&query);

        if let Some(s) = status {
            q = q.bind(s);
        }
        if let Some(c) = channel {
            q = q.bind(c);
        }
        q = q.bind(limit).bind(offset);

        let notifications = q
            .fetch_all(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to list notifications: {}", e)))?;

        Ok(notifications)
    }

    async fn get_pending(&self, limit: i64) -> Result<Vec<Notification>> {
        let notifications = sqlx::query_as::<_, Notification>(
            r#"
            SELECT * FROM notifications 
            WHERE status = 'pending'
            AND (scheduled_at IS NULL OR scheduled_at <= NOW())
            AND attempt_count < max_attempts
            ORDER BY priority DESC, created_at ASC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch pending notifications: {}", e)))?;

        Ok(notifications)
    }

    async fn get_due(&self, limit: i64) -> Result<Vec<Notification>> {
        let notifications = sqlx::query_as::<_, Notification>(
            r#"
            SELECT * FROM notifications 
            WHERE status = 'pending'
            AND scheduled_at IS NOT NULL
            AND scheduled_at <= NOW()
            ORDER BY scheduled_at ASC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch due notifications: {}", e)))?;

        Ok(notifications)
    }

    async fn get_by_recipient(&self, recipient: &str, limit: i64) -> Result<Vec<Notification>> {
        let notifications = sqlx::query_as::<_, Notification>(
            "SELECT * FROM notifications WHERE recipient = $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(recipient)
        .bind(limit)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch notifications by recipient: {}", e)))?;

        Ok(notifications)
    }

    async fn get_retryable(&self, limit: i64) -> Result<Vec<Notification>> {
        let notifications = sqlx::query_as::<_, Notification>(
            r#"
            SELECT * FROM notifications 
            WHERE status = 'failed'
            AND attempt_count < max_attempts
            ORDER BY attempt_count ASC, updated_at ASC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch retryable notifications: {}", e)))?;

        Ok(notifications)
    }

    async fn count_by_status(&self, status: DeliveryStatus) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE status = $1")
            .bind(status)
            .fetch_one(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to count notifications: {}", e)))?;

        Ok(count)
    }

    async fn cleanup_old(&self, before: DateTime<Utc>) -> Result<u64> {
        let result =
            sqlx::query("DELETE FROM notifications WHERE status = 'delivered' AND created_at < $1")
                .bind(before)
                .execute(&self.db)
                .await
                .map_err(|e| Error::Other(format!("Failed to cleanup old notifications: {}", e)))?;

        Ok(result.rows_affected())
    }
}
