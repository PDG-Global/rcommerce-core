//! WooCommerce REST API compatibility types and response mapping.
//!
//! Maps rCommerce internal models to WooCommerce v3 JSON schema.

use rust_decimal::Decimal;
use serde::Serialize;

/// WooCommerce product response (subset used by integrations)
#[derive(Debug, Serialize)]
pub struct WcProduct {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub status: String,
    #[serde(rename = "type")]
    pub product_type: String,
    pub description: String,
    pub short_description: String,
    pub sku: String,
    pub price: String,
    pub regular_price: String,
    pub sale_price: String,
    pub stock_quantity: i32,
    pub stock_status: String,
    pub categories: Vec<WcCategoryRef>,
    pub images: Vec<WcImage>,
    pub date_created: String,
    pub date_modified: String,
}

#[derive(Debug, Serialize)]
pub struct WcCategoryRef {
    pub id: String,
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Serialize)]
pub struct WcImage {
    pub id: String,
    pub src: String,
    pub alt: String,
}

#[derive(Debug, Serialize)]
pub struct WcProductCategory {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub count: i32,
}

/// WooCommerce order response
#[derive(Debug, Serialize)]
pub struct WcOrder {
    pub id: String,
    pub parent_id: i64,
    pub number: String,
    pub order_key: String,
    pub created_via: String,
    pub version: String,
    pub status: String,
    pub currency: String,
    pub date_created: String,
    pub date_created_gmt: String,
    pub date_modified: String,
    pub date_modified_gmt: String,
    pub discount_total: String,
    pub discount_tax: String,
    pub shipping_total: String,
    pub shipping_tax: String,
    pub cart_tax: String,
    pub total: String,
    pub total_tax: String,
    pub prices_include_tax: bool,
    pub customer_id: i64,
    pub customer_note: String,
    pub billing: WcAddress,
    pub shipping: WcAddress,
    pub payment_method: String,
    pub payment_method_title: String,
    pub transaction_id: String,
    pub date_completed: Option<String>,
    pub date_completed_gmt: Option<String>,
    pub date_paid: Option<String>,
    pub date_paid_gmt: Option<String>,
    pub cart_hash: String,
    pub meta_data: Vec<WcMetaData>,
    pub line_items: Vec<WcLineItem>,
    pub tax_lines: Vec<serde_json::Value>,
    pub shipping_lines: Vec<WcShippingLine>,
    pub fee_lines: Vec<serde_json::Value>,
    pub coupon_lines: Vec<serde_json::Value>,
    pub refunds: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct WcShippingLine {
    pub id: String,
    pub method_title: String,
    pub method_id: String,
    pub total: String,
    pub total_tax: String,
}

#[derive(Debug, Serialize)]
pub struct WcMetaData {
    pub id: String,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct WcAddress {
    pub first_name: String,
    pub last_name: String,
    pub address_1: String,
    pub address_2: String,
    pub city: String,
    pub state: String,
    pub postcode: String,
    pub country: String,
    pub email: String,
    pub phone: String,
}

#[derive(Debug, Serialize)]
pub struct WcLineItem {
    pub id: String,
    pub name: String,
    pub product_id: String,
    pub variation_id: i64,
    pub quantity: i32,
    pub tax_class: String,
    pub subtotal: String,
    pub subtotal_tax: String,
    pub total: String,
    pub total_tax: String,
    pub taxes: Vec<serde_json::Value>,
    pub meta_data: Vec<serde_json::Value>,
    pub sku: String,
    pub price: String,
    pub image: WcLineItemImage,
}

#[derive(Debug, Serialize)]
pub struct WcLineItemImage {
    pub id: String,
    pub src: String,
}

/// WooCommerce customer response
#[derive(Debug, Serialize)]
pub struct WcCustomer {
    pub id: String,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub billing: WcAddress,
    pub shipping: WcAddress,
    pub date_created: String,
    pub date_modified: String,
}

/// Map rCommerce order status to WooCommerce status
pub fn map_order_status(status: &str) -> String {
    match status {
        "pending" => "pending",
        "confirmed" => "processing",
        "processing" => "processing",
        "on_hold" => "on-hold",
        "completed" => "completed",
        "cancelled" => "cancelled",
        "refunded" => "refunded",
        _ => "pending",
    }
    .to_string()
}

/// Map WooCommerce status back to rCommerce
pub fn unmap_order_status(wc_status: &str) -> String {
    match wc_status {
        "pending" => "pending",
        "processing" => "confirmed",
        "on-hold" => "on_hold",
        "completed" => "completed",
        "cancelled" => "cancelled",
        "refunded" => "refunded",
        "failed" => "cancelled",
        _ => "pending",
    }
    .to_string()
}

/// Fulfillment status implied by a WooCommerce order status, if any
pub fn fulfillment_for_wc_status(wc_status: &str) -> Option<&'static str> {
    match wc_status {
        "completed" => Some("shipped"),
        "processing" => Some("processing"),
        "cancelled" => Some("cancelled"),
        _ => None,
    }
}

/// Format decimal as WooCommerce price string (2 decimal places)
pub fn format_price(val: &Decimal) -> String {
    format!("{:.2}", val)
}

#[cfg(test)]
mod wc_status_tests {
    use super::*;

    #[test]
    fn test_fulfillment_for_wc_status() {
        assert_eq!(fulfillment_for_wc_status("completed"), Some("shipped"));
        assert_eq!(fulfillment_for_wc_status("processing"), Some("processing"));
        assert_eq!(fulfillment_for_wc_status("cancelled"), Some("cancelled"));
        assert_eq!(fulfillment_for_wc_status("pending"), None);
        assert_eq!(fulfillment_for_wc_status("on-hold"), None);
        assert_eq!(fulfillment_for_wc_status("refunded"), None);
    }

    #[test]
    fn test_unmap_order_status_known_values() {
        assert_eq!(unmap_order_status("completed"), "completed");
        assert_eq!(unmap_order_status("processing"), "confirmed");
        assert_eq!(unmap_order_status("on-hold"), "on_hold");
        assert_eq!(unmap_order_status("cancelled"), "cancelled");
    }
}
