use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{ProductInventory, StockAlertLevel};
use crate::Result;

/// Low stock alert notification
#[derive(Debug, Clone)]
pub struct LowStockAlert {
    pub product_id: Uuid,
    pub product_name: String,
    pub current_stock: i32,
    pub threshold: i32,
    pub alert_level: StockAlertLevel,
    pub recommended_reorder_quantity: i32,
    pub locations_affected: Vec<LocationAlert>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct LocationAlert {
    pub location_id: Uuid,
    pub location_name: String,
    pub current_stock: i32,
    pub alert_level: StockAlertLevel,
}

impl LowStockAlert {
    pub fn new(product_id: Uuid, product_name: String, inventory: &ProductInventory) -> Self {
        let mut locations_affected = Vec::new();
        let mut critical_count = 0;
        let mut low_count = 0;

        for location in &inventory.locations {
            let threshold = inventory.low_stock_threshold;
            let critical_threshold = (threshold as f32 * 0.5) as i32;

            let alert_level = if location.available <= critical_threshold {
                critical_count += 1;
                StockAlertLevel::Critical
            } else if location.available <= threshold {
                low_count += 1;
                StockAlertLevel::Low
            } else {
                continue; // Skip locations not in low stock
            };

            locations_affected.push(LocationAlert {
                location_id: location.location_id,
                location_name: location.location_name.clone(),
                current_stock: location.available,
                alert_level,
            });
        }

        // Overall alert level
        let alert_level = if critical_count > 0 {
            StockAlertLevel::Critical
        } else if low_count > 0 {
            StockAlertLevel::Low
        } else {
            // This shouldn't happen if we're creating an alert
            StockAlertLevel::Low
        };

        Self {
            product_id,
            product_name,
            current_stock: inventory.total_available,
            threshold: inventory.low_stock_threshold,
            alert_level,
            recommended_reorder_quantity: inventory.low_stock_threshold * 5, // Reorder to 5x threshold
            locations_affected,
            created_at: Utc::now(),
        }
    }

    pub fn is_critical(&self) -> bool {
        self.alert_level == StockAlertLevel::Critical
    }

    pub fn notification_message(&self) -> String {
        let level = if self.is_critical() {
            "CRITICAL"
        } else {
            "Low"
        };

        if self.locations_affected.len() == 1 {
            format!(
                "{} stock alert for {} at {}. Current: {}, Threshold: {}",
                level,
                self.product_name,
                self.locations_affected[0].location_name,
                self.current_stock,
                self.threshold
            )
        } else {
            format!(
                "{} stock alert for {} ({} locations affected). Current: {}, Threshold: {}",
                level,
                self.product_name,
                self.locations_affected.len(),
                self.current_stock,
                self.threshold
            )
        }
    }
}

/// Stock alert notification service
pub struct StockAlertService {
    // In a real implementation, this would have:
    // - Email sender
    // - SMS gateway client
    // - Webhook dispatcher
    // - Database logger
}

impl Default for StockAlertService {
    fn default() -> Self {
        Self::new()
    }
}

impl StockAlertService {
    pub fn new() -> Self {
        Self {}
    }

    /// Send low stock email alert
    pub async fn send_email_alert(&self, alert: &LowStockAlert, to_email: &str) -> Result<()> {
        let subject = format!("Low Stock Alert: {}", alert.product_name);
        let body = self.format_email_body(alert);

        // TODO: Integrate with email service
        log::info!("Sending low stock email to {}: {}", to_email, subject);
        log::debug!("Email body:\n{}", body);

        Ok(())
    }

    /// Send low stock SMS alert
    pub async fn send_sms_alert(&self, alert: &LowStockAlert, to_phone: &str) -> Result<()> {
        let message = alert.notification_message();

        // TODO: Integrate with SMS gateway
        log::info!("Sending low stock SMS to {}: {}", to_phone, message);

        Ok(())
    }

    /// Send webhook notification
    pub async fn send_webhook_alert(&self, alert: &LowStockAlert, webhook_url: &str) -> Result<()> {
        let payload = serde_json::json!({
            "event": "inventory.low_stock",
            "product_id": alert.product_id,
            "product_name": alert.product_name,
            "current_stock": alert.current_stock,
            "threshold": alert.threshold,
            "alert_level": format!("{:?}", alert.alert_level).to_lowercase(),
            "recommended_reorder_quantity": alert.recommended_reorder_quantity,
            "locations_affected": alert.locations_affected.len(),
            "created_at": alert.created_at.to_rfc3339(),
        });

        // TODO: Send HTTP POST to webhook_url
        log::info!("Sending low stock webhook to {}: {}", webhook_url, payload);

        Ok(())
    }

    /// Log alert to database for audit trail
    pub async fn log_alert(&self, alert: &LowStockAlert) -> Result<Uuid> {
        let alert_id = Uuid::new_v4();

        // TODO: Store in database
        log::info!(
            "Logging low stock alert {} for product {}",
            alert_id,
            alert.product_name
        );

        Ok(alert_id)
    }

    /// Format email body
    fn format_email_body(&self, alert: &LowStockAlert) -> String {
        let mut body = String::new();

        body.push_str(&format!("Low Stock Alert for {}\n", alert.product_name));
        body.push_str(&format!("Alert Level: {:?}\n", alert.alert_level));
        body.push_str(&format!("Current Stock: {}\n", alert.current_stock));
        body.push_str(&format!("Threshold: {}\n", alert.threshold));
        body.push_str(&format!(
            "Recommended Reorder: {}\n",
            alert.recommended_reorder_quantity
        ));
        body.push_str(&format!(
            "Locations Affected: {}\n",
            alert.locations_affected.len()
        ));
        body.push_str(&format!("Time: {}\n", alert.created_at));

        if !alert.locations_affected.is_empty() {
            body.push_str("\nLocation Details:\n");
            for location in &alert.locations_affected {
                body.push_str(&format!(
                    "  - {}: {} units ({}).\n",
                    location.location_name,
                    location.current_stock,
                    format!("{:?}", location.alert_level).to_lowercase()
                ));
            }
        }

        body
    }
}

/// Bulk alert processor for scheduled checks
pub struct BulkAlertProcessor;

impl BulkAlertProcessor {
    pub async fn check_all_products(
        &self,
        _inventory_service: &super::service::InventoryService,
    ) -> Result<Vec<LowStockAlert>> {
        // TODO: Get all products that need checking
        // For now, this is a placeholder
        log::info!("Running bulk low stock check for all products");

        // In a real implementation:
        // 1. Query all active products
        // 2. Check inventory for each
        // 3. Generate alerts for low stock items
        // 4. Send notifications based on vendor preferences

        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{LocationInventory, ProductInventory};
    use super::*;

    #[test]
    fn test_low_stock_alert_creation() {
        let product_id = Uuid::new_v4();
        let location_id = Uuid::new_v4();

        let inventory = ProductInventory {
            product_id,
            total_available: 5,
            total_reserved: 0,
            total_incoming: 0,
            low_stock_threshold: 10,
            locations: vec![LocationInventory {
                location_id,
                location_name: "Warehouse A".to_string(),
                available: 5,
                reserved: 0,
                incoming: 0,
            }],
        };

        let alert = LowStockAlert::new(product_id, "Test Product".to_string(), &inventory);

        assert_eq!(alert.product_id, product_id);
        assert_eq!(alert.current_stock, 5);
        assert_eq!(alert.locations_affected.len(), 1);
    }
}
