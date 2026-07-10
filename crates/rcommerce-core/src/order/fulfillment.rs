use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{Error, Result};

/// Fulfillment record (shipment)
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Fulfillment {
    pub id: Uuid,
    pub order_id: Uuid,
    pub status: FulfillmentStatus,
    pub tracking_number: Option<String>,
    pub tracking_url: Option<String>,
    pub tracking_company: Option<String>,
    pub shipped_at: Option<DateTime<Utc>>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Fulfillment status
#[derive(Debug, Clone, Copy, PartialEq, sqlx::Type)]
#[sqlx(type_name = "fulfillment_status", rename_all = "snake_case")]
pub enum FulfillmentStatus {
    Pending,    // Fulfillment created, not yet started
    Processing, // Items being picked and packed
    Shipped,    // Order has been shipped
    Delivered,  // Order has been delivered
    Partial,    // Partial fulfillment (some items shipped)
    Canceled,   // Fulfillment canceled
    Returned,   // Order returned
}

impl FulfillmentStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            FulfillmentStatus::Delivered
                | FulfillmentStatus::Canceled
                | FulfillmentStatus::Returned
        )
    }

    pub fn is_shipped(&self) -> bool {
        matches!(
            self,
            FulfillmentStatus::Shipped | FulfillmentStatus::Delivered
        )
    }

    pub fn description(&self) -> &'static str {
        match self {
            FulfillmentStatus::Pending => "Fulfillment pending",
            FulfillmentStatus::Processing => "Items being prepared",
            FulfillmentStatus::Shipped => "Order shipped",
            FulfillmentStatus::Delivered => "Order delivered",
            FulfillmentStatus::Partial => "Partial fulfillment",
            FulfillmentStatus::Canceled => "Fulfillment canceled",
            FulfillmentStatus::Returned => "Order returned",
        }
    }
}

/// Fulfillment item
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FulfillmentItem {
    pub id: Uuid,
    pub fulfillment_id: Uuid,
    pub order_item_id: Uuid,
    pub quantity: i32,
    pub created_at: DateTime<Utc>,
}

/// Tracking information
#[derive(Debug, Clone)]
pub struct TrackingInfo {
    pub tracking_number: String,
    pub carrier: String,
    pub status: TrackingStatus,
    pub location: Option<String>,
    pub estimated_delivery: Option<DateTime<Utc>>,
    pub actual_delivery: Option<DateTime<Utc>>,
    pub last_updated: DateTime<Utc>,
}

/// Tracking status
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrackingStatus {
    PreTransit,
    InTransit,
    OutForDelivery,
    Delivered,
    Failed,
    Exception,
}

/// Fulfillment service
#[derive(Default)]
pub struct FulfillmentService {
    // In production, this would have:
    // - Shipping provider integrations
    // - Tracking update service
    // - Return management
    // - Warehouse integration
}

impl FulfillmentService {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a fulfillment from order
    pub async fn create_from_order(&self, order_id: Uuid) -> Result<Fulfillment> {
        // Get order items
        let order = sqlx::query_as::<_, crate::order::Order>("SELECT * FROM orders WHERE id = $1")
            .bind(order_id)
            .fetch_optional(&*crate::db::get_pool().await?)
            .await?
            .ok_or_else(|| Error::not_found("Order not found"))?;

        // Check if order can be fulfilled
        if order.status != crate::order::OrderStatus::Confirmed {
            return Err(Error::validation(
                "Order must be confirmed before fulfillment",
            ));
        }

        let fulfillment_id = Uuid::new_v4();
        let fulfillment = sqlx::query_as::<_, Fulfillment>(
            r#"
            INSERT INTO fulfillments (id, order_id, status, tracking_number, tracking_url, tracking_company)
            VALUES ($1, $2, 'pending', NULL, NULL, NULL)
            RETURNING *
            "#
        )
        .bind(fulfillment_id)
        .bind(order_id)
        .fetch_one(&*crate::db::get_pool().await?)
        .await?;

        // Create fulfillment items (all order items initially)
        // In production, might support partial fulfillment

        Ok(fulfillment)
    }

    /// Update fulfillment status
    pub async fn update_status(
        &self,
        fulfillment_id: Uuid,
        new_status: FulfillmentStatus,
    ) -> Result<Fulfillment> {
        let fulfillment =
            sqlx::query_as::<_, Fulfillment>("SELECT * FROM fulfillments WHERE id = $1")
                .bind(fulfillment_id)
                .fetch_optional(&*crate::db::get_pool().await?)
                .await?
                .ok_or_else(|| Error::not_found("Fulfillment not found"))?;

        // Update tracking timestamps
        let (shipped_at, delivered_at) = match new_status {
            FulfillmentStatus::Shipped => (Some(Utc::now()), fulfillment.delivered_at),
            FulfillmentStatus::Delivered => (fulfillment.shipped_at, Some(Utc::now())),
            _ => (fulfillment.shipped_at, fulfillment.delivered_at),
        };

        let updated = sqlx::query_as::<_, Fulfillment>(
            r#"
            UPDATE fulfillments 
            SET status = $1, shipped_at = $2, delivered_at = $3, updated_at = NOW()
            WHERE id = $4
            RETURNING *
            "#,
        )
        .bind(format!("{:?}", new_status))
        .bind(shipped_at)
        .bind(delivered_at)
        .bind(fulfillment_id)
        .fetch_one(&*crate::db::get_pool().await?)
        .await?;

        Ok(updated)
    }

    /// Add tracking information
    pub async fn add_tracking(
        &self,
        fulfillment_id: Uuid,
        tracking_number: String,
        carrier: String,
        tracking_url: Option<String>,
    ) -> Result<Fulfillment> {
        let fulfillment = sqlx::query_as::<_, Fulfillment>(
            r#"
            UPDATE fulfillments 
            SET tracking_number = $1, tracking_company = $2, tracking_url = $3, updated_at = NOW()
            WHERE id = $4
            RETURNING *
            "#,
        )
        .bind(tracking_number)
        .bind(carrier)
        .bind(tracking_url)
        .bind(fulfillment_id)
        .fetch_one(&*crate::db::get_pool().await?)
        .await?;

        Ok(fulfillment)
    }

    /// Mark fulfillment as shipped
    pub async fn mark_shipped(
        &self,
        fulfillment_id: Uuid,
        tracking_info: TrackingInfo,
    ) -> Result<Fulfillment> {
        // Get tracking URL before consuming tracking_info fields
        let url = tracking_info.tracking_url();
        let tracking_number = tracking_info.tracking_number;
        let carrier = tracking_info.carrier;

        let _fulfillment = self
            .add_tracking(fulfillment_id, tracking_number, carrier, url)
            .await?;

        self.update_status(fulfillment_id, FulfillmentStatus::Shipped)
            .await
    }

    /// Process delivery confirmation
    pub async fn confirm_delivery(&self, fulfillment_id: Uuid) -> Result<Fulfillment> {
        self.update_status(fulfillment_id, FulfillmentStatus::Delivered)
            .await
    }

    /// Handle return
    pub async fn process_return(
        &self,
        fulfillment_id: Uuid,
        return_reason: String,
    ) -> Result<Fulfillment> {
        // Log return reason in metadata
        sqlx::query("UPDATE fulfillments SET metadata = jsonb_set(metadata, '{return_reason}', $1) WHERE id = $2")
            .bind(serde_json::json!(return_reason))
            .bind(fulfillment_id)
            .execute(&*crate::db::get_pool().await?)
            .await?;

        self.update_status(fulfillment_id, FulfillmentStatus::Returned)
            .await
    }
}

impl TrackingInfo {
    /// Create tracking info from carrier data
    pub fn new(tracking_number: String, carrier: String) -> Self {
        Self {
            tracking_number,
            carrier,
            status: TrackingStatus::PreTransit,
            location: None,
            estimated_delivery: None,
            actual_delivery: None,
            last_updated: Utc::now(),
        }
    }

    /// Generate tracking URL
    pub fn tracking_url(&self) -> Option<String> {
        match self.carrier.to_lowercase().as_str() {
            "ups" => Some(format!(
                "https://www.ups.com/track?tracknum={}",
                self.tracking_number
            )),
            "fedex" => Some(format!(
                "https://www.fedex.com/apps/fedextrack/?tracknumbers={}",
                self.tracking_number
            )),
            "usps" => Some(format!(
                "https://tools.usps.com/go/TrackConfirmAction?qtc_tLabels1={}",
                self.tracking_number
            )),
            "dhl" => Some(format!(
                "https://www.dhl.com/en/express/tracking.html?AWB={}",
                self.tracking_number
            )),
            _ => None,
        }
    }

    /// Update tracking status
    pub fn update_status(&mut self, status: TrackingStatus, location: Option<String>) {
        self.status = status;
        self.location = location;
        self.last_updated = Utc::now();
    }

    /// Mark as delivered
    pub fn mark_delivered(&mut self) {
        self.status = TrackingStatus::Delivered;
        self.actual_delivery = Some(Utc::now());
        self.last_updated = Utc::now();
    }
}

impl TrackingStatus {
    pub fn description(&self) -> &'static str {
        match self {
            TrackingStatus::PreTransit => "Pre-transit",
            TrackingStatus::InTransit => "In transit",
            TrackingStatus::OutForDelivery => "Out for delivery",
            TrackingStatus::Delivered => "Delivered",
            TrackingStatus::Failed => "Delivery failed",
            TrackingStatus::Exception => "Delivery exception",
        }
    }
}

/// Fulfillment tracking update from carrier
#[derive(Debug, Clone)]
pub struct TrackingUpdate {
    pub tracking_number: String,
    pub status: TrackingStatus,
    pub location: Option<String>,
    pub message: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// Return request
#[derive(Debug, Clone)]
pub struct ReturnRequest {
    pub id: Uuid,
    pub fulfillment_id: Uuid,
    pub order_id: Uuid,
    pub items: Vec<ReturnItem>,
    pub reason: String,
    pub notes: Option<String>,
    pub status: ReturnStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ReturnItem {
    pub order_item_id: Uuid,
    pub quantity: i32,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReturnStatus {
    Requested,
    Approved,
    Received,
    Inspected,
    Refunded,
    Rejected,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fulfillment_status() {
        assert!(FulfillmentStatus::Shipped.is_shipped());
        assert!(FulfillmentStatus::Delivered.is_shipped());
        assert!(!FulfillmentStatus::Pending.is_shipped());
        assert!(FulfillmentStatus::Delivered.is_terminal());
    }

    #[test]
    fn test_tracking_url() {
        let tracking = TrackingInfo::new("1Z12345E0291980793".to_string(), "UPS".to_string());

        assert!(tracking.tracking_url().is_some());
        assert!(tracking.tracking_url().unwrap().contains("ups.com"));
    }
}
