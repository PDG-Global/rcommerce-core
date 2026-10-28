pub mod accounting;
pub mod admin;
pub mod auth;
pub mod cart;
pub mod catalogue;
pub mod checkout;
pub mod contact;
pub mod coupon;
pub mod customer;
pub mod downloads;
pub mod dunning;
pub mod email_helpers;
pub mod invoice;
pub mod newsletter;
pub mod order;
pub mod payment;
pub mod product;
pub mod review;
pub mod statistics;
pub mod subscription;
pub mod webhook;
pub mod woocommerce;

/// Phone numbers are mandatory on all newly written addresses (account
/// addresses and checkout shipping/billing addresses). The DB column stays
/// nullable for legacy/imported rows; this is enforced at the API boundary.
pub(crate) fn require_address_phone(phone: &Option<String>) -> Result<(), rcommerce_core::Error> {
    match phone.as_deref().map(str::trim) {
        Some(p) if !p.is_empty() => Ok(()),
        _ => Err(rcommerce_core::Error::validation(
            "Phone number is required",
        )),
    }
}

pub use accounting::accounting_router;
pub use admin::router as admin_router;
pub use auth::protected_router as auth_protected_router;
pub use auth::public_router as auth_public_router;
pub use cart::protected_router as cart_protected_router;
pub use cart::public_router as cart_public_router;
pub use cart::router as cart_router;
pub use catalogue::admin_router as catalogue_admin_router;
pub use checkout::router as checkout_router;
pub use coupon::router as coupon_router;
pub use customer::router as customer_router;
pub use downloads::router as downloads_router;
pub use dunning::{admin_router as dunning_admin_router, router as dunning_router};
pub use newsletter::router as newsletter_router;
pub use order::router as order_router;
pub use payment::router as payment_router;
pub use product::admin_router as product_admin_router;
pub use product::router as product_router;
pub use review::admin_router as review_admin_router;
pub use review::protected_router as review_protected_router;
pub use review::public_router as review_public_router;
pub use statistics::router as statistics_router;
pub use subscription::{admin_router as subscription_admin_router, router as subscription_router};
pub use webhook::router as webhook_router;

use crate::state::AppState;
use axum::{routing::get, Router};
use tower_http::trace::TraceLayer;

/// WooCommerce-format error response
pub fn woocommerce_error(status: u16, code: &str, message: &str) -> axum::response::Response {
    let body = serde_json::json!({
        "code": code,
        "message": message,
        "data": { "status": status }
    });
    axum::http::Response::builder()
        .status(status)
        .header("content-type", "application/json; charset=UTF-8")
        .body(axum::body::Body::from(
            serde_json::to_string(&body).unwrap_or_default(),
        ))
        .unwrap()
}

/// Create the main API router with all routes
/// Note: CORS is configured in server.rs, not here, to avoid conflicts
pub fn create_router(app_state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/", get(api_info))
        .nest("/api/v1", api_v1_routes())
        .layer(TraceLayer::new_for_http())
        .with_state(app_state)
}

/// API v1 routes
fn api_v1_routes() -> Router<AppState> {
    Router::new()
        .merge(product_router())
        .merge(customer_router())
        .merge(order_router())
        .merge(auth_public_router())
        .merge(auth_protected_router())
        // Cart routes split by auth requirement:
        // Public routes (guest cart, get cart) + Protected routes (modify, merge, coupons)
        .merge(cart_public_router())
        .merge(cart_protected_router())
        .merge(checkout_router())
        .merge(coupon_router())
        .merge(payment_router())
        .merge(subscription_router())
        .merge(subscription_admin_router())
        .merge(newsletter_router())
        .merge(review_public_router())
        .merge(review_protected_router())
        .merge(admin_router())
        .merge(statistics_router())
        .merge(dunning_router())
        .merge(dunning_admin_router())
        .merge(downloads_router())
        .merge(webhook_router())
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}

/// API info endpoint
async fn api_info() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "name": "R Commerce API",
        "version": "1.0.0",
        "status": "operational"
    }))
}

#[cfg(test)]
mod tests {
    use super::require_address_phone;

    #[test]
    fn require_address_phone_accepts_present_numbers() {
        assert!(require_address_phone(&Some("+852 9123 4567".to_string())).is_ok());
        assert!(require_address_phone(&Some(" 5551234 ".to_string())).is_ok());
    }

    #[test]
    fn require_address_phone_rejects_missing_or_blank() {
        for phone in [None, Some(String::new()), Some("   ".to_string())] {
            let err = require_address_phone(&phone).expect_err("phone must be rejected");
            assert!(
                err.to_string().contains("Phone number is required"),
                "unexpected error: {err}"
            );
        }
    }
}
