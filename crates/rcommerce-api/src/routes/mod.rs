pub mod admin;
pub mod auth;
pub mod cart;
pub mod checkout;
pub mod coupon;
pub mod customer;
pub mod downloads;
pub mod dunning;
pub mod order;
pub mod payment;
pub mod product;
pub mod statistics;
pub mod subscription;
pub mod webhook;

pub use admin::router as admin_router;
pub use auth::protected_router as auth_protected_router;
pub use auth::public_router as auth_public_router;
pub use cart::protected_router as cart_protected_router;
pub use cart::public_router as cart_public_router;
pub use cart::router as cart_router;
pub use checkout::router as checkout_router;
pub use coupon::router as coupon_router;
pub use customer::router as customer_router;
pub use downloads::router as downloads_router;
pub use dunning::router as dunning_router;
pub use order::router as order_router;
pub use payment::router as payment_router;
pub use product::admin_router as product_admin_router;
pub use product::router as product_router;
pub use statistics::router as statistics_router;
pub use subscription::router as subscription_router;
pub use webhook::router as webhook_router;

use crate::state::AppState;
use axum::{routing::get, Router};
use tower_http::trace::TraceLayer;

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
        .merge(admin_router())
        .merge(statistics_router())
        .merge(dunning_router())
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
