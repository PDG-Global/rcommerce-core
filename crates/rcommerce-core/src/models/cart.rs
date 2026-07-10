//! Cart model for managing shopping carts
//!
//! Carts support both guest users (via session token) and authenticated users.
//! Carts can be converted to orders when checkout is complete.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

/// Cart entity - represents a shopping cart
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Cart {
    pub id: Uuid,
    /// Optional customer ID (null for guest carts)
    pub customer_id: Option<Uuid>,
    /// Session token for guest carts (used to identify cart without login)
    pub session_token: Option<String>,
    /// Cart currency
    pub currency: super::Currency,
    /// Subtotal before discounts and taxes
    pub subtotal: Decimal,
    /// Total discount amount
    pub discount_total: Decimal,
    /// Total tax amount
    pub tax_total: Decimal,
    /// Shipping cost
    pub shipping_total: Decimal,
    /// Final total
    pub total: Decimal,
    /// Applied coupon code
    pub coupon_code: Option<String>,
    /// Email for guest checkout
    pub email: Option<String>,
    /// Shipping address ID (if saved)
    pub shipping_address_id: Option<Uuid>,
    /// Billing address ID (if saved)
    pub billing_address_id: Option<Uuid>,
    /// Selected shipping method
    pub shipping_method: Option<String>,
    /// Notes for the order
    pub notes: Option<String>,
    /// When the cart was created
    pub created_at: DateTime<Utc>,
    /// When the cart was last updated
    pub updated_at: DateTime<Utc>,
    /// When the cart expires (for cleanup)
    pub expires_at: Option<DateTime<Utc>>,
    /// Whether this cart has been converted to an order
    pub converted_to_order: bool,
    /// The order ID if converted
    pub order_id: Option<Uuid>,
}

/// Cart item - represents a product in the cart
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CartItem {
    pub id: Uuid,
    pub cart_id: Uuid,
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    /// Quantity of this item
    pub quantity: i32,
    /// Unit price at time of adding to cart
    pub unit_price: Decimal,
    /// Original price (before any discounts)
    pub original_price: Decimal,
    /// Line item subtotal (unit_price * quantity)
    pub subtotal: Decimal,
    /// Discount amount for this line item
    pub discount_amount: Decimal,
    /// Total after discounts (quantity * unit_price - discount)
    pub total: Decimal,
    /// Product SKU
    pub sku: Option<String>,
    /// Product title
    pub title: String,
    /// Variant title (e.g., "Large / Red")
    pub variant_title: Option<String>,
    /// Product image URL
    pub image_url: Option<String>,
    /// Whether this item requires shipping
    pub requires_shipping: bool,
    /// Whether this is a gift card
    pub is_gift_card: bool,
    /// Custom attributes (e.g., personalization)
    pub custom_attributes: Option<serde_json::Value>,
    /// When the item was added
    pub created_at: DateTime<Utc>,
    /// When the item was last updated
    pub updated_at: DateTime<Utc>,
}

/// Input for adding an item to cart
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AddToCartInput {
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    #[validate(range(min = 1, max = 9999))]
    pub quantity: i32,
    pub custom_attributes: Option<serde_json::Value>,
}

/// Input for updating a cart item
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateCartItemInput {
    #[validate(range(min = 0, max = 9999))]
    pub quantity: i32,
    pub custom_attributes: Option<serde_json::Value>,
}

/// Input for applying a coupon
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ApplyCouponInput {
    #[validate(length(min = 1, max = 50))]
    pub coupon_code: String,
}

/// Cart summary for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CartSummary {
    pub id: Uuid,
    pub item_count: i32,
    pub subtotal: Decimal,
    pub discount_total: Decimal,
    pub tax_total: Decimal,
    pub shipping_total: Decimal,
    pub total: Decimal,
    pub coupon_code: Option<String>,
    pub currency: super::Currency,
}

/// Cart with items for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CartWithItems {
    pub cart: Cart,
    pub items: Vec<CartItem>,
}

/// Cart session identifier - used to identify a cart
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CartIdentifier {
    /// Authenticated user with customer ID
    Customer(Uuid),
    /// Guest user with session token
    Session(String),
    /// Direct cart ID
    CartId(Uuid),
}

impl Cart {
    /// Calculate cart totals based on items
    pub fn recalculate(&mut self, items: &[CartItem]) {
        self.subtotal = items.iter().map(|i| i.subtotal).sum();
        self.discount_total = items.iter().map(|i| i.discount_amount).sum();
        self.total = self.subtotal - self.discount_total + self.tax_total + self.shipping_total;
    }

    /// Check if cart is empty
    pub fn is_empty(&self, items: &[CartItem]) -> bool {
        items.is_empty()
    }

    /// Get total quantity of items in cart
    pub fn total_quantity(&self, items: &[CartItem]) -> i32 {
        items.iter().map(|i| i.quantity).sum()
    }
}

impl CartItem {
    /// Calculate line item totals
    pub fn calculate_totals(&mut self) {
        self.subtotal = self.unit_price * Decimal::from(self.quantity);
        self.total = self.subtotal - self.discount_amount;
    }
}
