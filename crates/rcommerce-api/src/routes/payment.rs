//! Payment API Routes - Agnostic Payment System
//!
//! Provides unified endpoints for all payment operations regardless of gateway.
//! Frontend never communicates directly with payment providers.

use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use crate::state::AppState;
use rcommerce_core::payment::agnostic::*;
use rcommerce_core::Error;

/// Get available payment methods for a checkout
#[derive(Debug, Deserialize)]
pub struct GetPaymentMethodsRequest {
    pub currency: String,
    pub amount: String,
}

/// Response with available payment methods
#[derive(Debug, Serialize)]
pub struct GetPaymentMethodsResponse {
    pub gateway_id: String,
    pub gateway_name: String,
    pub payment_methods: Vec<PaymentMethodConfig>,
}

/// Get available payment methods
pub async fn get_payment_methods(
    State(state): State<AppState>,
    Json(request): Json<GetPaymentMethodsRequest>,
) -> Result<Json<Vec<GetPaymentMethodsResponse>>, Error> {
    // Parse amount
    let amount = request
        .amount
        .parse::<rust_decimal::Decimal>()
        .map_err(|_| Error::validation("Invalid amount format"))?;

    // Get available payment methods from payment service
    let methods = state
        .payment_service
        .get_available_payment_methods(&request.currency, amount)
        .await;

    // Group by gateway
    let mut gateway_methods: std::collections::HashMap<String, Vec<PaymentMethodConfig>> =
        std::collections::HashMap::new();

    for (gateway_id, method) in methods {
        gateway_methods.entry(gateway_id).or_default().push(method);
    }

    // Build response
    let mut responses = Vec::new();
    for (gateway_id, methods) in gateway_methods {
        // Get gateway name from the first method's gateway
        let gateway_name = if gateway_id == "stripe" {
            "Stripe".to_string()
        } else if gateway_id == "mock" {
            "Mock Gateway".to_string()
        } else {
            gateway_id.clone()
        };

        responses.push(GetPaymentMethodsResponse {
            gateway_id,
            gateway_name,
            payment_methods: methods,
        });
    }

    // If no gateways configured, return empty list
    Ok(Json(responses))
}

/// Initiate a payment request
#[derive(Debug, Deserialize)]
pub struct InitiatePaymentApiRequest {
    pub gateway_id: String,
    pub amount: String,
    pub currency: String,
    pub payment_method_type: PaymentMethodType,
    pub order_id: String,
    pub customer_email: String,
    pub payment_method_data: PaymentMethodData,
    pub save_payment_method: bool,
    pub description: String,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

/// Initiate a payment
pub async fn initiate_payment(
    State(state): State<AppState>,
    Json(request): Json<InitiatePaymentApiRequest>,
) -> Result<Json<InitiatePaymentResponse>, Error> {
    // Parse amount
    let amount = request
        .amount
        .parse::<rust_decimal::Decimal>()
        .map_err(|_| Error::validation("Invalid amount format"))?;

    // Parse order_id
    let order_id = Uuid::parse_str(&request.order_id)
        .map_err(|_| Error::validation("Invalid order_id format"))?;

    // Log idempotency key if provided
    if let Some(ref key) = request.idempotency_key {
        info!("Processing payment with idempotency key: {}", key);
    }

    // Get the gateway
    let gateway = state
        .payment_service
        .get_gateway(Some(&request.gateway_id))
        .ok_or_else(|| Error::validation(format!("Gateway '{}' not found", request.gateway_id)))?;

    // Build the initiate payment request
    let initiate_request = InitiatePaymentRequest {
        amount,
        currency: request.currency,
        payment_method_type: request.payment_method_type,
        order_id,
        customer_id: None, // Could be extracted from auth context
        customer_email: request.customer_email,
        customer_ip: None,
        billing_address: None,
        shipping_address: None,
        payment_method_data: request.payment_method_data,
        save_payment_method: request.save_payment_method,
        description: request.description,
        metadata: serde_json::json!({
            "order_id": order_id.to_string(),
            "idempotency_key": request.idempotency_key,
        }),
    };

    // Process the payment
    let response = gateway.initiate_payment(initiate_request).await?;

    // Log the result
    match &response {
        InitiatePaymentResponse::Success { payment_id, .. } => {
            info!("Payment initiated successfully: {}", payment_id);
        }
        InitiatePaymentResponse::RequiresAction {
            payment_id,
            action_type,
            ..
        } => {
            info!(
                "Payment requires action: {}, type: {:?}",
                payment_id, action_type
            );
        }
        InitiatePaymentResponse::Failed {
            payment_id,
            error_message,
            ..
        } => {
            warn!("Payment failed: {}, error: {}", payment_id, error_message);
        }
    }

    Ok(Json(response))
}

/// Complete a payment action (3DS, redirect return, etc.)
#[derive(Debug, Deserialize)]
pub struct CompletePaymentActionApiRequest {
    pub action_type: PaymentActionType,
    pub action_data: serde_json::Value,
}

pub async fn complete_payment_action(
    State(state): State<AppState>,
    Path(payment_id): Path<String>,
    Json(request): Json<CompletePaymentActionApiRequest>,
) -> Result<Json<CompletePaymentActionResponse>, Error> {
    // Get the gateway - we need to determine which gateway this payment belongs to
    // For now, we'll use the default gateway from the payment service
    // In production, you'd look up the payment in the database to find the gateway
    let gateway = state
        .payment_service
        .get_gateway(None)
        .ok_or_else(|| Error::payment_error("No payment gateway configured"))?;

    // Build the complete action request
    let complete_request = CompletePaymentActionRequest {
        payment_id: payment_id.clone(),
        action_type: request.action_type,
        action_data: request.action_data,
    };

    // Complete the payment action
    let response = gateway.complete_payment_action(complete_request).await?;

    // Log the result
    match &response {
        CompletePaymentActionResponse::Success { payment_id, .. } => {
            info!("Payment action completed successfully: {}", payment_id);
        }
        CompletePaymentActionResponse::RequiresAction { payment_id, .. } => {
            info!("Payment still requires action: {}", payment_id);
        }
        CompletePaymentActionResponse::Failed {
            payment_id,
            error_message,
            ..
        } => {
            warn!(
                "Payment action failed: {}, error: {}",
                payment_id, error_message
            );
        }
    }

    Ok(Json(response))
}

/// Payment status response
#[derive(Debug, Serialize)]
pub struct PaymentStatusResponse {
    pub payment_id: String,
    pub status: PaymentStatus,
    pub amount: String,
    pub currency: String,
    pub gateway_id: String,
    pub transaction_id: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
}

/// A payment row as stored in the `payments` table
#[derive(Debug, sqlx::FromRow)]
struct PaymentRow {
    id: Uuid,
    order_id: Uuid,
    amount: rust_decimal::Decimal,
    currency: String,
    gateway: String,
    gateway_payment_id: Option<String>,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Look up a stored payment by the gateway's payment/transaction id
async fn find_payment_by_gateway_id(
    db: &rcommerce_core::repository::Database,
    gateway_payment_id: &str,
) -> Result<Option<PaymentRow>, Error> {
    let row = sqlx::query_as::<_, PaymentRow>(
        r#"
        SELECT id, order_id, amount, currency::text AS currency, gateway,
               gateway_payment_id, status::text AS status, created_at, updated_at
        FROM payments
        WHERE gateway_payment_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(gateway_payment_id)
    .fetch_optional(db.pool())
    .await?;

    Ok(row)
}

/// Get payment status
pub async fn get_payment_status(
    State(state): State<AppState>,
    Path(payment_id): Path<String>,
) -> Result<Json<PaymentStatusResponse>, Error> {
    // Get the gateway
    let gateway = state
        .payment_service
        .get_gateway(None)
        .ok_or_else(|| Error::payment_error("No payment gateway configured"))?;

    // Get payment status from gateway
    let status = gateway.get_payment_status(&payment_id).await?;

    // Enrich the response with the stored payment record when available
    let payment_row = find_payment_by_gateway_id(&state.db, &payment_id).await?;

    info!("Retrieved payment status for {}: {:?}", payment_id, status);

    let (amount, currency, gateway_id, transaction_id, created_at, updated_at) = match payment_row {
        Some(row) => (
            row.amount.to_string(),
            row.currency,
            row.gateway,
            row.gateway_payment_id,
            row.created_at.to_rfc3339(),
            row.updated_at.map(|t| t.to_rfc3339()),
        ),
        None => (
            String::new(),
            String::new(),
            String::new(),
            Some(payment_id.clone()),
            chrono::Utc::now().to_rfc3339(),
            None,
        ),
    };

    Ok(Json(PaymentStatusResponse {
        payment_id: payment_id.clone(),
        status,
        amount,
        currency,
        gateway_id,
        transaction_id,
        created_at,
        updated_at,
    }))
}

/// Refund a payment
#[derive(Debug, Deserialize)]
pub struct RefundRequest {
    pub amount: Option<String>,
    pub reason: String,
}

pub async fn refund_payment(
    State(state): State<AppState>,
    Path(payment_id): Path<String>,
    Json(request): Json<RefundRequest>,
) -> Result<Json<RefundResponse>, Error> {
    // Parse amount if provided
    let amount = match request.amount {
        Some(amt_str) => {
            let amt = amt_str
                .parse::<rust_decimal::Decimal>()
                .map_err(|_| Error::validation("Invalid amount format"))?;
            Some(amt)
        }
        None => None,
    };

    // Get the gateway
    let gateway = state
        .payment_service
        .get_gateway(None)
        .ok_or_else(|| Error::payment_error("No payment gateway configured"))?;

    // Process the refund
    let response = gateway
        .refund_payment(&payment_id, amount, &request.reason)
        .await?;

    // Persist a refunds row for the accounting export (idempotent on the
    // gateway refund id; the webhook finalizes status/settled_at later).
    // A refund whose payment we never recorded locally can't be linked — skip.
    match find_payment_by_gateway_id(&state.db, &response.payment_id)
        .await?
        .or(find_payment_by_gateway_id(&state.db, &payment_id).await?)
    {
        Some(payment) => {
            if let Err(e) = record_refund_row(
                &state.db,
                &payment,
                response.amount,
                &response.currency,
                Some(&response.reason),
                refund_status_for_db(&response.status),
                &response.refund_id,
                None,
            )
            .await
            {
                // The refund itself succeeded; a failed bookkeeping insert must
                // not turn the API call into an error.
                tracing::warn!(
                    "Refund {} succeeded but recording the refunds row failed: {}",
                    response.refund_id,
                    e
                );
            }
        }
        None => {
            tracing::warn!(
                "Refund {} (payment {}) has no local payments row; refunds record skipped",
                response.refund_id,
                payment_id
            );
        }
    }

    info!(
        "Refund processed for payment {}: refund_id={}, status={:?}",
        payment_id, response.refund_id, response.status
    );

    Ok(Json(response))
}

/// Map a gateway refund status to the `payment_status` enum used by the
/// refunds table ('pending' covers in-flight async refunds; the webhook
/// transitions the row to 'refunded' once the gateway settles).
fn refund_status_for_db(status: &rcommerce_core::payment::agnostic::RefundStatus) -> &'static str {
    use rcommerce_core::payment::agnostic::RefundStatus;
    match status {
        RefundStatus::Succeeded => "refunded",
        RefundStatus::Failed => "failed",
        RefundStatus::Pending | RefundStatus::Processing => "pending",
    }
}

/// Insert a refunds row unless one with the same gateway_refund_id exists.
/// `settled_at` is Some for already-settled refunds (webhook path) and None
/// for async refunds whose settlement the webhook will record later.
async fn record_refund_row(
    db: &rcommerce_core::repository::Database,
    payment: &PaymentRow,
    amount: rust_decimal::Decimal,
    currency: &str,
    reason: Option<&str>,
    status: &str,
    gateway_refund_id: &str,
    settled_at: Option<chrono::DateTime<chrono::Utc>>,
) -> rcommerce_core::Result<u64> {
    let result = sqlx::query(
        r#"
        INSERT INTO refunds (id, payment_id, order_id, amount, currency, reason, status,
                             gateway_refund_id, settled_at, created_at, updated_at)
        SELECT $1, $2, $3, $4, UPPER($5)::currency, $6, $7::payment_status, $8, $9, NOW(), NOW()
        WHERE NOT EXISTS (SELECT 1 FROM refunds WHERE gateway_refund_id = $8)
        "#,
    )
    .bind(uuid::Uuid::new_v4())
    .bind(payment.id)
    .bind(payment.order_id)
    .bind(amount)
    .bind(currency)
    .bind(reason)
    .bind(status)
    .bind(gateway_refund_id)
    .bind(settled_at)
    .execute(db.pool())
    .await?;

    Ok(result.rows_affected())
}

/// Extract amount/currency from a refund webhook payload when the gateway
/// includes them (Airwallex puts them on the refund object in `data`).
fn refund_amount_currency_from_payload(
    data: &serde_json::Value,
) -> Option<(rust_decimal::Decimal, String)> {
    let amount = data.get("amount")?;
    // Parse numbers via their shortest string form: serde_json renders the
    // round-trip representation ("112.61"), which parses exactly, whereas an
    // f64 → Decimal conversion retains binary float dust.
    let amount = match amount {
        serde_json::Value::Number(n) => n.to_string().parse::<rust_decimal::Decimal>().ok(),
        serde_json::Value::String(s) => s.parse::<rust_decimal::Decimal>().ok(),
        _ => None,
    }?;
    let currency = data
        .get("currency")
        .and_then(|c| c.as_str())
        .map(|c| c.to_string())?;
    Some((amount, currency))
}

/// Save a payment method for future use
#[derive(Debug, Deserialize)]
pub struct SavePaymentMethodRequest {
    pub gateway_id: String,
    pub payment_method_data: PaymentMethodData,
}

pub async fn save_payment_method(
    State(state): State<AppState>,
    Json(request): Json<SavePaymentMethodRequest>,
) -> Result<Json<PaymentMethodToken>, Error> {
    // Get the gateway
    let gateway = state
        .payment_service
        .get_gateway(Some(&request.gateway_id))
        .ok_or_else(|| Error::validation(format!("Gateway '{}' not found", request.gateway_id)))?;

    // Tokenize the payment method
    let token = gateway
        .tokenize_payment_method(request.payment_method_data)
        .await?;

    info!("Payment method tokenized: {}", token.token);

    Ok(Json(token))
}

/// Get saved payment methods for a customer
pub async fn get_saved_payment_methods(
    State(state): State<AppState>,
    Path(customer_id): Path<String>,
) -> Result<Json<Vec<PaymentMethodInfo>>, Error> {
    // Get the default gateway
    let gateway = state
        .payment_service
        .get_gateway(None)
        .ok_or_else(|| Error::payment_error("No payment gateway configured"))?;

    // Get saved payment methods
    let methods = gateway.get_saved_payment_methods(&customer_id).await?;

    info!(
        "Retrieved {} saved payment methods for customer {}",
        methods.len(),
        customer_id
    );

    Ok(Json(methods))
}

/// Delete a saved payment method response
#[derive(Debug, Serialize)]
pub struct DeletePaymentMethodResponse {
    pub success: bool,
    pub message: String,
    pub token: String,
}

/// Delete a saved payment method
pub async fn delete_payment_method(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<DeletePaymentMethodResponse>, Error> {
    // Get the default gateway
    let gateway = state
        .payment_service
        .get_gateway(None)
        .ok_or_else(|| Error::payment_error("No payment gateway configured"))?;

    // Delete the payment method
    gateway.delete_payment_method(&token).await?;

    info!("Payment method deleted: {}", token);

    Ok(Json(DeletePaymentMethodResponse {
        success: true,
        message: "Payment method deleted successfully".to_string(),
        token,
    }))
}

/// Webhook response
#[derive(Debug, Serialize)]
pub struct WebhookResponse {
    pub success: bool,
    pub message: String,
    pub event_type: Option<String>,
}

/// Handle webhooks from payment providers
pub async fn handle_webhook(
    State(state): State<AppState>,
    Path(gateway_id): Path<String>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<Json<WebhookResponse>, Error> {
    // Get the gateway
    let gateway = state
        .payment_service
        .get_gateway(Some(&gateway_id))
        .ok_or_else(|| Error::validation(format!("Gateway '{}' not found", gateway_id)))?;

    // Extract headers for signature verification
    let header_vec: Vec<(String, String)> = headers
        .iter()
        .filter_map(|(k, v)| {
            let key = k.to_string();
            let value = v.to_str().ok()?.to_string();
            Some((key, value))
        })
        .collect();

    // Process the webhook
    let event = gateway.handle_webhook(body.as_bytes(), &header_vec).await?;

    info!(
        "Webhook received from {}: event_type={:?}, payment_id={}",
        gateway_id, event.event_type, event.payment_id
    );

    // Handle different event types
    match webhook_action(&event.event_type) {
        WebhookAction::ConfirmPayment => {
            info!("Payment succeeded: {}", event.payment_id);
            handle_payment_succeeded(&state, &event).await?;
        }
        WebhookAction::FailPayment => {
            warn!("Payment failed: {}", event.payment_id);
            handle_payment_failed(&state, &event).await?;
        }
        WebhookAction::CancelPayment => {
            warn!("Payment cancelled: {}", event.payment_id);
            handle_payment_cancelled(&state, &event).await?;
        }
        WebhookAction::MarkRefunded => {
            info!("Payment refunded: {}", event.payment_id);
            handle_payment_refunded(&state, &event).await?;
        }
        WebhookAction::ConsentVerified => {
            // Stored-card consent verified at the gateway. Consent status is
            // verified synchronously at checkout, so this is informational.
            info!("Payment consent verified at gateway: {}", event.payment_id);
        }
        WebhookAction::Ignore => {
            info!("Unhandled webhook event: {:?}", event.event_type);
        }
    }

    Ok(Json(WebhookResponse {
        success: true,
        message: "Webhook processed successfully".to_string(),
        event_type: Some(format!("{:?}", event.event_type)),
    }))
}

/// What the webhook handler should do with a given event type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WebhookAction {
    /// Mark the payment paid and confirm the order
    ConfirmPayment,
    /// Mark the payment failed; the order stays pending so the customer can retry
    FailPayment,
    /// Mark the payment cancelled and cancel the order
    CancelPayment,
    /// Mark the payment refunded
    MarkRefunded,
    /// A stored-card consent was verified (log only)
    ConsentVerified,
    /// Log only
    Ignore,
}

fn webhook_action(event_type: &WebhookEventType) -> WebhookAction {
    match event_type {
        WebhookEventType::PaymentSucceeded => WebhookAction::ConfirmPayment,
        WebhookEventType::PaymentFailed => WebhookAction::FailPayment,
        WebhookEventType::PaymentCancelled => WebhookAction::CancelPayment,
        WebhookEventType::PaymentRefunded => WebhookAction::MarkRefunded,
        WebhookEventType::SubscriptionPaymentSucceeded => WebhookAction::Ignore,
        WebhookEventType::SubscriptionPaymentFailed => WebhookAction::Ignore,
        WebhookEventType::SubscriptionCreated => WebhookAction::Ignore,
        WebhookEventType::SubscriptionCancelled => WebhookAction::Ignore,
        WebhookEventType::PaymentConsentVerified => WebhookAction::ConsentVerified,
        _ => WebhookAction::Ignore,
    }
}

/// Handle a successful payment webhook:
/// mark the payment paid, confirm the order (committing inventory), and send
/// the order-confirmation email.
async fn handle_payment_succeeded(state: &AppState, event: &WebhookEvent) -> Result<(), Error> {
    let payment = find_payment_by_gateway_id(&state.db, &event.payment_id).await?;
    let Some(payment) = payment else {
        warn!(
            "payment_intent.succeeded for unknown payment {}; nothing to update",
            event.payment_id
        );
        return Ok(());
    };

    if payment.status == "paid" {
        // The payment row is updated before the order is confirmed, so a
        // previous attempt may have recorded the payment but failed to
        // confirm the order (Airwallex retries webhooks). Fall through and
        // confirm the order below if it is still pending instead of skipping.
        info!(
            "Payment {} already marked paid; checking whether order still needs confirmation",
            event.payment_id
        );
    } else {
        sqlx::query(
            "UPDATE payments SET status = 'paid', processed_at = NOW(), updated_at = NOW() WHERE id = $1",
        )
        .bind(payment.id)
        .execute(state.db.pool())
        .await?;
    }

    // Confirm the order if it is still awaiting payment. Orders already
    // confirmed (e.g. by the checkout confirm endpoint racing the webhook)
    // are left alone; this keeps the handler idempotent.
    let order_status: Option<String> =
        sqlx::query_scalar("SELECT status::text FROM orders WHERE id = $1")
            .bind(payment.order_id)
            .fetch_optional(state.db.pool())
            .await?;

    match order_status.as_deref() {
        Some("pending") => {
            let detail = state
                .order_service
                .confirm_order_after_payment(payment.order_id)
                .await?;

            // A paid subscription-initial order starts the subscription.
            super::checkout::finalize_subscription_for_order(state, payment.order_id).await;

            // Notify the shop operator about the now-paid order
            super::email_helpers::send_admin_order_email(state, payment.order_id).await;

            // Fire-and-forget order confirmation email if SMTP configured
            if let Some(ref svc) = state.notification_service {
                let order_id = payment.order_id;
                let customer_email = detail.order.customer_email.clone();
                let order_service = state.order_service.clone();
                let customer_service = state.customer_service.clone();
                let db = state.db.clone();
                let branding = state.branding.clone();
                let svc = svc.clone();
                tokio::spawn(async move {
                    if let Err(e) = send_order_confirmation_email(
                        &svc,
                        &order_service,
                        &customer_service,
                        &db,
                        &branding,
                        order_id,
                        &customer_email,
                    )
                    .await
                    {
                        tracing::error!("Failed to send order confirmation email: {}", e);
                    }
                });
            }
        }
        Some(other) => info!(
            "Order {} is in status '{}'; not re-confirming after payment {}",
            payment.order_id, other, event.payment_id
        ),
        None => warn!(
            "Payment {} references missing order {}",
            event.payment_id, payment.order_id
        ),
    }

    Ok(())
}

/// Handle a failed payment webhook: mark the payment failed and put the
/// order's payment status back to failed while it stays pending.
async fn handle_payment_failed(state: &AppState, event: &WebhookEvent) -> Result<(), Error> {
    let payment = find_payment_by_gateway_id(&state.db, &event.payment_id).await?;
    let Some(payment) = payment else {
        warn!(
            "payment_intent.failed for unknown payment {}; nothing to update",
            event.payment_id
        );
        return Ok(());
    };

    if payment.status == "paid" {
        warn!(
            "Received failed webhook for already-paid payment {}; ignoring",
            event.payment_id
        );
        return Ok(());
    }

    sqlx::query("UPDATE payments SET status = 'failed', updated_at = NOW() WHERE id = $1")
        .bind(payment.id)
        .execute(state.db.pool())
        .await?;

    // Only touch orders still awaiting payment; a failed webhook must never
    // roll back an order that was already confirmed.
    let update_result = sqlx::query(
        "UPDATE orders SET payment_status = 'failed', updated_at = NOW() WHERE id = $1 AND status = 'pending'",
    )
    .bind(payment.order_id)
    .execute(state.db.pool())
    .await?;

    // Notify the customer only when the order was actually moved to failed
    if update_result.rows_affected() > 0 {
        if let Some(order) =
            sqlx::query_as::<_, rcommerce_core::models::Order>("SELECT * FROM orders WHERE id = $1")
                .bind(payment.order_id)
                .fetch_optional(state.db.pool())
                .await?
        {
            if !order.customer_email.is_empty() {
                let customer_name = order
                    .customer_email
                    .split('@')
                    .next()
                    .unwrap_or("Customer")
                    .to_string();
                let amount = format!("{} {:.2}", payment.currency, payment.amount);
                match rcommerce_core::notification::EmailNotificationFactory::payment_failed(
                    &order.customer_email,
                    &customer_name,
                    &order.order_number,
                    &amount,
                    1,
                    3,
                    None,
                    &state.branding,
                ) {
                    Ok(notification) => {
                        super::email_helpers::spawn_send(
                            state,
                            notification,
                            "payment-failed email",
                        );
                    }
                    Err(e) => warn!(
                        "Failed to build payment-failed email for order {}: {}",
                        order.order_number, e
                    ),
                }
            }
        }
    }

    Ok(())
}

/// Handle a cancelled payment webhook: mark the payment cancelled and cancel
/// the order through the order service so inventory reservations are released.
async fn handle_payment_cancelled(state: &AppState, event: &WebhookEvent) -> Result<(), Error> {
    let payment = find_payment_by_gateway_id(&state.db, &event.payment_id).await?;
    let Some(payment) = payment else {
        warn!(
            "payment_intent.cancelled for unknown payment {}; nothing to update",
            event.payment_id
        );
        return Ok(());
    };

    if payment.status == "paid" {
        warn!(
            "Received cancelled webhook for already-paid payment {}; ignoring",
            event.payment_id
        );
        return Ok(());
    }

    sqlx::query("UPDATE payments SET status = 'cancelled', updated_at = NOW() WHERE id = $1")
        .bind(payment.id)
        .execute(state.db.pool())
        .await?;

    let order_status: Option<String> =
        sqlx::query_scalar("SELECT status::text FROM orders WHERE id = $1")
            .bind(payment.order_id)
            .fetch_optional(state.db.pool())
            .await?;

    if order_status.as_deref() == Some("pending") {
        state
            .order_service
            .cancel_order(
                payment.order_id,
                "Payment cancelled by payment gateway".to_string(),
            )
            .await?;
    }

    Ok(())
}

/// Handle a refund webhook: mark the payment refunded and the matching refund
/// record completed.
async fn handle_payment_refunded(state: &AppState, event: &WebhookEvent) -> Result<(), Error> {
    // For refund events `payment_id` is the refund object id and
    // `transaction_id` is the payment intent id our payments row was keyed by.
    let payment = match event.transaction_id.as_deref() {
        Some(tid) => find_payment_by_gateway_id(&state.db, tid).await?,
        None => None,
    }
    .or(find_payment_by_gateway_id(&state.db, &event.payment_id).await?);

    let Some(payment) = payment else {
        warn!(
            "refund webhook for unknown payment {} (refund {}); nothing to update",
            event.transaction_id.as_deref().unwrap_or(""),
            event.payment_id
        );
        return Ok(());
    };

    sqlx::query("UPDATE payments SET status = 'refunded', updated_at = NOW() WHERE id = $1")
        .bind(payment.id)
        .execute(state.db.pool())
        .await?;

    // Finalize the matching refund record when present. An AWX refund webhook
    // means the refund has settled: mark it refunded and stamp settled_at.
    let updated = sqlx::query(
        "UPDATE refunds SET status = 'refunded', settled_at = COALESCE(settled_at, NOW()), \
         updated_at = NOW() WHERE gateway_refund_id = $1",
    )
    .bind(&event.payment_id)
    .execute(state.db.pool())
    .await?;

    // Refund created outside rcommerce (e.g. Airwallex dashboard): no row yet,
    // so create one now, pulling amount/currency from the payload when present
    // and falling back to the original payment.
    if updated.rows_affected() == 0 {
        let (amount, currency) = refund_amount_currency_from_payload(&event.data)
            .unwrap_or((payment.amount, payment.currency.clone()));
        if let Err(e) = record_refund_row(
            &state.db,
            &payment,
            amount,
            &currency,
            None,
            "refunded",
            &event.payment_id,
            Some(chrono::Utc::now()),
        )
        .await
        {
            tracing::warn!(
                "refund webhook for {}: recording the refunds row failed: {}",
                event.payment_id,
                e
            );
        }
    }

    sqlx::query(
        "UPDATE orders SET payment_status = 'refunded', updated_at = NOW() WHERE id = $1 AND payment_status = 'paid'",
    )
    .bind(payment.order_id)
    .execute(state.db.pool())
    .await?;

    Ok(())
}

/// Send the order-confirmation email, mirroring the checkout confirm flow
async fn send_order_confirmation_email(
    notification_service: &rcommerce_core::notification::NotificationService,
    order_service: &rcommerce_core::services::OrderService,
    customer_service: &rcommerce_core::services::CustomerService,
    db: &rcommerce_core::repository::Database,
    branding: &rcommerce_core::BrandingConfig,
    order_id: Uuid,
    customer_email: &str,
) -> rcommerce_core::Result<()> {
    use rcommerce_core::notification::{
        Address as EmailAddress, EmailNotificationFactory, OrderConfirmationParams, OrderItem,
    };

    let order_detail = match order_service.get_order(order_id).await? {
        Some(d) => d,
        None => return Ok(()),
    };

    let customer = match order_detail.order.customer_id {
        Some(id) => {
            let detail = customer_service.get_customer(id).await?;
            detail.map(|d| d.customer)
        }
        None => None,
    };

    // Load the full order row (JSON address snapshot, payment + shipping
    // method); also used for the invoice PDF attachment below.
    let invoice_order = fetch_invoice_order(db, order_id).await.ok();

    // Greeting name: first name from the customer record, then the order's
    // shipping address JSON (covers guest checkout), then a neutral fallback.
    let json_first_name = invoice_order
        .as_ref()
        .and_then(|(order, _)| {
            order
                .shipping_address
                .as_ref()
                .and_then(|v| v.get("first_name"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .filter(|s| !s.is_empty());
    let customer_name = customer
        .as_ref()
        .filter(|c| !c.first_name.is_empty())
        .map(|c| c.first_name.clone())
        .or(json_first_name)
        .unwrap_or_else(|| "there".to_string());

    // Convert order items to email template format
    let email_items: Vec<OrderItem> = order_detail
        .items
        .iter()
        .map(|item| OrderItem {
            name: item.name.clone(),
            variant_title: item.variant_name.clone(),
            sku: item.sku.clone().unwrap_or_default(),
            quantity: item.quantity,
            price: format!(
                "{:.2}",
                item.price * rust_decimal::Decimal::from(item.quantity)
            ),
            is_bundle_component: item.is_bundle_component == Some(true),
        })
        .collect();

    // Shipping/billing address: prefer the order's JSON snapshot (works for
    // guest orders where customer_id is NULL), fall back to the customer's
    // address book, then to a name-only placeholder.
    let json_shipping = invoice_order.as_ref().and_then(|(order, _)| {
        order
            .shipping_address
            .as_ref()
            .and_then(email_address_from_json)
            .or_else(|| {
                order
                    .billing_address
                    .as_ref()
                    .and_then(email_address_from_json)
            })
    });
    let json_billing = invoice_order.as_ref().and_then(|(order, _)| {
        order
            .billing_address
            .as_ref()
            .and_then(email_address_from_json)
            .or_else(|| {
                order
                    .shipping_address
                    .as_ref()
                    .and_then(email_address_from_json)
            })
    });

    let empty_addr = |name: &str| EmailAddress {
        name: name.to_string(),
        street: String::new(),
        city: String::new(),
        state: String::new(),
        zip: String::new(),
        country: String::new(),
    };

    let full_name = customer
        .as_ref()
        .map(|c| format!("{} {}", c.first_name, c.last_name))
        .filter(|n| !n.trim().is_empty());

    let shipping_addr = match json_shipping {
        Some(addr) => addr,
        None => {
            let from_book = if let (Some(addr_id), Some(cust_id)) = (
                order_detail.order.shipping_address_id,
                order_detail.order.customer_id,
            ) {
                sqlx::query_as::<_, rcommerce_core::common::Address>(
                    "SELECT * FROM addresses WHERE id = $1 AND customer_id = $2",
                )
                .bind(addr_id)
                .bind(cust_id)
                .fetch_optional(db.pool())
                .await?
                .map(|a| EmailAddress {
                    name: format!("{} {}", a.first_name, a.last_name),
                    street: a.address1,
                    city: a.city,
                    state: a.state.unwrap_or_default(),
                    zip: a.zip,
                    country: a.country,
                })
            } else {
                None
            };
            from_book.unwrap_or_else(|| empty_addr(full_name.as_deref().unwrap_or("there")))
        }
    };
    let billing_addr = json_billing.unwrap_or_else(|| shipping_addr.clone());

    let shipping_method = invoice_order
        .as_ref()
        .and_then(|(order, _)| order.shipping_method.clone())
        .map(|m| rcommerce_core::invoice::display_shipping_method(&m))
        .unwrap_or_default();

    let params = OrderConfirmationParams {
        recipient_email: customer_email,
        customer_name: &customer_name,
        order_number: &order_detail.order.order_number,
        order_date: &order_detail
            .order
            .created_at
            .format("%B %d, %Y")
            .to_string(),
        order_total: &format!(
            "{} {:.2}",
            order_detail.order.currency, order_detail.order.total
        ),
        subtotal: &format!("${:.2}", order_detail.order.subtotal),
        discount_total: &format!("${:.2}", order_detail.order.discount_total),
        discount_code: order_detail.order.coupon_code.as_deref(),
        shipping_total: &format!("${:.2}", order_detail.order.shipping_total),
        shipping_method: &shipping_method,
        tax_total: &format!("${:.2}", order_detail.order.tax_total),
        items: &email_items,
        shipping_address: &shipping_addr,
        billing_address: &billing_addr,
        branding,
    };

    let mut notification = EmailNotificationFactory::order_confirmation(params)?;

    // Attach the invoice PDF; never block the confirmation email on PDF failure.
    if let Some((order, items)) = invoice_order {
        let invoice_data = super::invoice::invoice_data_from_order(&order, &items, branding);
        match rcommerce_core::invoice::generate_invoice_pdf(&invoice_data) {
            Ok(pdf) => {
                notification
                    .attachments
                    .push(rcommerce_core::notification::EmailAttachment {
                        filename: format!("invoice-{}.pdf", order.order_number),
                        content_type: "application/pdf".to_string(),
                        content: pdf,
                    });
            }
            Err(e) => {
                tracing::error!(
                    "Failed to generate invoice PDF for order {}: {}",
                    order.order_number,
                    e
                );
            }
        }
    } else {
        tracing::error!(
            "Failed to load order {} for invoice PDF attachment",
            order_id
        );
    }

    notification_service.send(&notification).await?;
    Ok(())
}

/// Build an email address from an order's JSON address snapshot
/// (keys: first_name, last_name, address1|address_1, address2|address_2,
/// city, state, zip|postcode, country)
pub(crate) fn email_address_from_json(
    v: &serde_json::Value,
) -> Option<rcommerce_core::notification::Address> {
    use rcommerce_core::notification::Address as EmailAddress;

    let str_at = |keys: &[&str]| -> Option<String> {
        keys.iter()
            .find_map(|k| v.get(k))
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    };

    let first_name = str_at(&["first_name"])?;
    let last_name = str_at(&["last_name"]).unwrap_or_default();
    let address1 = str_at(&["address1", "address_1"]).unwrap_or_default();
    if address1.is_empty() {
        return None;
    }
    let street = match str_at(&["address2", "address_2"]) {
        Some(address2) => format!("{}, {}", address1, address2),
        None => address1,
    };

    Some(EmailAddress {
        name: format!("{} {}", first_name, last_name).trim().to_string(),
        street,
        city: str_at(&["city"]).unwrap_or_default(),
        state: str_at(&["state"]).unwrap_or_default(),
        zip: str_at(&["zip", "postcode"]).unwrap_or_default(),
        country: str_at(&["country"]).unwrap_or_default(),
    })
}

/// Load the full order row and items needed to build an invoice
pub(crate) async fn fetch_invoice_order(
    db: &rcommerce_core::repository::Database,
    order_id: Uuid,
) -> rcommerce_core::Result<(
    rcommerce_core::models::Order,
    Vec<rcommerce_core::order::OrderItem>,
)> {
    let order = sqlx::query_as::<_, rcommerce_core::models::Order>(
        "SELECT id, order_number, customer_id, customer_email, currency::text AS currency_text, order_type, subtotal, tax_total, shipping_total, discount_total, total, status, payment_status, fulfillment_status, billing_address_id, shipping_address_id, payment_method, shipping_method, notes, tags, draft, subscription_id, billing_cycle, coupon_code, coupon_id, ip_address, user_agent, metadata, shipping_address, billing_address, created_at, updated_at, completed_at, cancelled_at FROM orders WHERE id = $1"
    )
    .bind(order_id)
    .fetch_optional(db.pool())
    .await?
    .ok_or_else(|| rcommerce_core::Error::not_found("Order not found"))?;

    let items = sqlx::query_as::<_, rcommerce_core::order::OrderItem>(
        "SELECT * FROM order_items WHERE order_id = $1",
    )
    .bind(order_id)
    .fetch_all(db.pool())
    .await?;

    Ok((order, items))
}

/// Router for payment routes (mounted at /api/v1) - excludes webhooks
pub fn router() -> Router<AppState> {
    payment_routes()
}

/// Payment routes that require authentication
pub fn payment_routes() -> Router<AppState> {
    Router::new()
        // Get available payment methods
        .route("/payments/methods", post(get_payment_methods))
        // Initiate a payment
        .route("/payments", post(initiate_payment))
        // Get payment status
        .route("/payments/:payment_id", get(get_payment_status))
        // Complete payment action (3DS, redirect)
        .route(
            "/payments/:payment_id/complete",
            post(complete_payment_action),
        )
        // Refund a payment
        .route("/payments/:payment_id/refund", post(refund_payment))
        // Save payment method
        .route("/payment-methods", post(save_payment_method))
        // Get saved payment methods
        .route(
            "/customers/:customer_id/payment-methods",
            get(get_saved_payment_methods),
        )
        // Delete payment method
        .route("/payment-methods/:token", delete(delete_payment_method))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_action_mapping() {
        use WebhookEventType::*;

        assert_eq!(
            webhook_action(&PaymentSucceeded),
            WebhookAction::ConfirmPayment
        );
        assert_eq!(webhook_action(&PaymentFailed), WebhookAction::FailPayment);
        assert_eq!(
            webhook_action(&PaymentCancelled),
            WebhookAction::CancelPayment
        );
        assert_eq!(
            webhook_action(&PaymentRefunded),
            WebhookAction::MarkRefunded
        );

        // Everything else is logged and ignored
        for event in [
            PaymentPending,
            PaymentProcessing,
            PaymentPartiallyRefunded,
            DisputeCreated,
            DisputeResolved,
            SubscriptionCreated,
            SubscriptionCancelled,
            SubscriptionPaymentSucceeded,
            SubscriptionPaymentFailed,
        ] {
            assert_eq!(webhook_action(&event), WebhookAction::Ignore);
        }
    }

    #[test]
    fn test_refund_status_for_db_mapping() {
        use rcommerce_core::payment::agnostic::RefundStatus;
        assert_eq!(refund_status_for_db(&RefundStatus::Succeeded), "refunded");
        assert_eq!(refund_status_for_db(&RefundStatus::Failed), "failed");
        assert_eq!(refund_status_for_db(&RefundStatus::Pending), "pending");
        assert_eq!(refund_status_for_db(&RefundStatus::Processing), "pending");
    }

    #[test]
    fn test_refund_amount_currency_from_payload() {
        // Airwallex-style numeric amount with currency.
        let data = serde_json::json!({
            "id": "re_123",
            "amount": 112.61,
            "currency": "USD",
        });
        let (amount, currency) = refund_amount_currency_from_payload(&data).unwrap();
        assert_eq!(amount, rust_decimal::Decimal::new(11261, 2));
        assert_eq!(currency, "USD");

        // String amount also accepted.
        let data = serde_json::json!({"amount": "13.45", "currency": "hkd"});
        let (amount, currency) = refund_amount_currency_from_payload(&data).unwrap();
        assert_eq!(amount, rust_decimal::Decimal::new(1345, 2));
        assert_eq!(currency, "hkd");

        // Missing fields → None (caller falls back to the original payment).
        assert_eq!(
            refund_amount_currency_from_payload(&serde_json::json!({})),
            None
        );
        assert_eq!(
            refund_amount_currency_from_payload(&serde_json::json!({"amount": 5})),
            None
        );
        assert_eq!(
            refund_amount_currency_from_payload(
                &serde_json::json!({"amount": "not-a-number", "currency": "USD"})
            ),
            None
        );
    }
}
