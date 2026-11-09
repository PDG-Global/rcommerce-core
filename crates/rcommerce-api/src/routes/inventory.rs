//! Stock management routes for the back-office (`/admin/inventory/...`):
//! stock levels per location, manual adjustments, goods receipts, reorder
//! settings and the movement history. See docs/api/06-inventory-api.md.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post},
    Extension, Json, Router,
};
use rcommerce_core::inventory::stock_admin::{
    self, AdjustmentInput, LevelFilter, MovementFilter, ReceiptInput, ReorderInput,
};
use rcommerce_core::services::{Action, Resource};
use uuid::Uuid;

use crate::middleware::JwtAuth;
use crate::state::AppState;

type ApiResult<T> = Result<T, rcommerce_core::Error>;

/// Inventory access: global admins, or callers holding the inventory scope
fn require(auth: &JwtAuth, action: Action) -> ApiResult<()> {
    if auth.is_admin() || auth.can(Resource::Inventory, action) {
        Ok(())
    } else {
        Err(rcommerce_core::Error::forbidden(
            "Inventory access required",
        ))
    }
}

/// `{page, per_page, total, total_pages}` for a list response
fn pagination(page: Option<i64>, per_page: Option<i64>, total: i64) -> serde_json::Value {
    let per_page = per_page.unwrap_or(50).clamp(1, 200);
    serde_json::json!({
        "page": page.unwrap_or(1).max(1),
        "per_page": per_page,
        "total": total,
        "total_pages": (total + per_page - 1) / per_page,
    })
}

/// GET /admin/inventory/levels?location_id&product_id&q&low_only&page&per_page
async fn list_levels(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Query(filter): Query<LevelFilter>,
) -> ApiResult<Json<serde_json::Value>> {
    require(&auth, Action::Read)?;
    let (levels, total) = stock_admin::list_levels(state.db.pool(), &filter).await?;
    Ok(Json(serde_json::json!({
        "levels": levels,
        "pagination": pagination(filter.page, filter.per_page, total),
    })))
}

/// PATCH /admin/inventory/levels/:id {reorder_point, reorder_quantity}
async fn set_reorder(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
    Json(input): Json<ReorderInput>,
) -> ApiResult<Json<serde_json::Value>> {
    require(&auth, Action::Write)?;
    let level = stock_admin::set_reorder(state.db.pool(), id, &input).await?;
    Ok(Json(serde_json::json!({ "level": level })))
}

/// POST /admin/inventory/adjustments
async fn adjust(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Json(input): Json<AdjustmentInput>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    require(&auth, Action::Write)?;
    let level = stock_admin::adjust(state.db.pool(), &input, auth.customer_id).await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "level": level })),
    ))
}

/// POST /admin/inventory/receipts
async fn receive(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Json(input): Json<ReceiptInput>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    require(&auth, Action::Write)?;
    let levels = stock_admin::receive(state.db.pool(), &input, auth.customer_id).await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "levels": levels })),
    ))
}

/// GET /admin/inventory/movements?location_id&product_id&from&to&page&per_page
async fn list_movements(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Query(filter): Query<MovementFilter>,
) -> ApiResult<Json<serde_json::Value>> {
    require(&auth, Action::Read)?;
    let (movements, total) = stock_admin::list_movements(state.db.pool(), &filter).await?;
    Ok(Json(serde_json::json!({
        "movements": movements,
        "pagination": pagination(filter.page, filter.per_page, total),
    })))
}

/// Stock management routes; the caller mounts them behind the admin middleware
pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/admin/inventory/levels", get(list_levels))
        .route("/admin/inventory/levels/:id", patch(set_reorder))
        .route("/admin/inventory/adjustments", post(adjust))
        .route("/admin/inventory/receipts", post(receive))
        .route("/admin/inventory/movements", get(list_movements))
}

#[cfg(test)]
mod tests {
    use super::pagination;

    #[test]
    fn pagination_rounds_pages_up_and_clamps_the_size() {
        let p = pagination(Some(2), Some(20), 41);
        assert_eq!(p["total_pages"], 3);
        assert_eq!(p["page"], 2);
        let p = pagination(None, Some(1000), 0);
        assert_eq!(p["per_page"], 200);
        assert_eq!(p["total_pages"], 0);
    }
}
