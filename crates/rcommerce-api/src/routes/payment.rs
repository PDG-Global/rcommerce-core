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

    info!("Retrieved payment status for {}: {:?}", payment_id, status);

    // Build response (in production, you'd fetch full payment details from database)
    Ok(Json(PaymentStatusResponse {
        payment_id: payment_id.clone(),
        status,
        amount: "0.00".to_string(),       // Would come from database
        currency: "USD".to_string(),      // Would come from database
        gateway_id: "stripe".to_string(), // Would come from database
        transaction_id: None,             // Would come from database
        created_at: chrono::Utc::now().to_rfc3339(), // Would come from database
        updated_at: None,
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

    info!(
        "Refund processed for payment {}: refund_id={}, status={:?}",
        payment_id, response.refund_id, response.status
    );

    Ok(Json(response))
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
    match event.event_type {
        WebhookEventType::PaymentSucceeded => {
            info!("Payment succeeded: {}", event.payment_id);
            // TODO: Update order status, send confirmation email, etc.
        }
        WebhookEventType::PaymentFailed => {
            warn!("Payment failed: {}", event.payment_id);
            // TODO: Update order status, notify customer, etc.
        }
        WebhookEventType::PaymentRefunded => {
            info!("Payment refunded: {}", event.payment_id);
            // TODO: Update order status, send refund confirmation, etc.
        }
        _ => {
            info!("Unhandled webhook event: {:?}", event.event_type);
        }
    }

    Ok(Json(WebhookResponse {
        success: true,
        message: "Webhook processed successfully".to_string(),
        event_type: Some(format!("{:?}", event.event_type)),
    }))
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
