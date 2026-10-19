//! Dunning API Routes
//!
//! Provides endpoints for managing payment retries and dunning workflows:
//! - Admin endpoints for manual processing and monitoring
//! - Customer endpoints for viewing dunning history

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::state::AppState;
use rcommerce_core::models::DunningConfig;
use rcommerce_core::DunningService;

/// Query parameters for listing pending retries
#[derive(Debug, Deserialize)]
pub struct ListRetriesQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// Admin: Trigger manual dunning processing
///
/// POST /api/v1/admin/dunning/process
async fn admin_process_dunning(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let Some(gateway) = state.subscription_gateway.clone() else {
        return Err(StatusCode::BAD_REQUEST);
    };

    // Create dunning service
    let dunning_service = DunningService::with_config(
        (*state.subscription_repository).clone(),
        DunningConfig::default(),
    );

    match dunning_service
        .process_all_due_retries(gateway.as_ref(), Some(&state.order_service))
        .await
    {
        Ok(result) => Ok(Json(serde_json::json!({
            "success": true,
            "message": format!("Processed {} retries", result.processed),
            "result": {
                "processed": result.processed,
                "succeeded": result.succeeded,
                "failed": result.failed,
                "pending": result.pending,
            }
        }))),
        Err(e) => {
            tracing::error!("Failed to process dunning: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// List pending retries
///
/// GET /api/v1/admin/dunning/retries
async fn admin_list_pending_retries(
    State(state): State<AppState>,
    Query(query): Query<ListRetriesQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let dunning_service = DunningService::with_config(
        (*state.subscription_repository).clone(),
        state.subscription_service.config().clone(),
    );

    match dunning_service.get_pending_retries().await {
        Ok(invoices) => {
            let page = query.page.unwrap_or(1);
            let per_page = query.per_page.unwrap_or(20).min(100);

            let total: i64 = invoices.len() as i64;
            let start = ((page - 1) * per_page) as usize;
            let _end = (start + per_page as usize).min(invoices.len());

            let paginated: Vec<_> = invoices
                .into_iter()
                .skip(start)
                .take(per_page as usize)
                .collect();

            Ok(Json(serde_json::json!({
                "retries": paginated,
                "pagination": {
                    "page": page,
                    "per_page": per_page,
                    "total": total,
                    "total_pages": (total as f64 / per_page as f64).ceil() as i64
                }
            })))
        }
        Err(e) => {
            tracing::error!("Failed to list pending retries: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get invoices that need retry
///
/// GET /api/v1/admin/dunning/retries/due
async fn admin_list_due_retries(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let dunning_service = DunningService::with_config(
        (*state.subscription_repository).clone(),
        state.subscription_service.config().clone(),
    );

    match dunning_service.get_invoices_for_retry().await {
        Ok(retryable) => Ok(Json(serde_json::json!({
            "invoices": retryable,
            "total": retryable.len() as i64
        }))),
        Err(e) => {
            tracing::error!("Failed to list due retries: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Admin: Manually trigger a retry for a specific invoice
///
/// POST /api/v1/admin/dunning/retry/:invoice_id
async fn admin_manual_retry(
    State(state): State<AppState>,
    Path(invoice_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let Some(gateway) = state.subscription_gateway.clone() else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let dunning_service = DunningService::with_config(
        (*state.subscription_repository).clone(),
        state.subscription_service.config().clone(),
    );

    match dunning_service
        .manual_retry(invoice_id, gateway.as_ref(), Some(&state.order_service))
        .await
    {
        Ok(result) => {
            let response = match result {
                rcommerce_core::models::PaymentRecoveryResult::Success => {
                    serde_json::json!({
                        "success": true,
                        "message": "Payment retry successful",
                        "status": "success"
                    })
                }
                rcommerce_core::models::PaymentRecoveryResult::RetryScheduled {
                    next_retry_at,
                    attempt_number,
                    max_attempts,
                } => {
                    serde_json::json!({
                        "success": true,
                        "message": "Payment retry scheduled",
                        "status": "retry_scheduled",
                        "next_retry_at": next_retry_at,
                        "attempt_number": attempt_number,
                        "max_attempts": max_attempts
                    })
                }
                rcommerce_core::models::PaymentRecoveryResult::FailedPermanent {
                    cancelled_at,
                    reason,
                } => {
                    serde_json::json!({
                        "success": false,
                        "message": reason,
                        "status": "failed_permanent",
                        "cancelled_at": cancelled_at
                    })
                }
            };

            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!(
                "Failed to execute manual retry for invoice {}: {}",
                invoice_id,
                e
            );
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

/// Get dunning history for a subscription (owner or admin only)
///
/// GET /api/v1/subscriptions/:id/dunning-history
async fn get_dunning_history(
    State(state): State<AppState>,
    Extension(auth): Extension<crate::middleware::JwtAuth>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // 404 for foreign subscriptions (no id enumeration)
    let subscription = state
        .subscription_service
        .get_subscription(id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    if !auth.is_admin() && subscription.customer_id != auth.customer_id {
        return Err(StatusCode::NOT_FOUND);
    }

    let dunning_service = DunningService::with_config(
        (*state.subscription_repository).clone(),
        state.subscription_service.config().clone(),
    );

    match dunning_service.get_dunning_history(id).await {
        Ok(history) => Ok(Json(serde_json::json!({
            "subscription_id": history.subscription_id,
            "subscription_status": history.subscription_status,
            "total_attempts": history.total_attempts,
            "is_cancelled": history.is_cancelled,
            "retry_attempts": history.retry_attempts,
            "emails_sent": history.emails_sent
        }))),
        Err(e) => {
            tracing::error!(
                "Failed to get dunning history for subscription {}: {}",
                id,
                e
            );
            Err(StatusCode::NOT_FOUND)
        }
    }
}

/// Get dunning configuration
///
/// GET /api/v1/admin/dunning/config
async fn admin_get_config(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config = state.subscription_service.config();

    Json(serde_json::json!({
        "enabled": true,
        "max_retries": config.max_retries,
        "retry_intervals_days": config.retry_intervals_days,
        "grace_period_days": config.grace_period_days,
        "email_on_first_failure": config.email_on_first_failure,
        "email_on_final_failure": config.email_on_final_failure,
        "late_fee_after_retry": config.late_fee_after_retry,
        "late_fee_amount": config.late_fee_amount,
    }))
}

/// Process a failed payment (webhook handler)
///
/// POST /api/v1/admin/dunning/failed-payment
#[derive(Debug, Deserialize)]
pub struct FailedPaymentRequest {
    pub subscription_id: Uuid,
    pub invoice_id: Uuid,
    pub error_message: String,
}

async fn admin_process_failed_payment(
    State(state): State<AppState>,
    Json(request): Json<FailedPaymentRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let dunning_service = DunningService::with_config(
        (*state.subscription_repository).clone(),
        state.subscription_service.config().clone(),
    );

    match dunning_service
        .process_failed_payment(
            request.subscription_id,
            request.invoice_id,
            &request.error_message,
        )
        .await
    {
        Ok(result) => {
            let response = match result {
                rcommerce_core::models::PaymentRecoveryResult::Success => {
                    serde_json::json!({
                        "success": true,
                        "message": "Payment processed successfully",
                        "status": "success"
                    })
                }
                rcommerce_core::models::PaymentRecoveryResult::RetryScheduled {
                    next_retry_at,
                    attempt_number,
                    max_attempts,
                } => {
                    serde_json::json!({
                        "success": true,
                        "message": "Payment failed, retry scheduled",
                        "status": "retry_scheduled",
                        "next_retry_at": next_retry_at,
                        "attempt_number": attempt_number,
                        "max_attempts": max_attempts
                    })
                }
                rcommerce_core::models::PaymentRecoveryResult::FailedPermanent {
                    cancelled_at,
                    reason,
                } => {
                    serde_json::json!({
                        "success": false,
                        "message": reason,
                        "status": "failed_permanent",
                        "cancelled_at": cancelled_at
                    })
                }
            };

            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("Failed to process failed payment: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

/// Process a payment recovery (webhook handler)
///
/// POST /api/v1/admin/dunning/recovery
#[derive(Debug, Deserialize)]
pub struct PaymentRecoveryRequest {
    pub subscription_id: Uuid,
    pub invoice_id: Uuid,
    pub payment_id: String,
}

async fn admin_process_recovery(
    State(state): State<AppState>,
    Json(request): Json<PaymentRecoveryRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let dunning_service = DunningService::with_config(
        (*state.subscription_repository).clone(),
        state.subscription_service.config().clone(),
    );

    match dunning_service
        .process_recovery(
            request.subscription_id,
            request.invoice_id,
            request.payment_id,
        )
        .await
    {
        Ok(_) => Ok(Json(serde_json::json!({
            "success": true,
            "message": "Payment recovery processed successfully"
        }))),
        Err(e) => {
            tracing::error!("Failed to process payment recovery: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

/// Reset dunning state for a subscription (owner or admin only; after a
/// payment method update)
///
/// POST /api/v1/subscriptions/:id/reset-dunning
async fn reset_dunning_state(
    State(state): State<AppState>,
    Extension(auth): Extension<crate::middleware::JwtAuth>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let subscription = state
        .subscription_service
        .get_subscription(id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;
    if !auth.is_admin() && subscription.customer_id != auth.customer_id {
        return Err(StatusCode::NOT_FOUND);
    }

    let dunning_service = DunningService::with_config(
        (*state.subscription_repository).clone(),
        state.subscription_service.config().clone(),
    );

    match dunning_service.reset_dunning_state(id).await {
        Ok(_) => Ok(Json(serde_json::json!({
            "success": true,
            "message": "Dunning state reset successfully"
        }))),
        Err(e) => {
            tracing::error!(
                "Failed to reset dunning state for subscription {}: {}",
                id,
                e
            );
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

/// Get dunning statistics
///
/// GET /api/v1/admin/dunning/stats
async fn admin_get_stats(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let dunning_service = DunningService::with_config(
        (*state.subscription_repository).clone(),
        state.subscription_service.config().clone(),
    );

    match dunning_service.get_pending_retries().await {
        Ok(pending) => match dunning_service.get_invoices_for_retry().await {
            Ok(due) => Ok(Json(serde_json::json!({
                "pending_retries": pending.len(),
                "invoices_due": due.len(),
                "config": {
                    "max_retries": state.subscription_service.config().max_retries,
                    "retry_intervals_days": state.subscription_service.config().retry_intervals_days,
                    "grace_period_days": state.subscription_service.config().grace_period_days,
                }
            }))),
            Err(e) => {
                tracing::error!("Failed to get invoices for retry: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        },
        Err(e) => {
            tracing::error!("Failed to get pending retries: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Router for customer-facing dunning routes (JWT auth required)
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/subscriptions/:id/dunning-history",
            get(get_dunning_history),
        )
        .route(
            "/subscriptions/:id/reset-dunning",
            post(reset_dunning_state),
        )
}

/// Router for admin dunning routes (mounted under admin_middleware)
pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/admin/dunning/process", post(admin_process_dunning))
        .route("/admin/dunning/retries", get(admin_list_pending_retries))
        .route("/admin/dunning/retries/due", get(admin_list_due_retries))
        .route("/admin/dunning/retry/:invoice_id", post(admin_manual_retry))
        .route("/admin/dunning/config", get(admin_get_config))
        .route("/admin/dunning/stats", get(admin_get_stats))
        .route(
            "/admin/dunning/failed-payment",
            post(admin_process_failed_payment),
        )
        .route("/admin/dunning/recovery", post(admin_process_recovery))
}
