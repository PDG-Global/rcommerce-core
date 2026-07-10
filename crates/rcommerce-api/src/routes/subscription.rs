//! Subscription API Routes
//!
//! Provides endpoints for:
//! - Creating and managing subscriptions
//! - Viewing subscription details and invoices
//! - Cancelling and pausing subscriptions
//! - Admin subscription management

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::middleware::JwtAuth;
use crate::state::AppState;
use rcommerce_core::models::{
    CancelSubscriptionRequest, CreateSubscriptionRequest, SubscriptionFilter, SubscriptionStatus,
    UpdateSubscriptionRequest,
};
use rcommerce_core::repository::SubscriptionRepository;

/// Query parameters for listing subscriptions
#[derive(Debug, Deserialize)]
pub struct ListSubscriptionsQuery {
    pub status: Option<SubscriptionStatus>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// Create subscription handler
async fn create_subscription(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Json(mut request): Json<CreateSubscriptionRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Set customer_id from authenticated user
    request.customer_id = auth.customer_id;

    match state
        .subscription_service
        .create_subscription(request)
        .await
    {
        Ok(subscription) => Ok(Json(serde_json::json!({
            "success": true,
            "subscription": subscription
        }))),
        Err(e) => {
            tracing::error!("Failed to create subscription: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

/// List subscriptions for authenticated customer
async fn list_my_subscriptions(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Query(query): Query<ListSubscriptionsQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Get customer_id from authenticated user
    let customer_id = auth.customer_id;

    let filter = SubscriptionFilter {
        customer_id: Some(customer_id),
        status: query.status,
        ..Default::default()
    };

    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(20).min(100);

    match state
        .subscription_service
        .list_subscriptions(filter, page, per_page)
        .await
    {
        Ok((subscriptions, total)) => Ok(Json(serde_json::json!({
            "subscriptions": subscriptions,
            "pagination": {
                "page": page,
                "per_page": per_page,
                "total": total,
                "total_pages": (total as f64 / per_page as f64).ceil() as i64
            }
        }))),
        Err(e) => {
            tracing::error!("Failed to list subscriptions: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get subscription by ID
async fn get_subscription(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.subscription_service.get_subscription(id).await {
        Ok(subscription) => Ok(Json(serde_json::json!({
            "subscription": subscription
        }))),
        Err(e) => {
            tracing::error!("Failed to get subscription: {}", e);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

/// Update subscription
async fn update_subscription(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateSubscriptionRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state
        .subscription_service
        .update_subscription(id, request)
        .await
    {
        Ok(subscription) => Ok(Json(serde_json::json!({
            "success": true,
            "subscription": subscription
        }))),
        Err(e) => {
            tracing::error!("Failed to update subscription: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

/// Cancel subscription
async fn cancel_subscription(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<CancelSubscriptionRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state
        .subscription_service
        .cancel_subscription(id, request)
        .await
    {
        Ok(subscription) => Ok(Json(serde_json::json!({
            "success": true,
            "message": "Subscription cancelled successfully",
            "subscription": subscription
        }))),
        Err(e) => {
            tracing::error!("Failed to cancel subscription: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

/// Pause subscription
async fn pause_subscription(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.subscription_service.pause_subscription(id).await {
        Ok(subscription) => Ok(Json(serde_json::json!({
            "success": true,
            "message": "Subscription paused successfully",
            "subscription": subscription
        }))),
        Err(e) => {
            tracing::error!("Failed to pause subscription: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

/// Resume subscription
async fn resume_subscription(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.subscription_service.resume_subscription(id).await {
        Ok(subscription) => Ok(Json(serde_json::json!({
            "success": true,
            "message": "Subscription resumed successfully",
            "subscription": subscription
        }))),
        Err(e) => {
            tracing::error!("Failed to resume subscription: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

/// Get subscription invoices
async fn get_subscription_invoices(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.subscription_repository.list_invoices(id).await {
        Ok(invoices) => Ok(Json(serde_json::json!({
            "invoices": invoices
        }))),
        Err(e) => {
            tracing::error!("Failed to get invoices: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Admin: List all subscriptions
async fn admin_list_subscriptions(
    State(state): State<AppState>,
    Query(query): Query<ListSubscriptionsQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let filter = SubscriptionFilter {
        status: query.status,
        ..Default::default()
    };

    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(20).min(100);

    match state
        .subscription_service
        .list_subscriptions(filter, page, per_page)
        .await
    {
        Ok((subscriptions, total)) => Ok(Json(serde_json::json!({
            "subscriptions": subscriptions,
            "pagination": {
                "page": page,
                "per_page": per_page,
                "total": total,
                "total_pages": (total as f64 / per_page as f64).ceil() as i64
            }
        }))),
        Err(e) => {
            tracing::error!("Failed to list subscriptions: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Admin: Get subscription summary statistics
async fn admin_get_summary(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.subscription_service.get_summary().await {
        Ok(summary) => Ok(Json(serde_json::json!({
            "summary": summary
        }))),
        Err(e) => {
            tracing::error!("Failed to get subscription summary: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Admin: Process due subscriptions (billing run)
async fn admin_process_billing(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.subscription_service.process_due_subscriptions().await {
        Ok(invoices) => Ok(Json(serde_json::json!({
            "success": true,
            "message": format!("Processed {} subscriptions", invoices.len()),
            "invoices_created": invoices.len()
        }))),
        Err(e) => {
            tracing::error!("Failed to process billing: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Router for subscription routes
pub fn router() -> Router<AppState> {
    // Customer routes
    let customer_routes = Router::new()
        .route("/subscriptions", get(list_my_subscriptions))
        .route("/subscriptions", post(create_subscription))
        .route("/subscriptions/:id", get(get_subscription))
        .route("/subscriptions/:id", put(update_subscription))
        .route("/subscriptions/:id/cancel", post(cancel_subscription))
        .route("/subscriptions/:id/pause", post(pause_subscription))
        .route("/subscriptions/:id/resume", post(resume_subscription))
        .route(
            "/subscriptions/:id/invoices",
            get(get_subscription_invoices),
        );

    // Admin routes
    let admin_routes = Router::new()
        .route("/admin/subscriptions", get(admin_list_subscriptions))
        .route("/admin/subscriptions/summary", get(admin_get_summary))
        .route(
            "/admin/subscriptions/process-billing",
            post(admin_process_billing),
        );

    Router::new().merge(customer_routes).merge(admin_routes)
}
