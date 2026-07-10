use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use crate::models::address::Address;
use crate::models::customer::Customer;
use crate::notification::channels::{EmailChannel, SmsChannel, WebhookChannel};
use crate::notification::templates::NotificationTemplate;
use crate::notification::{
    DeliveryAttempt, DeliveryStatus, Notification, NotificationChannel, NotificationPriority,
    Recipient, TemplateVariables,
};
use crate::order::{Fulfillment, Order, OrderItem};
use crate::{Error, Result};

/// Main notification service
pub struct NotificationService {
    #[allow(dead_code)]
    email_channel: EmailChannel,
    #[allow(dead_code)]
    sms_channel: SmsChannel,
    #[allow(dead_code)]
    webhook_channel: WebhookChannel,
    db: sqlx::PgPool,
}

impl NotificationService {
    pub fn new(
        email_channel: EmailChannel,
        sms_channel: SmsChannel,
        webhook_channel: WebhookChannel,
        db: sqlx::PgPool,
    ) -> Self {
        Self {
            email_channel,
            sms_channel,
            webhook_channel,
            db,
        }
    }

    /// Send a notification
    pub async fn send(&self, notification: &Notification) -> Result<DeliveryAttempt> {
        // Create delivery attempt
        let mut attempt = DeliveryAttempt::new(
            format!("{:?}", notification.channel),
            DeliveryStatus::Pending,
        );

        // Send based on channel
        match notification.channel {
            NotificationChannel::Email => {
                self.email_channel.send(notification).await?;
                attempt.mark_sent();
                attempt.mark_delivered(); // Simplified - email is "delivered" when sent
            }
            NotificationChannel::Sms => {
                log::info!("Sending SMS to: {}", notification.recipient);
                attempt.mark_sent();
                attempt.mark_delivered();
            }
            NotificationChannel::Webhook => {
                log::info!("Sending webhook to: {}", notification.recipient);
                attempt.mark_sent();
                attempt.mark_delivered();
            }
            _ => return Err(Error::not_implemented("Notification channel not supported")),
        }

        Ok(attempt)
    }

    /// Send notification with retry logic
    pub async fn send_with_retry(
        &self,
        notification: &Notification,
        max_retries: u32,
    ) -> Result<DeliveryAttempt> {
        let mut attempt = self.send(notification).await?;

        let mut retry_count = 0;
        while attempt.status == DeliveryStatus::Failed && retry_count < max_retries {
            retry_count += 1;

            log::warn!(
                "Notification {} failed, retry attempt {}/{}",
                notification.id,
                retry_count,
                max_retries
            );

            // Wait before retry (exponential backoff)
            let backoff_duration = std::time::Duration::from_secs(2_u64.pow(retry_count));
            tokio::time::sleep(backoff_duration).await;

            // Retry
            attempt = self.send(notification).await?;
        }

        if attempt.status == DeliveryStatus::Failed {
            log::error!(
                "Notification {} failed after {} attempts",
                notification.id,
                max_retries
            );
        }

        Ok(attempt)
    }

    /// Send notification to multiple recipients
    pub async fn send_bulk(
        &self,
        notification: &Notification,
        recipients: Vec<Recipient>,
    ) -> Result<Vec<DeliveryAttempt>> {
        let mut attempts = Vec::new();

        for recipient in recipients {
            let mut notification = notification.clone();
            // Get the appropriate recipient address based on channel
            let channel = notification.channel;
            notification.recipient = match channel {
                NotificationChannel::Email => recipient.email.clone().unwrap_or_default(),
                NotificationChannel::Sms => recipient.phone.clone().unwrap_or_default(),
                NotificationChannel::Webhook => recipient.webhook_url.clone().unwrap_or_default(),
                _ => String::new(),
            };

            match self.send(&notification).await {
                Ok(attempt) => attempts.push(attempt),
                Err(e) => {
                    log::error!(
                        "Failed to send notification to {:?}: {}",
                        notification.recipient,
                        e
                    );
                    continue;
                }
            }
        }

        Ok(attempts)
    }

    /// Queue notification for delayed sending
    pub async fn queue(&self, notification: &Notification, send_at: DateTime<Utc>) -> Result<Uuid> {
        let queue_id = Uuid::new_v4();

        // Store in database queue
        sqlx::query(
            r#"
            INSERT INTO notification_queue (id, notification_id, scheduled_at, status)
            VALUES ($1, $2, $3, 'queued')
            "#,
        )
        .bind(queue_id)
        .bind(notification.id)
        .bind(send_at)
        .execute(self.db())
        .await?;

        log::info!(
            "Notification {} queued for sending at {}",
            notification.id,
            send_at
        );

        Ok(queue_id)
    }

    /// Cancel a queued notification
    pub async fn cancel_queued(&self, queue_id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM notification_queue WHERE id = $1")
            .bind(queue_id)
            .execute(self.db())
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get notification history for a recipient
    pub async fn get_history(
        &self,
        recipient_address: &str,
        limit: i64,
    ) -> Result<Vec<Notification>> {
        let notifications = sqlx::query_as::<_, Notification>(
            r#"
            SELECT n.* FROM notifications n
            JOIN delivery_attempts da ON n.id = da.notification_id
            WHERE n.recipient_address = $1
            ORDER BY n.created_at DESC
            LIMIT $2
            "#,
        )
        .bind(recipient_address)
        .bind(limit)
        .fetch_all(self.db())
        .await?;

        Ok(notifications)
    }

    /// Get delivery statistics
    pub async fn get_delivery_stats(
        &self,
        channel: Option<NotificationChannel>,
        since: Option<DateTime<Utc>>,
    ) -> Result<DeliveryStats> {
        let mut query = String::from("SELECT status, COUNT(*) FROM delivery_attempts WHERE 1=1");

        if let Some(ch) = channel {
            query.push_str(&format!(
                " AND channel = '{}'",
                format!("{:?}", ch).to_lowercase()
            ));
        }

        if let Some(date) = since {
            query.push_str(&format!(" AND created_at >= '{}'", date));
        }

        query.push_str(" GROUP BY status");

        let rows = sqlx::query(&query).fetch_all(self.db()).await?;

        let mut stats = DeliveryStats::default();

        for row in rows {
            let status: String = row.get(0);
            let count: i64 = row.get(1);

            match status.as_str() {
                "sent" => stats.sent = count as u32,
                "delivered" => stats.delivered = count as u32,
                "failed" => stats.failed = count as u32,
                "bounced" => stats.bounced = count as u32,
                _ => {}
            }
        }

        Ok(stats)
    }

    /// Helper: Get provider name for channel
    #[allow(dead_code)]
    fn get_provider(&self, channel: &NotificationChannel) -> String {
        match channel {
            NotificationChannel::Email => "smtp",
            NotificationChannel::Sms => "twilio",
            NotificationChannel::Webhook => "webhook",
            _ => "unknown",
        }
        .to_string()
    }

    /// Helper: Get database connection
    fn db(&self) -> &sqlx::PgPool {
        &self.db
    }
}

/// Delivery statistics
#[derive(Debug, Clone, Default)]
pub struct DeliveryStats {
    pub sent: u32,
    pub delivered: u32,
    pub failed: u32,
    pub bounced: u32,
    pub opened: u32,
    pub clicked: u32,
}

impl DeliveryStats {
    pub fn delivery_rate(&self) -> f32 {
        if self.sent == 0 {
            0.0
        } else {
            self.delivered as f32 / self.sent as f32
        }
    }

    pub fn failure_rate(&self) -> f32 {
        if self.sent == 0 {
            0.0
        } else {
            self.failed as f32 / self.sent as f32
        }
    }
}

/// Notification factory for common use cases
pub struct NotificationFactory;

impl NotificationFactory {
    /// Get recipient address for a given channel
    fn get_recipient_address(recipient: &Recipient, channel: NotificationChannel) -> String {
        match channel {
            NotificationChannel::Email => recipient.email.clone().unwrap_or_default(),
            NotificationChannel::Sms => recipient.phone.clone().unwrap_or_default(),
            NotificationChannel::Webhook => recipient.webhook_url.clone().unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// Order confirmation notification (plain text)
    pub fn order_confirmation(order: &Order, recipient: Recipient) -> Notification {
        let channel = recipient.primary_channel();
        let recipient_addr = Self::get_recipient_address(&recipient, channel);

        Notification::new(
            channel,
            recipient_addr,
            format!("Order Confirmed: {}", order.order_number),
            format!(
                "Your order {} has been confirmed. Total: ${}",
                order.order_number, order.total
            ),
        )
        .with_priority(NotificationPriority::High)
        .with_metadata(serde_json::json!({
            "order_id": order.id,
            "type": "order_confirmation",
        }))
    }

    /// Order confirmation notification with HTML invoice template
    pub fn order_confirmation_html(
        order: &Order,
        recipient: Recipient,
        customer: &Customer,
        shipping_address: &Address,
        billing_address: &Address,
        order_items: &[OrderItem],
    ) -> Result<Notification> {
        // Load the HTML template
        let template = NotificationTemplate::load("order_confirmation_html")
            .map_err(|e| Error::notification_error(format!("Failed to load template: {}", e)))?;

        // Prepare template variables
        let mut variables = TemplateVariables::new();
        variables.add_order(order);
        variables.add_customer(customer);
        variables.add_addresses(shipping_address, billing_address);
        variables.add_order_items(order_items);
        variables.add_totals(order);
        variables.add_company_info("PDG Global Limited", "support@rcommerce.app");

        // Render templates
        let body = template
            .render(&variables)
            .map_err(|e| Error::notification_error(format!("Failed to render template: {}", e)))?;

        let html_body = template.render_html(&variables).map_err(|e| {
            Error::notification_error(format!("Failed to render HTML template: {}", e))
        })?;

        let channel = recipient.primary_channel();
        let recipient_addr = Self::get_recipient_address(&recipient, channel);

        let mut notification =
            Notification::new(channel, recipient_addr, template.subject.clone(), body)
                .with_priority(NotificationPriority::High)
                .with_metadata(serde_json::json!({
                    "order_id": order.id,
                    "type": "order_confirmation_html",
                    "template_id": template.id,
                }));

        if let Some(html) = html_body {
            notification = notification.with_html_body(html);
        }

        Ok(notification)
    }

    /// Order shipped notification
    pub fn order_shipped(
        order: &Order,
        fulfillment: &Fulfillment,
        recipient: Recipient,
    ) -> Notification {
        let mut body = format!("Your order {} has been shipped!", order.order_number);

        if let Some(tracking) = &fulfillment.tracking_number {
            body.push_str(&format!("\n\nTracking: {}", tracking));
        }

        let channel = recipient.primary_channel();
        let recipient_addr = Self::get_recipient_address(&recipient, channel);

        Notification::new(
            channel,
            recipient_addr,
            format!("Order Shipped: {}", order.order_number),
            body,
        )
        .with_priority(NotificationPriority::High)
        .with_metadata(serde_json::json!({
            "order_id": order.id,
            "fulfillment_id": fulfillment.id,
            "type": "order_shipped",
        }))
    }

    /// Low stock email
    pub fn low_stock_alert(
        alert: &crate::inventory::LowStockAlert,
        recipient: Recipient,
    ) -> Notification {
        let priority = if alert.is_critical() {
            NotificationPriority::Urgent
        } else {
            NotificationPriority::High
        };

        let channel = recipient.primary_channel();
        let recipient_addr = Self::get_recipient_address(&recipient, channel);

        Notification::new(
            channel,
            recipient_addr,
            format!("Low Stock Alert: {}", alert.product_name),
            alert.notification_message(),
        )
        .with_priority(priority)
        .with_metadata(serde_json::json!({
            "product_id": alert.product_id,
            "type": "low_stock_alert",
            "alert_level": format!("{:?}", alert.alert_level),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_delivery_stats() {
        let stats = DeliveryStats {
            sent: 1000,
            delivered: 950,
            failed: 45,
            bounced: 5,
            ..Default::default()
        };

        assert_eq!(stats.delivery_rate(), 0.95);
        assert_eq!(stats.failure_rate(), 0.045);
    }
}
