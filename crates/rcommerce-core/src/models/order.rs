use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

/// Order type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type, PartialEq, Eq, Default)]
#[sqlx(type_name = "order_type", rename_all = "snake_case")]
pub enum OrderType {
    /// One-time purchase
    #[default]
    OneTime,
    /// Subscription initial order
    SubscriptionInitial,
    /// Subscription renewal order
    SubscriptionRenewal,
}

/// Order entity
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Order {
    pub id: Uuid,
    pub order_number: String,
    pub customer_id: Option<Uuid>,
    pub customer_email: String,
    #[sqlx(rename = "currency_text")]
    pub currency: String,
    pub order_type: OrderType,
    pub subtotal: Decimal,
    pub tax_total: Decimal,
    pub shipping_total: Decimal,
    pub discount_total: Decimal,
    pub total: Decimal,
    pub status: OrderStatus,
    pub fulfillment_status: FulfillmentStatus,
    pub payment_status: PaymentStatus,
    /// Stable store-prefixed reference (e.g. "88-1024") used as the Xero
    /// invoice number and the order_id sent to payment providers.
    /// `sqlx(default)`: queries with explicit column lists predating this
    /// field decode it as NULL.
    #[sqlx(default)]
    pub invoice_number: Option<String>,
    /// Test-mode order (sandbox gateway, admin test action, flagged payment
    /// method). Test records still move real money and must be bookable.
    #[sqlx(default)]
    pub test: bool,
    pub shipping_address_id: Option<Uuid>,
    pub billing_address_id: Option<Uuid>,
    pub payment_method: Option<String>,
    pub shipping_method: Option<String>,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub draft: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    // Subscription fields (for subscription orders)
    pub subscription_id: Option<Uuid>,
    pub billing_cycle: Option<i32>, // Which billing cycle this is (1, 2, 3...)
    // Additional DB columns
    pub shipping_address: Option<serde_json::Value>,
    pub billing_address: Option<serde_json::Value>,
    pub metadata: Option<serde_json::Value>,
    pub coupon_code: Option<String>,
    pub coupon_id: Option<Uuid>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub cancelled_at: Option<DateTime<Utc>>,
}

/// Order status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "order_status", rename_all = "snake_case")]
pub enum OrderStatus {
    Pending,
    Confirmed,
    Processing,
    OnHold,
    Completed,
    Cancelled,
    Refunded,
}

/// Fulfillment status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "fulfillment_status", rename_all = "snake_case")]
pub enum FulfillmentStatus {
    Pending,
    Processing,
    Partial,
    Shipped,
    Delivered,
    Cancelled,
    Returned,
}

/// Payment status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "payment_status", rename_all = "snake_case")]
pub enum PaymentStatus {
    Pending,
    Authorized,
    Paid,
    Failed,
    Cancelled,
    Refunded,
}

/// Order item
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OrderItem {
    pub id: Uuid,
    pub order_id: Uuid,
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub quantity: i32,
    pub price: Decimal,
    pub subtotal: Decimal,
    pub tax_amount: Decimal,
    pub total: Decimal,
    pub sku: Option<String>,
    pub title: String,
    pub variant_title: Option<String>,
    pub requires_shipping: bool,
    pub is_gift_card: bool,
    pub weight: Option<Decimal>,
    pub weight_unit: Option<super::WeightUnit>,
    pub image_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // Digital product fields
    pub is_digital: Option<bool>,
    pub download_url: Option<String>,
    pub license_key: Option<String>,
    // Bundle product fields
    pub bundle_parent_id: Option<Uuid>,
    pub is_bundle_component: Option<bool>,
}

/// Order fulfillment
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
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

/// Payment method types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "payment_method_type", rename_all = "snake_case")]
pub enum PaymentMethodType {
    CreditCard,
    DebitCard,
    BankTransfer,
    CashOnDelivery,
    DigitalWallet,
    Cryptocurrency,
}

/// Order filter for queries
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OrderFilter {
    pub status: Option<OrderStatus>,
    pub fulfillment_status: Option<FulfillmentStatus>,
    pub payment_status: Option<PaymentStatus>,
    pub customer_id: Option<Uuid>,
    pub email: Option<String>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
    pub updated_after: Option<DateTime<Utc>>,
    pub total_min: Option<Decimal>,
    pub total_max: Option<Decimal>,
    pub tags: Vec<String>,
    pub draft: Option<bool>,
}

/// Create order request
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateOrderRequest {
    pub customer_id: Option<Uuid>,

    #[validate(email)]
    pub email: String,

    pub currency: super::Currency,

    pub shipping_address_id: Option<Uuid>,

    pub billing_address_id: Option<Uuid>,

    pub payment_method: Option<String>,

    pub shipping_method: Option<String>,

    pub notes: Option<String>,
}

/// Order note
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OrderNote {
    pub id: Uuid,
    pub order_id: Uuid,
    pub author: Option<String>,
    pub note: String,
    pub is_customer_notified: bool,
    pub created_at: DateTime<Utc>,
}

/// Order item with digital product details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItemWithDownloads {
    #[serde(flatten)]
    pub order_item: OrderItem,
    pub downloads: Vec<super::OrderItemDownload>,
    pub license_key: Option<super::LicenseKey>,
}

/// Order with all related data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderWithDetails {
    #[serde(flatten)]
    pub order: Order,
    pub items: Vec<OrderItemWithDownloads>,
    pub fulfillments: Vec<Fulfillment>,
    pub notes: Vec<OrderNote>,
}
