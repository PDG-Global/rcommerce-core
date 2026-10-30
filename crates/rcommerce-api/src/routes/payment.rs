//! Payment API Routes - Agnostic Payment System
//!
//! Provides unified endpoints for all payment operations regardless of gateway.
//! Frontend never communicates directly with payment providers.

use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Extension, Json, Router,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{middleware::JwtAuth, state::AppState};
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

#[derive(Debug, sqlx::FromRow)]
struct OrderAuthority {
    id: Uuid,
    customer_id: Option<Uuid>,
    customer_email: String,
    total: Decimal,
    currency: String,
    status: String,
    payment_status: String,
}

impl OrderAuthority {
    fn require_awaiting_payment(&self) -> Result<(), Error> {
        if self.status != "pending" || !matches!(self.payment_status.as_str(), "pending" | "failed")
        {
            return Err(Error::validation(
                "Order is not awaiting a retryable payment",
            ));
        }
        Ok(())
    }
}

async fn authoritative_order(
    db: &rcommerce_core::repository::Database,
    order_id: Uuid,
    auth: &JwtAuth,
) -> Result<OrderAuthority, Error> {
    let order = sqlx::query_as::<_, OrderAuthority>(
        "SELECT id, customer_id, customer_email, total, currency::text AS currency, \
         status::text AS status, payment_status::text AS payment_status \
         FROM orders WHERE id = $1",
    )
    .bind(order_id)
    .fetch_optional(db.pool())
    .await?
    .ok_or_else(|| Error::not_found("Order not found"))?;

    if !auth.is_admin() && order.customer_id != Some(auth.customer_id) {
        return Err(Error::not_found("Order not found"));
    }
    Ok(order)
}

async fn acquire_payment_attempt(
    db: &rcommerce_core::repository::Database,
    order_id: Uuid,
    gateway_id: &str,
) -> Result<Uuid, Error> {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO payment_initiation_attempts (id, order_id, gateway) \
         SELECT $1, $2, $3 FROM orders \
         WHERE id = $2 AND status = 'pending' AND payment_status IN ('pending', 'failed') \
         AND NOT EXISTS ( \
             SELECT 1 FROM payments WHERE order_id = $2 AND status IN ('pending', 'authorized', 'paid', 'refunded') \
         ) \
         ON CONFLICT (order_id) DO NOTHING RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(order_id)
    .bind(gateway_id)
    .fetch_optional(db.pool())
    .await?
    .ok_or_else(|| {
        Error::validation("Order is not awaiting payment or already has a payment in progress")
    })
}

async fn release_payment_attempt(db: &rcommerce_core::repository::Database, attempt_id: Uuid) {
    if let Err(error) = sqlx::query("DELETE FROM payment_initiation_attempts WHERE id = $1")
        .bind(attempt_id)
        .execute(db.pool())
        .await
    {
        warn!(
            "Failed to release payment attempt {}: {}",
            attempt_id, error
        );
    }
}

/// Initiate a payment
pub async fn initiate_payment(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Json(request): Json<InitiatePaymentApiRequest>,
) -> Result<Json<InitiatePaymentResponse>, Error> {
    let order_id = Uuid::parse_str(&request.order_id)
        .map_err(|_| Error::validation("Invalid order_id format"))?;
    let order = authoritative_order(&state.db, order_id, &auth).await?;
    order.require_awaiting_payment()?;
    let amount = request
        .amount
        .parse::<Decimal>()
        .map_err(|_| Error::validation("Invalid amount format"))?;
    if amount != order.total {
        return Err(Error::validation(
            "Payment amount does not match the order total",
        ));
    }
    if !request.currency.eq_ignore_ascii_case(&order.currency) {
        return Err(Error::validation(
            "Payment currency does not match the order currency",
        ));
    }
    if request.save_payment_method {
        return Err(Error::validation(
            "Save payment methods through the payment-methods endpoint",
        ));
    }

    let gateway = state
        .payment_service
        .get_gateway(Some(&request.gateway_id))
        .ok_or_else(|| Error::validation(format!("Gateway '{}' not found", request.gateway_id)))?;
    let mut gateway_customer_id = None;
    if let PaymentMethodData::CardToken { token, .. } = &request.payment_method_data {
        let customer_id = order
            .customer_id
            .ok_or_else(|| Error::not_found("Saved payment method not found"))?;
        require_saved_method_owner(&state.db, customer_id, &request.gateway_id, token).await?;
        let contact = gateway_customer_contact(&state.db, customer_id).await?;
        gateway_customer_id = Some(
            existing_gateway_customer_id(&state.db, customer_id, &request.gateway_id, &contact)
                .await?
                .ok_or_else(|| Error::payment_error("Gateway customer mapping is missing"))?,
        );
    }

    let attempt_id = acquire_payment_attempt(&state.db, order_id, &request.gateway_id).await?;
    let response = match gateway
        .initiate_payment(InitiatePaymentRequest {
            amount: order.total,
            currency: order.currency.clone(),
            payment_method_type: request.payment_method_type,
            order_id,
            customer_id: order.customer_id,
            customer_email: order.customer_email.clone(),
            customer_ip: None,
            billing_address: None,
            shipping_address: None,
            payment_method_data: request.payment_method_data,
            save_payment_method: false,
            description: request.description,
            metadata: serde_json::json!({
                "order_id": order_id.to_string(),
                "idempotency_key": request.idempotency_key,
                "gateway_customer_id": gateway_customer_id,
            }),
        })
        .await
    {
        Ok(response) => response,
        Err(error) => {
            release_payment_attempt(&state.db, attempt_id).await;
            return Err(error);
        }
    };

    let persisted = persist_payment_outcome(
        &state,
        &order,
        &request.gateway_id,
        DirectPaymentOutcome::from_initiate(&response),
    )
    .await;
    if persisted.is_ok() {
        release_payment_attempt(&state.db, attempt_id).await;
    } else {
        warn!(
            "Retaining payment attempt {} after the gateway returned an outcome that could not be persisted",
            attempt_id
        );
    }
    persisted?;
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
    Extension(auth): Extension<JwtAuth>,
    Path(payment_id): Path<String>,
    Json(request): Json<CompletePaymentActionApiRequest>,
) -> Result<Json<CompletePaymentActionResponse>, Error> {
    let (payment, order) = authorized_payment(&state.db, &payment_id, &auth).await?;
    let gateway_payment_id = payment
        .gateway_payment_id
        .clone()
        .ok_or_else(|| Error::not_found("Payment not found"))?;
    let gateway = state
        .payment_service
        .get_gateway(Some(&payment.gateway))
        .ok_or_else(|| Error::payment_error("Stored payment gateway is not configured"))?;
    let response = gateway
        .complete_payment_action(CompletePaymentActionRequest {
            payment_id: gateway_payment_id.clone(),
            action_type: request.action_type,
            action_data: request.action_data,
        })
        .await?;
    let outcome = DirectPaymentOutcome::from_complete(&response);
    if outcome.payment_id != gateway_payment_id {
        return Err(Error::payment_error(
            "Gateway returned a different payment identifier",
        ));
    }

    persist_payment_outcome(&state, &order, &payment.gateway, outcome).await?;
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

async fn find_payment_by_gateway_id(
    db: &rcommerce_core::repository::Database,
    gateway_id: &str,
    gateway_payment_id: &str,
) -> Result<Option<PaymentRow>, Error> {
    let rows = sqlx::query_as::<_, PaymentRow>(
        r#"
        SELECT id, order_id, amount, currency::text AS currency, gateway,
               gateway_payment_id, status::text AS status, created_at, updated_at
        FROM payments
        WHERE gateway = $1 AND gateway_payment_id = $2
        ORDER BY created_at DESC
        LIMIT 2
        "#,
    )
    .bind(gateway_id)
    .bind(gateway_payment_id)
    .fetch_all(db.pool())
    .await?;
    if rows.len() > 1 {
        return Err(Error::payment_error(
            "Multiple local payments match the gateway payment identifier",
        ));
    }

    Ok(rows.into_iter().next())
}

async fn find_payment_for_api(
    db: &rcommerce_core::repository::Database,
    payment_reference: &str,
) -> Result<Option<PaymentRow>, Error> {
    if let Ok(id) = Uuid::parse_str(payment_reference) {
        return sqlx::query_as::<_, PaymentRow>(
            r#"
            SELECT id, order_id, amount, currency::text AS currency, gateway,
                   gateway_payment_id, status::text AS status, created_at, updated_at
            FROM payments WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(db.pool())
        .await
        .map_err(Into::into);
    }
    if let Some((gateway_id, gateway_payment_id)) = payment_reference.split_once(':') {
        if !gateway_id.is_empty() && !gateway_payment_id.is_empty() {
            return find_payment_by_gateway_id(db, gateway_id, gateway_payment_id).await;
        }
    }

    let rows = sqlx::query_as::<_, PaymentRow>(
        r#"
        SELECT id, order_id, amount, currency::text AS currency, gateway,
               gateway_payment_id, status::text AS status, created_at, updated_at
        FROM payments
        WHERE gateway_payment_id = $1
        ORDER BY created_at DESC
        LIMIT 2
        "#,
    )
    .bind(payment_reference)
    .fetch_all(db.pool())
    .await?;
    if rows.len() > 1 {
        return Err(Error::payment_error(
            "Payment identifier is ambiguous; use <gateway>:<payment_id> or the local payment UUID",
        ));
    }
    Ok(rows.into_iter().next())
}

async fn authorized_payment(
    db: &rcommerce_core::repository::Database,
    payment_id: &str,
    auth: &JwtAuth,
) -> Result<(PaymentRow, OrderAuthority), Error> {
    let payment = find_payment_for_api(db, payment_id)
        .await?
        .ok_or_else(|| Error::not_found("Payment not found"))?;
    let order = authoritative_order(db, payment.order_id, auth).await?;
    Ok((payment, order))
}

struct DirectPaymentOutcome {
    payment_id: String,
    status: PaymentStatus,
    payment_method: Option<PaymentMethodInfo>,
    error_message: Option<String>,
}

impl DirectPaymentOutcome {
    fn from_initiate(response: &InitiatePaymentResponse) -> Self {
        match response {
            InitiatePaymentResponse::Success {
                payment_id,
                payment_status,
                payment_method,
                ..
            } => Self {
                payment_id: payment_id.clone(),
                status: payment_status.clone(),
                payment_method: Some(payment_method.clone()),
                error_message: None,
            },
            InitiatePaymentResponse::RequiresAction { payment_id, .. } => Self {
                payment_id: payment_id.clone(),
                status: PaymentStatus::RequiresAction,
                payment_method: None,
                error_message: None,
            },
            InitiatePaymentResponse::Failed {
                payment_id,
                error_code,
                error_message,
                ..
            } => Self {
                payment_id: payment_id.clone(),
                status: if error_code == "payment_cancelled" {
                    PaymentStatus::Cancelled
                } else {
                    PaymentStatus::Failed
                },
                payment_method: None,
                error_message: Some(error_message.clone()),
            },
        }
    }

    fn from_complete(response: &CompletePaymentActionResponse) -> Self {
        match response {
            CompletePaymentActionResponse::Success {
                payment_id,
                payment_status,
                payment_method,
                ..
            } => Self {
                payment_id: payment_id.clone(),
                status: payment_status.clone(),
                payment_method: Some(payment_method.clone()),
                error_message: None,
            },
            CompletePaymentActionResponse::RequiresAction { payment_id, .. } => Self {
                payment_id: payment_id.clone(),
                status: PaymentStatus::RequiresAction,
                payment_method: None,
                error_message: None,
            },
            CompletePaymentActionResponse::Failed {
                payment_id,
                error_code,
                error_message,
                ..
            } => Self {
                payment_id: payment_id.clone(),
                status: if error_code == "payment_cancelled" {
                    PaymentStatus::Cancelled
                } else {
                    PaymentStatus::Failed
                },
                payment_method: None,
                error_message: Some(error_message.clone()),
            },
        }
    }

    fn from_status(payment_id: &str, status: PaymentStatus) -> Self {
        Self {
            payment_id: payment_id.to_string(),
            status,
            payment_method: None,
            error_message: None,
        }
    }
}

fn payment_status_for_db(status: &PaymentStatus) -> &'static str {
    match status {
        PaymentStatus::Pending | PaymentStatus::Processing | PaymentStatus::RequiresAction => {
            "pending"
        }
        PaymentStatus::Succeeded => "paid",
        PaymentStatus::Failed => "failed",
        PaymentStatus::Cancelled => "cancelled",
        PaymentStatus::Refunded => "refunded",
        PaymentStatus::PartiallyRefunded => "paid",
        PaymentStatus::Disputed => "pending",
    }
}

fn payment_method_for_db(method: Option<&PaymentMethodInfo>) -> Option<&'static str> {
    match method.map(|m| &m.method_type) {
        Some(PaymentMethodType::Card) => Some("credit_card"),
        Some(PaymentMethodType::BankTransfer) => Some("bank_transfer"),
        Some(PaymentMethodType::CashOnDelivery) => Some("cash_on_delivery"),
        Some(PaymentMethodType::GooglePay | PaymentMethodType::ApplePay) => Some("digital_wallet"),
        Some(PaymentMethodType::Crypto) => Some("cryptocurrency"),
        _ => None,
    }
}

const PAYMENT_ROW_RETURNING: &str = "id, order_id, amount, currency::text AS currency, gateway, gateway_payment_id, status::text AS status, created_at, updated_at";

async fn cancel_pending_order_for_payment(state: &AppState, order_id: Uuid) -> Result<(), Error> {
    let cancellable = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM orders WHERE id = $1 AND status = 'pending' \
         AND payment_status NOT IN ('paid', 'refunded')",
    )
    .bind(order_id)
    .fetch_optional(state.db.pool())
    .await?;
    if cancellable.is_some() {
        state
            .order_service
            .cancel_order(order_id, "Payment cancelled by payment gateway".to_string())
            .await?;
        sqlx::query(
            "UPDATE orders SET payment_status = 'cancelled', updated_at = NOW() \
             WHERE id = $1 AND status = 'cancelled' AND payment_status NOT IN ('paid', 'refunded')",
        )
        .bind(order_id)
        .execute(state.db.pool())
        .await?;
    }
    Ok(())
}

async fn persist_payment_outcome(
    state: &AppState,
    order: &OrderAuthority,
    gateway_id: &str,
    outcome: DirectPaymentOutcome,
) -> Result<PaymentRow, Error> {
    let status = payment_status_for_db(&outcome.status);
    let method = outcome.payment_method.as_ref();
    let method_type = payment_method_for_db(method);
    let last_four = method.and_then(|m| m.last_four.as_deref());
    let card_brand = method.and_then(|m| m.card_brand.as_deref());
    let existing = find_payment_by_gateway_id(&state.db, gateway_id, &outcome.payment_id).await?;
    let payment = match existing {
        Some(existing) => {
            if existing.order_id != order.id {
                return Err(Error::payment_error(
                    "Gateway payment identifier is already bound to another order",
                ));
            }
            let update_sql = format!(
                "UPDATE payments SET amount = $1, currency = UPPER($2)::currency, \
                 status = CASE \
                       WHEN payments.status = 'refunded' THEN payments.status \
                       WHEN payments.status = 'paid' AND $3 NOT IN ('paid', 'refunded') \
                       THEN payments.status ELSE $3::payment_status END, \
                 payment_method = COALESCE($4::payment_method_type, payment_method), \
                 card_last_four = COALESCE($5, card_last_four), card_brand = COALESCE($6, card_brand), \
                 error_message = $7, \
                 processed_at = CASE WHEN $3 = 'paid' THEN COALESCE(processed_at, NOW()) ELSE processed_at END, \
                 updated_at = NOW() \
                 WHERE id = $8 AND order_id = $9 RETURNING {PAYMENT_ROW_RETURNING}"
            );
            sqlx::query_as::<_, PaymentRow>(&update_sql)
                .bind(order.total)
                .bind(&order.currency)
                .bind(status)
                .bind(method_type)
                .bind(last_four)
                .bind(card_brand)
                .bind(&outcome.error_message)
                .bind(existing.id)
                .bind(order.id)
                .fetch_one(state.db.pool())
                .await?
        }
        None => {
            let insert_sql = format!(
                "INSERT INTO payments (order_id, amount, currency, status, gateway, gateway_payment_id, \
                 payment_method, card_last_four, card_brand, error_message, processed_at, test) \
                 VALUES ($1, $2, UPPER($3)::currency, $4::payment_status, $5, $6, \
                 $7::payment_method_type, $8, $9, $10, \
                 CASE WHEN $4 = 'paid' THEN NOW() ELSE NULL END, \
                 (SELECT test FROM orders WHERE id = $1)) RETURNING {PAYMENT_ROW_RETURNING}"
            );
            sqlx::query_as::<_, PaymentRow>(&insert_sql)
                .bind(order.id)
                .bind(order.total)
                .bind(&order.currency)
                .bind(status)
                .bind(gateway_id)
                .bind(&outcome.payment_id)
                .bind(method_type)
                .bind(last_four)
                .bind(card_brand)
                .bind(&outcome.error_message)
                .fetch_one(state.db.pool())
                .await?
        }
    };

    if payment.status == "paid" {
        state
            .order_service
            .confirm_order_after_payment(order.id)
            .await?;
    } else if payment.status == "cancelled" {
        cancel_pending_order_for_payment(state, order.id).await?;
    } else {
        let order_payment_status = match payment.status.as_str() {
            "failed" => "failed",
            "refunded" => "refunded",
            _ => "pending",
        };
        sqlx::query(
            "UPDATE orders SET payment_status = $1::payment_status, updated_at = NOW() \
             WHERE id = $2 AND status = 'pending' AND payment_status <> 'paid'",
        )
        .bind(order_payment_status)
        .bind(order.id)
        .execute(state.db.pool())
        .await?;
    }

    Ok(payment)
}

/// Get payment status
pub async fn get_payment_status(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(payment_id): Path<String>,
) -> Result<Json<PaymentStatusResponse>, Error> {
    let (payment, order) = authorized_payment(&state.db, &payment_id, &auth).await?;
    let gateway_payment_id = payment
        .gateway_payment_id
        .clone()
        .ok_or_else(|| Error::not_found("Payment not found"))?;
    let gateway = state
        .payment_service
        .get_gateway(Some(&payment.gateway))
        .ok_or_else(|| Error::payment_error("Stored payment gateway is not configured"))?;
    let status = gateway.get_payment_status(&gateway_payment_id).await?;
    let payment = persist_payment_outcome(
        &state,
        &order,
        &payment.gateway,
        DirectPaymentOutcome::from_status(&gateway_payment_id, status.clone()),
    )
    .await?;

    Ok(Json(PaymentStatusResponse {
        payment_id: gateway_payment_id,
        status,
        amount: payment.amount.to_string(),
        currency: payment.currency,
        gateway_id: payment.gateway,
        transaction_id: payment.gateway_payment_id,
        created_at: payment.created_at.to_rfc3339(),
        updated_at: payment.updated_at.map(|t| t.to_rfc3339()),
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
    Extension(auth): Extension<JwtAuth>,
    Path(payment_id): Path<String>,
    Json(request): Json<RefundRequest>,
) -> Result<Json<RefundResponse>, Error> {
    if !auth.is_admin() {
        return Err(Error::forbidden("Admin access required for refunds"));
    }
    let payment = find_payment_for_api(&state.db, &payment_id)
        .await?
        .ok_or_else(|| Error::not_found("Payment not found"))?;
    let gateway_payment_id = payment
        .gateway_payment_id
        .clone()
        .ok_or_else(|| Error::not_found("Payment not found"))?;
    let amount = request
        .amount
        .as_deref()
        .map(|amount| {
            amount
                .parse::<Decimal>()
                .map_err(|_| Error::validation("Invalid amount format"))
        })
        .transpose()?;
    if amount.is_some_and(|amount| amount <= Decimal::ZERO) {
        return Err(Error::validation("Refund amount must be positive"));
    }
    let gateway = state
        .payment_service
        .get_gateway(Some(&payment.gateway))
        .ok_or_else(|| Error::payment_error("Stored payment gateway is not configured"))?;
    let response = gateway
        .refund_payment(&gateway_payment_id, amount, &request.reason)
        .await?;
    if response.payment_id != gateway_payment_id {
        return Err(Error::payment_error(
            "Gateway returned a refund for a different payment",
        ));
    }
    if !response.currency.eq_ignore_ascii_case(&payment.currency) {
        return Err(Error::payment_error(
            "Gateway returned a refund in a different currency",
        ));
    }
    if response.amount < Decimal::ZERO {
        return Err(Error::payment_error(
            "Gateway returned a negative refund amount",
        ));
    }

    let settled_at = (response.status == RefundStatus::Succeeded).then_some(response.created_at);
    reconcile_refund(
        &state,
        &payment,
        response.amount,
        &response.currency,
        Some(&response.reason),
        refund_status_for_db(&response.status),
        &response.refund_id,
        settled_at,
    )
    .await?;

    info!(
        "Refund processed for payment {}: refund_id={}, status={:?}",
        gateway_payment_id, response.refund_id, response.status
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

async fn record_refund_row(
    connection: &mut sqlx::PgConnection,
    payment: &PaymentRow,
    amount: Decimal,
    currency: &str,
    reason: Option<&str>,
    status: &str,
    gateway_refund_id: &str,
    settled_at: Option<chrono::DateTime<chrono::Utc>>,
) -> rcommerce_core::Result<()> {
    let updated = sqlx::query(
        "UPDATE refunds SET amount = $4, currency = UPPER($5)::currency, \
         reason = COALESCE($6, reason), \
         status = CASE WHEN refunds.status = 'refunded' THEN refunds.status ELSE $7::payment_status END, \
         settled_at = CASE WHEN $7 = 'refunded' THEN COALESCE(refunds.settled_at, $9, NOW()) \
                           ELSE refunds.settled_at END, \
         updated_at = NOW() \
         WHERE payment_id = $2 AND gateway_refund_id = $8",
    )
    .bind(Uuid::new_v4())
    .bind(payment.id)
    .bind(payment.order_id)
    .bind(amount)
    .bind(currency)
    .bind(reason)
    .bind(status)
    .bind(gateway_refund_id)
    .bind(settled_at)
    .execute(&mut *connection)
    .await?;
    if updated.rows_affected() > 0 {
        return Ok(());
    }

    sqlx::query(
        r#"
        INSERT INTO refunds (id, payment_id, order_id, amount, currency, reason, status,
                             gateway_refund_id, settled_at, created_at, updated_at)
        SELECT $1, $2, $3, $4, UPPER($5)::currency, $6, $7::payment_status, $8,
               CASE WHEN $7 = 'refunded' THEN COALESCE($9, NOW()) ELSE NULL END, NOW(), NOW()
        WHERE NOT EXISTS (
            SELECT 1 FROM refunds WHERE payment_id = $2 AND gateway_refund_id = $8
        )
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(payment.id)
    .bind(payment.order_id)
    .bind(amount)
    .bind(currency)
    .bind(reason)
    .bind(status)
    .bind(gateway_refund_id)
    .bind(settled_at)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn reconcile_refund(
    state: &AppState,
    payment: &PaymentRow,
    amount: Decimal,
    currency: &str,
    reason: Option<&str>,
    status: &str,
    gateway_refund_id: &str,
    settled_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<(), Error> {
    if amount < Decimal::ZERO {
        return Err(Error::payment_error("Refund amount cannot be negative"));
    }
    let mut transaction = state.db.pool().begin().await?;
    let payment_sql =
        format!("SELECT {PAYMENT_ROW_RETURNING} FROM payments WHERE id = $1 FOR UPDATE");
    let locked = sqlx::query_as::<_, PaymentRow>(&payment_sql)
        .bind(payment.id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| Error::not_found("Payment not found"))?;
    if locked.order_id != payment.order_id || locked.gateway != payment.gateway {
        return Err(Error::payment_error(
            "Payment binding changed during refund",
        ));
    }
    if !matches!(locked.status.as_str(), "paid" | "refunded") {
        return Err(Error::validation("Payment is not in a refundable state"));
    }
    if !currency.eq_ignore_ascii_case(&locked.currency) {
        return Err(Error::payment_error(
            "Refund currency does not match the payment currency",
        ));
    }

    record_refund_row(
        &mut transaction,
        &locked,
        amount,
        currency,
        reason,
        status,
        gateway_refund_id,
        settled_at,
    )
    .await?;
    let settled: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount), 0) FROM refunds \
         WHERE payment_id = $1 AND status = 'refunded'",
    )
    .bind(locked.id)
    .fetch_one(&mut *transaction)
    .await?;
    let fully_refunded = settled > Decimal::ZERO && settled >= locked.amount;
    sqlx::query(
        "UPDATE payments SET status = CASE WHEN $2 THEN 'refunded'::payment_status \
                                           ELSE 'paid'::payment_status END, \
         updated_at = NOW() WHERE id = $1",
    )
    .bind(locked.id)
    .bind(fully_refunded)
    .execute(&mut *transaction)
    .await?;
    if !fully_refunded && settled > Decimal::ZERO {
        sqlx::query(
            "UPDATE orders SET payment_status = 'paid', updated_at = NOW() \
             WHERE id = $1 AND status NOT IN ('refunded', 'cancelled') \
             AND payment_status = 'refunded'",
        )
        .bind(locked.order_id)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;

    if fully_refunded {
        state
            .order_service
            .reconcile_order_refund(locked.order_id)
            .await?;
    }
    Ok(())
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

fn saved_method_token_hash(_gateway_id: &str, token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[derive(Debug, sqlx::FromRow)]
struct SavedMethodOwnership {
    id: Uuid,
    customer_id: Uuid,
    gateway: String,
}

async fn require_saved_method_owner(
    db: &rcommerce_core::repository::Database,
    customer_id: Uuid,
    gateway_id: &str,
    token: &str,
) -> Result<(), Error> {
    let owned = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM saved_payment_methods \
         WHERE customer_id = $1 AND gateway = $2 AND token_hash = $3)",
    )
    .bind(customer_id)
    .bind(gateway_id)
    .bind(saved_method_token_hash(gateway_id, token))
    .fetch_one(db.pool())
    .await?;
    if !owned {
        return Err(Error::not_found("Saved payment method not found"));
    }
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
struct GatewayCustomerContact {
    email: String,
    first_name: String,
    last_name: String,
    airwallex_customer_id: Option<String>,
}

async fn gateway_customer_contact(
    db: &rcommerce_core::repository::Database,
    customer_id: Uuid,
) -> Result<GatewayCustomerContact, Error> {
    sqlx::query_as::<_, GatewayCustomerContact>(
        "SELECT email, COALESCE(first_name, '') AS first_name, \
         COALESCE(last_name, '') AS last_name, airwallex_customer_id FROM customers WHERE id = $1",
    )
    .bind(customer_id)
    .fetch_optional(db.pool())
    .await?
    .ok_or_else(|| Error::not_found("Customer not found"))
}

fn validate_provider_customer_id(
    customer_id: Uuid,
    provider_customer_id: &str,
) -> Result<(), Error> {
    if provider_customer_id.is_empty()
        || Uuid::parse_str(provider_customer_id).is_ok_and(|id| id == customer_id)
    {
        return Err(Error::payment_error(
            "Gateway returned an invalid customer identifier",
        ));
    }
    Ok(())
}

async fn store_gateway_customer_mapping(
    db: &rcommerce_core::repository::Database,
    customer_id: Uuid,
    gateway_id: &str,
    provider_customer_id: &str,
    legacy_airwallex_id: Option<&str>,
) -> Result<String, Error> {
    validate_provider_customer_id(customer_id, provider_customer_id)?;
    if gateway_id == "airwallex"
        && legacy_airwallex_id.is_some_and(|legacy| legacy != provider_customer_id)
    {
        return Err(Error::payment_error(
            "Conflicting Airwallex customer mappings",
        ));
    }

    sqlx::query(
        "INSERT INTO payment_gateway_customers (customer_id, gateway, gateway_customer_id) \
         VALUES ($1, $2, $3) ON CONFLICT (customer_id, gateway) DO NOTHING",
    )
    .bind(customer_id)
    .bind(gateway_id)
    .bind(provider_customer_id)
    .execute(db.pool())
    .await?;
    let stored = sqlx::query_scalar::<_, String>(
        "SELECT gateway_customer_id FROM payment_gateway_customers \
         WHERE customer_id = $1 AND gateway = $2",
    )
    .bind(customer_id)
    .bind(gateway_id)
    .fetch_optional(db.pool())
    .await?
    .ok_or_else(|| {
        Error::payment_error("Gateway customer is already mapped to another customer")
    })?;
    if stored != provider_customer_id {
        return Err(Error::payment_error(
            "Conflicting gateway customer mappings",
        ));
    }

    if gateway_id == "airwallex" {
        sqlx::query(
            "UPDATE customers SET airwallex_customer_id = $1, updated_at = NOW() \
             WHERE id = $2 AND airwallex_customer_id IS NULL",
        )
        .bind(provider_customer_id)
        .bind(customer_id)
        .execute(db.pool())
        .await?;
    }
    Ok(stored)
}

async fn existing_gateway_customer_id(
    db: &rcommerce_core::repository::Database,
    customer_id: Uuid,
    gateway_id: &str,
    contact: &GatewayCustomerContact,
) -> Result<Option<String>, Error> {
    let mapped = sqlx::query_scalar::<_, String>(
        "SELECT gateway_customer_id FROM payment_gateway_customers \
         WHERE customer_id = $1 AND gateway = $2",
    )
    .bind(customer_id)
    .bind(gateway_id)
    .fetch_optional(db.pool())
    .await?;
    if let Some(mapped) = mapped.as_deref() {
        validate_provider_customer_id(customer_id, mapped)?;
    }
    let legacy = (gateway_id == "airwallex")
        .then_some(contact.airwallex_customer_id.as_deref())
        .flatten()
        .filter(|id| !id.is_empty());
    if let Some(legacy) = legacy {
        validate_provider_customer_id(customer_id, legacy)?;
    }
    match (mapped, legacy) {
        (Some(mapped), Some(legacy)) if mapped != legacy => Err(Error::payment_error(
            "Conflicting Airwallex customer mappings",
        )),
        (Some(mapped), _) => Ok(Some(mapped)),
        (None, Some(legacy)) => {
            store_gateway_customer_mapping(db, customer_id, gateway_id, legacy, Some(legacy))
                .await
                .map(Some)
        }
        (None, None) => Ok(None),
    }
}

fn saved_method_type(method_type: &PaymentMethodType) -> &'static str {
    match method_type {
        PaymentMethodType::Card => "card",
        PaymentMethodType::GooglePay => "google_pay",
        PaymentMethodType::ApplePay => "apple_pay",
        PaymentMethodType::Alipay => "alipay",
        PaymentMethodType::WechatPay => "wechat_pay",
        PaymentMethodType::PayPal => "paypal",
        PaymentMethodType::BankTransfer => "bank_transfer",
        PaymentMethodType::BuyNowPayLater => "buy_now_pay_later",
        PaymentMethodType::Crypto => "crypto",
        PaymentMethodType::CashOnDelivery => "cash_on_delivery",
    }
}

pub async fn save_payment_method(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Json(request): Json<SavePaymentMethodRequest>,
) -> Result<Json<PaymentMethodToken>, Error> {
    let contact = gateway_customer_contact(&state.db, auth.customer_id).await?;
    let provider_customer_id = match existing_gateway_customer_id(
        &state.db,
        auth.customer_id,
        &request.gateway_id,
        &contact,
    )
    .await?
    {
        Some(id) => id,
        None => {
            let gateway = state
                .payment_service
                .get_gateway(Some(&request.gateway_id))
                .ok_or_else(|| {
                    Error::validation(format!("Gateway '{}' not found", request.gateway_id))
                })?;
            let name = format!("{} {}", contact.first_name, contact.last_name);
            let provider_customer_id = gateway.create_customer(&contact.email, name.trim()).await?;
            store_gateway_customer_mapping(
                &state.db,
                auth.customer_id,
                &request.gateway_id,
                &provider_customer_id,
                contact.airwallex_customer_id.as_deref(),
            )
            .await?
        }
    };
    let gateway = state
        .payment_service
        .get_gateway(Some(&request.gateway_id))
        .ok_or_else(|| Error::validation(format!("Gateway '{}' not found", request.gateway_id)))?;
    let token = gateway
        .tokenize_payment_method_for_customer(&provider_customer_id, request.payment_method_data)
        .await?;
    let method = &token.payment_method;
    let token_hash = saved_method_token_hash(&request.gateway_id, &token.token);
    let insert_result = sqlx::query(
        "INSERT INTO saved_payment_methods (customer_id, gateway, token_hash, method_type, \
         card_last_four, card_brand, exp_month, exp_year, cardholder_name, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(auth.customer_id)
    .bind(&request.gateway_id)
    .bind(&token_hash)
    .bind(saved_method_type(&method.method_type))
    .bind(&method.last_four)
    .bind(&method.card_brand)
    .bind(&method.exp_month)
    .bind(&method.exp_year)
    .bind(&method.cardholder_name)
    .bind(token.expires_at)
    .execute(state.db.pool())
    .await;
    if let Err(error) = insert_result {
        let already_registered = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM saved_payment_methods WHERE token_hash = $1)",
        )
        .bind(&token_hash)
        .fetch_one(state.db.pool())
        .await
        .unwrap_or(true);
        if !already_registered {
            if let Err(cleanup_error) = gateway.delete_payment_method(&token.token).await {
                warn!(
                    "Failed to disable an unattached provider payment method after a database error: {}",
                    cleanup_error
                );
            }
        } else {
            return Err(Error::forbidden("Payment method is already registered"));
        }
        return Err(error.into());
    }

    info!(
        "Saved a payment method for customer {} using {}",
        auth.customer_id, request.gateway_id
    );
    Ok(Json(token))
}

/// Get saved payment methods for a customer
pub async fn get_saved_payment_methods(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(customer_id): Path<Uuid>,
) -> Result<Json<Vec<PaymentMethodInfo>>, Error> {
    if !auth.is_admin() && auth.customer_id != customer_id {
        return Err(Error::not_found("Customer not found"));
    }
    let contact = gateway_customer_contact(&state.db, customer_id).await?;
    let gateway_ids = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT gateway FROM saved_payment_methods WHERE customer_id = $1 ORDER BY gateway",
    )
    .bind(customer_id)
    .fetch_all(state.db.pool())
    .await?;
    let mut owned_methods = Vec::new();
    for gateway_id in gateway_ids {
        let token_hashes = sqlx::query_scalar::<_, String>(
            "SELECT token_hash FROM saved_payment_methods WHERE customer_id = $1 AND gateway = $2",
        )
        .bind(customer_id)
        .bind(&gateway_id)
        .fetch_all(state.db.pool())
        .await?
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
        let Some(provider_customer_id) =
            existing_gateway_customer_id(&state.db, customer_id, &gateway_id, &contact).await?
        else {
            continue;
        };
        let gateway = state
            .payment_service
            .get_gateway(Some(&gateway_id))
            .ok_or_else(|| Error::payment_error("Stored payment gateway is not configured"))?;
        let methods = gateway
            .get_saved_payment_methods(&provider_customer_id)
            .await?;
        owned_methods.extend(methods.into_iter().filter(|method| {
            method.token.as_deref().is_some_and(|token| {
                token_hashes.contains(&saved_method_token_hash(&gateway_id, token))
            })
        }));
    }
    Ok(Json(owned_methods))
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
    Extension(auth): Extension<JwtAuth>,
    Path(token): Path<String>,
) -> Result<Json<DeletePaymentMethodResponse>, Error> {
    let method = sqlx::query_as::<_, SavedMethodOwnership>(
        "SELECT id, customer_id, gateway FROM saved_payment_methods WHERE token_hash = $1",
    )
    .bind(saved_method_token_hash("", &token))
    .fetch_optional(state.db.pool())
    .await?
    .ok_or_else(|| Error::not_found("Saved payment method not found"))?;
    if !auth.is_admin() && method.customer_id != auth.customer_id {
        return Err(Error::not_found("Saved payment method not found"));
    }

    let contact = gateway_customer_contact(&state.db, method.customer_id).await?;
    existing_gateway_customer_id(&state.db, method.customer_id, &method.gateway, &contact)
        .await?
        .ok_or_else(|| Error::payment_error("Gateway customer mapping is missing"))?;
    let gateway = state
        .payment_service
        .get_gateway(Some(&method.gateway))
        .ok_or_else(|| Error::payment_error("Stored payment gateway is not configured"))?;
    gateway.delete_payment_method(&token).await?;
    sqlx::query("DELETE FROM saved_payment_methods WHERE id = $1")
        .bind(method.id)
        .execute(state.db.pool())
        .await?;

    info!(
        "Deleted a saved payment method for customer {} using {}",
        method.customer_id, method.gateway
    );
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
            handle_payment_succeeded(&state, &gateway_id, &event).await?;
        }
        WebhookAction::FailPayment => {
            warn!("Payment failed: {}", event.payment_id);
            handle_payment_failed(&state, &gateway_id, &event).await?;
        }
        WebhookAction::CancelPayment => {
            warn!("Payment cancelled: {}", event.payment_id);
            handle_payment_cancelled(&state, &gateway_id, &event).await?;
        }
        WebhookAction::MarkRefunded => {
            info!("Payment refunded: {}", event.payment_id);
            handle_payment_refunded(&state, &gateway_id, &event).await?;
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
        WebhookEventType::PaymentRefunded | WebhookEventType::PaymentPartiallyRefunded => {
            WebhookAction::MarkRefunded
        }
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
async fn handle_payment_succeeded(
    state: &AppState,
    gateway_id: &str,
    event: &WebhookEvent,
) -> Result<(), Error> {
    let payment = find_payment_by_gateway_id(&state.db, gateway_id, &event.payment_id).await?;
    let Some(payment) = payment else {
        warn!(
            "payment_intent.succeeded for unknown payment {}; nothing to update",
            event.payment_id
        );
        return Ok(());
    };

    if payment.status == "refunded" {
        warn!(
            "Received succeeded webhook for refunded payment {}; ignoring",
            event.payment_id
        );
        return Ok(());
    }
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
        let updated = sqlx::query(
            "UPDATE payments SET status = 'paid', processed_at = NOW(), updated_at = NOW() \
             WHERE id = $1 AND status <> 'refunded'",
        )
        .bind(payment.id)
        .execute(state.db.pool())
        .await?;
        if updated.rows_affected() == 0 {
            warn!(
                "Succeeded webhook arrived after payment {} was refunded; ignoring",
                event.payment_id
            );
            return Ok(());
        }
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
            let (detail, transitioned) = state
                .order_service
                .confirm_order_after_payment_transition(payment.order_id)
                .await?;
            if !transitioned {
                return Ok(());
            }

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
async fn handle_payment_failed(
    state: &AppState,
    gateway_id: &str,
    event: &WebhookEvent,
) -> Result<(), Error> {
    let payment = find_payment_by_gateway_id(&state.db, gateway_id, &event.payment_id).await?;
    let Some(payment) = payment else {
        warn!(
            "payment_intent.failed for unknown payment {}; nothing to update",
            event.payment_id
        );
        return Ok(());
    };

    if matches!(payment.status.as_str(), "paid" | "refunded") {
        warn!(
            "Received failed webhook for terminal payment {}; ignoring",
            event.payment_id
        );
        return Ok(());
    }

    let updated = sqlx::query(
        "UPDATE payments SET status = 'failed', updated_at = NOW() \
         WHERE id = $1 AND status NOT IN ('paid', 'refunded')",
    )
    .bind(payment.id)
    .execute(state.db.pool())
    .await?;
    if updated.rows_affected() == 0 {
        warn!(
            "Failed webhook arrived after payment {} reached a terminal state; ignoring",
            event.payment_id
        );
        return Ok(());
    }

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
async fn handle_payment_cancelled(
    state: &AppState,
    gateway_id: &str,
    event: &WebhookEvent,
) -> Result<(), Error> {
    let payment = find_payment_by_gateway_id(&state.db, gateway_id, &event.payment_id).await?;
    let Some(payment) = payment else {
        warn!(
            "payment_intent.cancelled for unknown payment {}; nothing to update",
            event.payment_id
        );
        return Ok(());
    };

    if matches!(payment.status.as_str(), "paid" | "refunded") {
        warn!(
            "Received cancelled webhook for terminal payment {}; ignoring",
            event.payment_id
        );
        return Ok(());
    }

    let updated = sqlx::query(
        "UPDATE payments SET status = 'cancelled', updated_at = NOW() \
         WHERE id = $1 AND status NOT IN ('paid', 'refunded')",
    )
    .bind(payment.id)
    .execute(state.db.pool())
    .await?;
    if updated.rows_affected() == 0 {
        warn!(
            "Cancelled webhook arrived after payment {} reached a terminal state; ignoring",
            event.payment_id
        );
        return Ok(());
    }

    cancel_pending_order_for_payment(state, payment.order_id).await?;

    Ok(())
}

async fn handle_payment_refunded(
    state: &AppState,
    gateway_id: &str,
    event: &WebhookEvent,
) -> Result<(), Error> {
    let payment = match event.transaction_id.as_deref() {
        Some(transaction_id) => {
            find_payment_by_gateway_id(&state.db, gateway_id, transaction_id).await?
        }
        None => None,
    };
    let payment = match payment {
        Some(payment) => Some(payment),
        None => find_payment_by_gateway_id(&state.db, gateway_id, &event.payment_id).await?,
    };

    let Some(payment) = payment else {
        warn!(
            "refund webhook for unknown payment {} (refund {}); nothing to update",
            event.transaction_id.as_deref().unwrap_or(""),
            event.payment_id
        );
        return Ok(());
    };

    let refund_id = event
        .data
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&event.payment_id);
    let existing = sqlx::query_as::<_, (Decimal, String)>(
        "SELECT amount, currency::text FROM refunds \
         WHERE payment_id = $1 AND gateway_refund_id = $2",
    )
    .bind(payment.id)
    .bind(refund_id)
    .fetch_optional(state.db.pool())
    .await?;
    let (amount, currency) = refund_amount_currency_from_payload(&event.data)
        .or(existing)
        .unwrap_or((payment.amount, payment.currency.clone()));
    reconcile_refund(
        state,
        &payment,
        amount,
        &currency,
        None,
        "refunded",
        refund_id,
        Some(event.timestamp),
    )
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
        assert_eq!(
            webhook_action(&PaymentPartiallyRefunded),
            WebhookAction::MarkRefunded
        );

        // Everything else is logged and ignored
        for event in [
            PaymentPending,
            PaymentProcessing,
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

    #[test]
    fn payment_cancelled_error_maps_to_cancelled_outcomes() {
        let initiated = InitiatePaymentResponse::Failed {
            payment_id: "intent_cancelled".to_string(),
            error_code: "payment_cancelled".to_string(),
            error_message: "Payment cancelled".to_string(),
            retry_allowed: false,
        };
        assert_eq!(
            DirectPaymentOutcome::from_initiate(&initiated).status,
            PaymentStatus::Cancelled
        );
        let completed = CompletePaymentActionResponse::Failed {
            payment_id: "intent_cancelled".to_string(),
            error_code: "payment_cancelled".to_string(),
            error_message: "Payment cancelled".to_string(),
            retry_allowed: false,
        };
        assert_eq!(
            DirectPaymentOutcome::from_complete(&completed).status,
            PaymentStatus::Cancelled
        );
        let failed = InitiatePaymentResponse::Failed {
            payment_id: "intent_failed".to_string(),
            error_code: "card_declined".to_string(),
            error_message: "Card declined".to_string(),
            retry_allowed: true,
        };
        assert_eq!(
            DirectPaymentOutcome::from_initiate(&failed).status,
            PaymentStatus::Failed
        );
    }

    #[test]
    fn direct_payment_statuses_map_to_paid_only_on_success() {
        for status in [
            PaymentStatus::Pending,
            PaymentStatus::Processing,
            PaymentStatus::RequiresAction,
        ] {
            assert_eq!(payment_status_for_db(&status), "pending");
        }
        assert_eq!(payment_status_for_db(&PaymentStatus::Succeeded), "paid");
        assert_eq!(payment_status_for_db(&PaymentStatus::Failed), "failed");
        assert_eq!(
            payment_status_for_db(&PaymentStatus::PartiallyRefunded),
            "paid"
        );
        assert_eq!(payment_status_for_db(&PaymentStatus::Refunded), "refunded");
    }

    #[test]
    fn saved_method_hashes_and_provider_ids_are_safe() {
        let owner = Uuid::new_v4();
        let first = saved_method_token_hash("airwallex", "provider_token");
        let second = saved_method_token_hash("stripe", "provider_token");
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert!(!first.contains("provider_token"));
        assert!(validate_provider_customer_id(owner, &owner.to_string()).is_err());
        assert!(validate_provider_customer_id(owner, "").is_err());
        assert!(validate_provider_customer_id(owner, "cus_provider").is_ok());
    }

    #[tokio::test]
    #[ignore = "Requires PostgreSQL TEST_DATABASE_URL"]
    async fn airwallex_customer_mapping_backfills_without_breaking_legacy_consents() {
        let url = std::env::var("TEST_DATABASE_URL").unwrap();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .unwrap();
        rcommerce_core::Migrator::new(pool.clone())
            .migrate()
            .await
            .unwrap();
        let db = rcommerce_core::repository::Database::new(pool.clone());
        let legacy_customer = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO customers (id, email, airwallex_customer_id) VALUES ($1, $2, 'cus_legacy')",
        )
        .bind(legacy_customer)
        .bind(format!("legacy-{legacy_customer}@example.com"))
        .execute(&pool)
        .await
        .unwrap();
        let contact = gateway_customer_contact(&db, legacy_customer)
            .await
            .unwrap();
        let resolved = existing_gateway_customer_id(&db, legacy_customer, "airwallex", &contact)
            .await
            .unwrap();
        assert_eq!(resolved.as_deref(), Some("cus_legacy"));
        let legacy: String =
            sqlx::query_scalar("SELECT airwallex_customer_id FROM customers WHERE id = $1")
                .bind(legacy_customer)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(legacy, "cus_legacy");

        let new_customer = Uuid::new_v4();
        sqlx::query("INSERT INTO customers (id, email) VALUES ($1, $2)")
            .bind(new_customer)
            .bind(format!("new-{new_customer}@example.com"))
            .execute(&pool)
            .await
            .unwrap();
        store_gateway_customer_mapping(&db, new_customer, "airwallex", "cus_new", None)
            .await
            .unwrap();
        let legacy: String =
            sqlx::query_scalar("SELECT airwallex_customer_id FROM customers WHERE id = $1")
                .bind(new_customer)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(legacy, "cus_new");
    }
}
