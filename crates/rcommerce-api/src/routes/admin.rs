pub mod products;

use crate::state::AppState;
use axum::{extract::State, routing::get, Json, Router};

/// Get admin dashboard stats
pub async fn get_stats(State(_state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "stats": {
            "total_orders": 0,
            "total_customers": 0,
            "total_revenue": 0,
            "pending_orders": 0
        }
    }))
}

/// List all API keys (admin only)
pub async fn list_api_keys(State(_state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "api_keys": []
    }))
}

/// Router for admin routes
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/stats", get(get_stats))
        .route("/admin/api-keys", get(list_api_keys))
        .merge(products::router())
}
