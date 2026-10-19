//! Subscription API Routes
//!
//! Provides endpoints for:
//! - Viewing and managing the authenticated customer's subscriptions
//! - Creating stored-card payment consents (for subscription checkout)
//! - Cancelling and pausing subscriptions
//! - Admin subscription management (mounted under the admin middleware group)

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

/// 404 for subscriptions that don't exist OR aren't the caller's, so foreign
/// ids can't be distinguished from missing ones (no id enumeration).
fn forbidden_or_missing() -> StatusCode {
    StatusCode::NOT_FOUND
}

/// Load a subscription only when it belongs to the caller (or the caller is
/// an admin).
async fn owned_subscription(
    state: &AppState,
    auth: &JwtAuth,
    id: Uuid,
) -> Result<rcommerce_core::models::Subscription, StatusCode> {
    let subscription = state
        .subscription_service
        .get_subscription(id)
        .await
        .map_err(|_| forbidden_or_missing())?;
    if !auth.is_admin() && subscription.customer_id != auth.customer_id {
        return Err(forbidden_or_missing());
    }
    Ok(subscription)
}

/// Create subscription handler (admin only)
///
/// The raw create path bypasses the checkout flow, so it is restricted to
/// admins and the order must belong to the requested customer.
async fn create_subscription(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Json(mut request): Json<CreateSubscriptionRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    // The order being subscribed must belong to the customer.
    let order_owner: Option<Uuid> =
        sqlx::query_scalar("SELECT customer_id FROM orders WHERE id = $1")
            .bind(request.order_id)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| {
                tracing::error!("Failed to look up order ownership: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    match order_owner {
        Some(owner) if owner == request.customer_id => {}
        None => {
            return Err(StatusCode::BAD_REQUEST);
        }
        Some(_) => {
            // Order belongs to a different customer.
            return Err(StatusCode::BAD_REQUEST);
        }
    }

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

/// The caller's subscriptions with product titles and recent invoices.
///
/// GET /api/v1/subscriptions/mine
async fn my_subscriptions(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let subscriptions = state
        .subscription_service
        .get_customer_subscriptions(auth.customer_id, false)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list customer subscriptions: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut out = Vec::with_capacity(subscriptions.len());
    for sub in subscriptions {
        let product_title: Option<String> =
            sqlx::query_scalar("SELECT title FROM products WHERE id = $1")
                .bind(sub.product_id)
                .fetch_optional(state.db.pool())
                .await
                .unwrap_or(None);

        let invoices = state
            .subscription_repository
            .list_invoices(sub.id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to list invoices: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .into_iter()
            .take(12)
            .collect::<Vec<_>>();

        out.push(serde_json::json!({
            "id": sub.id,
            "status": sub.status,
            "product_id": sub.product_id,
            "product_title": product_title,
            "variant_id": sub.variant_id,
            "quantity": sub.quantity,
            "amount": sub.amount,
            "currency": sub.currency,
            "interval": sub.interval,
            "interval_count": sub.interval_count,
            "next_billing_at": sub.next_billing_at,
            "current_cycle": sub.current_cycle,
            "created_at": sub.created_at,
            "invoices": invoices,
        }));
    }

    Ok(Json(serde_json::json!({ "subscriptions": out })))
}

/// Create a stored-card payment consent for subscription checkout.
///
/// POST /api/v1/subscriptions/consents
///
/// Finds (or creates) the customer's Airwallex customer and creates a
/// payment consent. The storefront verifies the consent client-side with the
/// returned client_secret, then passes `consent_id` as `payment_consent_id`
/// to /checkout/complete.
#[derive(Debug, Deserialize)]
pub struct CreateConsentRequest {
    pub card: ConsentCardRequest,
    /// Airwallex merchant_trigger_reason (e.g. "scheduled" for regular
    /// subscription billing, "unscheduled" for ad-hoc top-ups)
    #[serde(default = "default_trigger_reason")]
    pub merchant_trigger_reason: String,
}

#[derive(Debug, Deserialize)]
pub struct ConsentCardRequest {
    pub number: String,
    pub exp_month: u32,
    pub exp_year: u32,
    pub cvc: String,
    pub name: String,
}

fn default_trigger_reason() -> String {
    "scheduled".to_string()
}

async fn create_consent(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Json(request): Json<CreateConsentRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let gateway = state.subscription_gateway.clone().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "No payment gateway available for consents"})),
        )
    })?;

    let customer = state
        .customer_service
        .find_by_id(auth.customer_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Customer not found"})),
            )
        })?;

    // Find or create the customer's gateway (Airwallex) customer.
    let existing: Option<String> =
        sqlx::query_scalar("SELECT airwallex_customer_id FROM customers WHERE id = $1")
            .bind(auth.customer_id)
            .fetch_optional(state.db.pool())
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": e.to_string()})),
                )
            })?
            .flatten();

    let gateway_customer_id = match existing {
        Some(id) if !id.is_empty() => id,
        _ => {
            let name = format!("{} {}", customer.first_name, customer.last_name);
            let id = gateway
                .create_customer(&customer.email, &name)
                .await
                .map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({"error": e.to_string()})),
                    )
                })?;
            sqlx::query("UPDATE customers SET airwallex_customer_id = $1 WHERE id = $2")
                .bind(&id)
                .bind(auth.customer_id)
                .execute(state.db.pool())
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": e.to_string()})),
                    )
                })?;
            id
        }
    };

    let consent = gateway
        .create_payment_consent(
            &gateway_customer_id,
            rcommerce_core::payment::CardDetails {
                number: request.card.number,
                exp_month: request.card.exp_month,
                exp_year: request.card.exp_year,
                cvc: request.card.cvc,
                name: request.card.name,
            },
            &request.merchant_trigger_reason,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "consent": {
            "consent_id": consent.consent_id,
            "client_secret": consent.client_secret,
        }
    })))
}

/// Get subscription by ID (owner or admin only)
async fn get_subscription(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let subscription = owned_subscription(&state, &auth, id).await?;
    Ok(Json(serde_json::json!({
        "subscription": subscription
    })))
}

/// Update subscription (owner or admin only)
async fn update_subscription(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateSubscriptionRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    owned_subscription(&state, &auth, id).await?;
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

/// Cancel subscription (owner or admin only)
async fn cancel_subscription(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
    Json(request): Json<CancelSubscriptionRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    owned_subscription(&state, &auth, id).await?;
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

/// Pause subscription (owner or admin only)
async fn pause_subscription(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    owned_subscription(&state, &auth, id).await?;
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

/// Resume subscription (owner or admin only)
async fn resume_subscription(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    owned_subscription(&state, &auth, id).await?;
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

/// Get subscription invoices (owner or admin only)
async fn get_subscription_invoices(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    owned_subscription(&state, &auth, id).await?;
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

/// Admin: Process due subscriptions (billing run with charging)
async fn admin_process_billing(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let Some(gateway) = state.subscription_gateway.clone() else {
        return Err(StatusCode::BAD_REQUEST);
    };
    match state
        .subscription_service
        .process_due_subscriptions_with_charging(&gateway, &state.order_service)
        .await
    {
        Ok(report) => Ok(Json(serde_json::json!({
            "success": true,
            "message": format!(
                "Processed {} subscriptions: {} charged, {} failed",
                report.processed, report.charged, report.failed
            ),
            "processed": report.processed,
            "charged": report.charged,
            "failed": report.failed,
        }))),
        Err(e) => {
            tracing::error!("Failed to process billing: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Router for customer-facing subscription routes (JWT auth required)
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/subscriptions", get(list_my_subscriptions))
        .route("/subscriptions", post(create_subscription))
        .route("/subscriptions/mine", get(my_subscriptions))
        .route("/subscriptions/consents", post(create_consent))
        .route("/subscriptions/:id", get(get_subscription))
        .route("/subscriptions/:id", put(update_subscription))
        .route("/subscriptions/:id/cancel", post(cancel_subscription))
        .route("/subscriptions/:id/pause", post(pause_subscription))
        .route("/subscriptions/:id/resume", post(resume_subscription))
        .route(
            "/subscriptions/:id/invoices",
            get(get_subscription_invoices),
        )
}

/// Router for admin subscription routes (mounted under admin_middleware)
pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/admin/subscriptions", get(admin_list_subscriptions))
        .route("/admin/subscriptions/summary", get(admin_get_summary))
        .route(
            "/admin/subscriptions/process-billing",
            post(admin_process_billing),
        )
}
