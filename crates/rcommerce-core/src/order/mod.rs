pub mod calculation;
pub mod fulfillment;
pub mod lifecycle;
pub mod service;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

pub use calculation::{OrderCalculator, OrderTotals};
pub use fulfillment::{Fulfillment, FulfillmentStatus, TrackingInfo};
pub use lifecycle::{OrderEvent, OrderStatus, OrderTransition};
pub use service::OrderService;

/// Core order struct
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Order {
    pub id: Uuid,
    pub order_number: String,
    pub customer_id: Option<Uuid>,
    pub customer_email: String,
    pub billing_address_id: Option<Uuid>,
    pub shipping_address_id: Option<Uuid>,
    pub status: OrderStatus,
    pub fulfillment_status: FulfillmentStatus,
    pub payment_status: PaymentStatus,
    pub currency: String,
    pub subtotal: Decimal,
    pub tax_total: Decimal,
    pub shipping_total: Decimal,
    pub discount_total: Decimal,
    pub total: Decimal,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Order item
#[derive(Debug, Clone, sqlx::FromRow)]
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
    #[sqlx(rename = "title")]
    pub name: String,
    #[sqlx(rename = "variant_title")]
    pub variant_name: Option<String>,
    pub weight: Option<Decimal>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Payment status for orders
#[derive(Debug, Clone, Copy, PartialEq, sqlx::Type)]
#[sqlx(type_name = "payment_status", rename_all = "snake_case")]
pub enum PaymentStatus {
    Pending,
    Authorized,
    Paid,
    Failed,
    Refunded,
    PartiallyRefunded,
}

/// Order query filter
#[derive(Debug, Clone, Default)]
pub struct OrderFilter {
    pub status: Option<OrderStatus>,
    pub fulfillment_status: Option<FulfillmentStatus>,
    pub payment_status: Option<PaymentStatus>,
    pub customer_id: Option<Uuid>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
    pub order_number: Option<String>,
}

/// Create order request
#[derive(Debug, Clone)]
pub struct CreateOrderRequest {
    pub customer_id: Option<Uuid>,
    pub customer_email: String,
    pub billing_address_id: Option<Uuid>,
    pub shipping_address_id: Option<Uuid>,
    pub items: Vec<CreateOrderItem>,
    pub currency: String,
    pub subtotal: Decimal,
    pub tax_total: Decimal,
    pub shipping_total: Decimal,
    pub discount_total: Decimal,
    pub total: Decimal,
    pub notes: Option<String>,
    pub tags: Option<Vec<String>>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct CreateOrderItem {
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub quantity: i32,
    pub price: Decimal,
    pub tax_amount: Decimal,
    pub skip_inventory_reservation: bool,
}

impl CreateOrderItem {
    pub fn new(
        product_id: Uuid,
        variant_id: Option<Uuid>,
        quantity: i32,
        price: Decimal,
        tax_amount: Decimal,
    ) -> Self {
        Self {
            product_id,
            variant_id,
            quantity,
            price,
            tax_amount,
            skip_inventory_reservation: false,
        }
    }
}
