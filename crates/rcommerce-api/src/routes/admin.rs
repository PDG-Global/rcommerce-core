pub mod products;

use crate::state::AppState;
use axum::{extract::State, routing::{get, patch}, Json, Router};
use rcommerce_core::repository::statistics_repository::PgStatisticsRepository;
use rcommerce_core::repository::ApiKeyRepository;
use rcommerce_core::services::statistics_service::StatisticsService;
use rcommerce_core::Error;

/// Get admin dashboard stats
///
/// GET /api/v1/admin/stats
pub async fn get_stats(State(state): State<AppState>) -> Result<Json<serde_json::Value>, Error> {
    let repository = PgStatisticsRepository::new(state.db.pool().clone());
    let service = StatisticsService::with_repository(repository);

    match service.get_dashboard_metrics().await {
        Ok(metrics) => Ok(Json(serde_json::json!({
            "status": "ok",
            "stats": {
                "total_orders": metrics.total_orders,
                "total_customers": metrics.total_customers,
                "total_revenue": metrics.total_revenue,
                "pending_orders": metrics.pending_orders,
            }
        }))),
        Err(e) => {
            tracing::error!("Failed to compute admin dashboard stats: {}", e);
            Err(Error::internal("Failed to compute dashboard statistics"))
        }
    }
}

/// List all API keys (admin only)
///
/// GET /api/v1/admin/api-keys
///
/// Returns key metadata only; secrets (key hashes) are never exposed.
pub async fn list_api_keys(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, Error> {
    let keys = state.api_key_repository.list_all().await?;

    let api_keys: Vec<serde_json::Value> = keys
        .into_iter()
        .map(|k| {
            serde_json::json!({
                "id": k.id,
                "key_prefix": k.key_prefix,
                "name": k.name,
                "scopes": k.scopes,
                "is_active": k.is_active,
                "customer_id": k.customer_id,
                "expires_at": k.expires_at,
                "last_used_at": k.last_used_at,
                "rate_limit_per_minute": k.rate_limit_per_minute,
                "created_at": k.created_at,
                "revoked_at": k.revoked_at,
                "revoked_reason": k.revoked_reason,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "api_keys": api_keys
    })))
}

/// Request body for flagging/unflagging an order as a test order.
#[derive(Debug, serde::Deserialize)]
pub struct SetOrderTestFlagBody {
    pub test: bool,
}

/// Flag or unflag an order as a test order (admin only; the whole admin
/// router sits behind the admin auth middleware).
///
/// PATCH /api/v1/admin/orders/:id with {"test": true|false}
///
/// Test orders are excluded from accounting exports; the accounting team
/// flags owner-created manual test orders through this endpoint.
pub async fn set_order_test_flag(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<uuid::Uuid>,
    Json(body): Json<SetOrderTestFlagBody>,
) -> Result<Json<serde_json::Value>, Error> {
    let order = sqlx::query_as::<_, (uuid::Uuid, String, bool)>(
        "UPDATE orders SET test = $1, updated_at = NOW() WHERE id = $2 RETURNING id, order_number, test",
    )
    .bind(body.test)
    .bind(id)
    .fetch_optional(state.db.pool())
    .await?;

    match order {
        Some((id, order_number, test)) => Ok(Json(serde_json::json!({
            "id": id,
            "order_number": order_number,
            "test": test,
        }))),
        None => Err(Error::not_found("Order not found")),
    }
}

/// Router for admin routes
///
/// Route paths already carry the "/admin" prefix; the server merges this
/// router directly under /api/v1 (nesting it under "/admin" again would
/// produce double-prefixed paths like /api/v1/admin/admin/stats).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/stats", get(get_stats))
        .route("/admin/api-keys", get(list_api_keys))
        .route("/admin/orders/:id", patch(set_order_test_flag))
        .merge(products::router())
}
