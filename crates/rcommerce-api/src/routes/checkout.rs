//! Checkout routes for order creation
//!
//! Provides endpoints for the complete checkout flow:
//! - POST /checkout/initiate - Start checkout, calculate tax and shipping rates
//! - POST /checkout/shipping - Select shipping method
//! - POST /checkout/complete - Complete checkout, create order and process payment

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Extension, Json, Router,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::middleware::JwtAuth;
use crate::state::AppState;

// Import core checkout types
use rcommerce_core::models::Address;
use rcommerce_core::notification::{
    service::NotificationService, Address as EmailAddress, Notification, NotificationChannel,
    NotificationPriority, OrderConfirmationParams, OrderItem,
};
use rcommerce_core::payment::{CardDetails, PaymentMethod};
use rcommerce_core::services::{
    CheckoutResult, CheckoutSummary, CompleteCheckoutRequest, InitiateCheckoutRequest,
    SelectShippingRequest,
};
use rcommerce_core::{CustomerService, OrderService};

/// Request to initiate checkout
#[derive(Debug, Deserialize)]
pub struct InitiateCheckoutApiRequest {
    pub cart_id: Uuid,
    pub shipping_address: Address,
    pub billing_address: Option<Address>,
    pub vat_id: Option<String>,
    pub currency: Option<String>,
}

/// Request to select shipping
#[derive(Debug, Deserialize)]
pub struct SelectShippingApiRequest {
    pub cart_id: Uuid,
    pub shipping_address: Address,
    pub shipping_rate: ShippingRateResponse,
}

/// Request to complete checkout
#[derive(Debug, Deserialize)]
pub struct CompleteCheckoutApiRequest {
    pub cart_id: Uuid,
    pub shipping_address: Address,
    pub billing_address: Option<Address>,
    pub payment_method: PaymentMethodRequest,
    pub customer_email: String,
    pub vat_id: Option<String>,
    pub notes: Option<String>,
    pub selected_shipping_rate: ShippingRateResponse,
    /// Mark this order as a test order (accounting spec 2.2). Exported with
    /// test=true so accounting can book it to a suspense account.
    #[serde(default)]
    pub test: Option<bool>,
}

/// Request to confirm checkout after client-side payment confirmation
#[derive(Debug, Deserialize)]
pub struct ConfirmCheckoutApiRequest {
    pub order_id: Uuid,
    pub payment_intent_id: String,
    pub cart_id: Uuid,
}

/// Payment method request
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PaymentMethodRequest {
    Card {
        number: String,
        exp_month: u32,
        exp_year: u32,
        cvc: String,
        name: String,
    },
    StripeElements,
    GooglePay,
    ApplePay,
    WeChatPay,
    AliPay,
    BankTransfer,
    CashOnDelivery,
}

impl From<PaymentMethodRequest> for PaymentMethod {
    fn from(req: PaymentMethodRequest) -> Self {
        match req {
            PaymentMethodRequest::Card {
                number,
                exp_month,
                exp_year,
                cvc,
                name,
            } => PaymentMethod::Card(CardDetails {
                number,
                exp_month,
                exp_year,
                cvc,
                name,
            }),
            PaymentMethodRequest::StripeElements => PaymentMethod::StripeElements,
            PaymentMethodRequest::GooglePay => PaymentMethod::GooglePay,
            PaymentMethodRequest::ApplePay => PaymentMethod::ApplePay,
            PaymentMethodRequest::WeChatPay => PaymentMethod::WeChatPay,
            PaymentMethodRequest::AliPay => PaymentMethod::AliPay,
            PaymentMethodRequest::BankTransfer => PaymentMethod::BankTransfer,
            PaymentMethodRequest::CashOnDelivery => PaymentMethod::CashOnDelivery,
        }
    }
}

/// Shipping rate response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShippingRateResponse {
    pub provider_id: String,
    pub carrier: String,
    pub service_code: String,
    pub service_name: String,
    pub rate: Decimal,
    pub currency: String,
    pub delivery_days: Option<i32>,
    pub total_cost: Decimal,
}

impl From<rcommerce_core::shipping::ShippingRate> for ShippingRateResponse {
    fn from(rate: rcommerce_core::shipping::ShippingRate) -> Self {
        Self {
            provider_id: rate.provider_id,
            carrier: rate.carrier,
            service_code: rate.service_code,
            service_name: rate.service_name,
            rate: rate.rate,
            currency: rate.currency,
            delivery_days: rate.delivery_days,
            total_cost: rate.total_cost,
        }
    }
}

impl From<ShippingRateResponse> for rcommerce_core::shipping::ShippingRate {
    fn from(rate: ShippingRateResponse) -> Self {
        Self {
            provider_id: rate.provider_id,
            carrier: rate.carrier,
            service_code: rate.service_code,
            service_name: rate.service_name,
            rate: rate.rate,
            currency: rate.currency,
            delivery_days: rate.delivery_days,
            delivery_date: None,
            estimated: true,
            insurance_fee: None,
            fuel_surcharge: None,
            handling_fee: None,
            other_fees: std::collections::HashMap::new(),
            total_cost: rate.total_cost,
        }
    }
}

/// Tax breakdown item
#[derive(Debug, Clone, Serialize)]
pub struct TaxBreakdownResponse {
    pub tax_zone_name: String,
    pub tax_rate_name: String,
    pub rate: Decimal,
    pub taxable_amount: Decimal,
    pub tax_amount: Decimal,
}

/// Cart item in checkout response
#[derive(Debug, Clone, Serialize)]
pub struct CheckoutItemResponse {
    pub id: Uuid,
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub title: String,
    pub sku: Option<String>,
    pub quantity: i32,
    pub unit_price: Decimal,
    pub total: Decimal,
}

/// Checkout summary response
#[derive(Debug, Clone, Serialize)]
pub struct CheckoutSummaryResponse {
    pub cart_id: Uuid,
    pub items: Vec<CheckoutItemResponse>,
    pub subtotal: Decimal,
    pub discount_total: Decimal,
    pub shipping_total: Decimal,
    pub shipping_tax: Decimal,
    pub item_tax: Decimal,
    pub tax_total: Decimal,
    pub total: Decimal,
    pub currency: String,
    pub available_shipping_rates: Vec<ShippingRateResponse>,
    pub selected_shipping_rate: Option<ShippingRateResponse>,
    pub tax_breakdown: Vec<TaxBreakdownResponse>,
    pub vat_id_valid: Option<bool>,
    pub publishable_key: Option<String>,
    pub airwallex_client_id: Option<String>,
    pub airwallex_env: Option<String>,
}

impl From<CheckoutSummary> for CheckoutSummaryResponse {
    fn from(summary: CheckoutSummary) -> Self {
        Self {
            cart_id: summary.cart_id,
            items: summary
                .items
                .into_iter()
                .map(|item| CheckoutItemResponse {
                    id: item.id,
                    product_id: item.product_id,
                    variant_id: item.variant_id,
                    title: item.title,
                    sku: item.sku,
                    quantity: item.quantity,
                    unit_price: item.unit_price,
                    total: item.total,
                })
                .collect(),
            subtotal: summary.subtotal,
            discount_total: summary.discount_total,
            shipping_total: summary.shipping_total,
            shipping_tax: summary.shipping_tax,
            item_tax: summary.item_tax,
            tax_total: summary.tax_total,
            total: summary.total,
            currency: format!("{:?}", summary.currency),
            available_shipping_rates: summary
                .available_shipping_rates
                .into_iter()
                .map(Into::into)
                .collect(),
            selected_shipping_rate: summary.selected_shipping_rate.map(Into::into),
            tax_breakdown: summary
                .tax_breakdown
                .into_iter()
                .map(|tb| TaxBreakdownResponse {
                    tax_zone_name: tb.tax_zone_name,
                    tax_rate_name: tb.tax_rate_name,
                    rate: tb.rate,
                    taxable_amount: tb.taxable_amount,
                    tax_amount: tb.tax_amount,
                })
                .collect(),
            vat_id_valid: summary.vat_id_valid,
            publishable_key: summary.publishable_key,
            airwallex_client_id: summary.airwallex_client_id,
            airwallex_env: summary.airwallex_env,
        }
    }
}

/// Order item response
#[derive(Debug, Clone, Serialize)]
pub struct OrderItemResponse {
    pub id: Uuid,
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub title: String,
    pub sku: Option<String>,
    pub quantity: i32,
    pub price: Decimal,
    pub total: Decimal,
    pub tax_amount: Decimal,
}

/// Order response
#[derive(Debug, Clone, Serialize)]
pub struct OrderResponse {
    pub id: Uuid,
    pub order_number: String,
    pub customer_id: Option<Uuid>,
    pub customer_email: String,
    pub status: String,
    pub payment_status: String,
    pub fulfillment_status: String,
    pub currency: String,
    pub subtotal: Decimal,
    pub tax_total: Decimal,
    pub shipping_total: Decimal,
    pub discount_total: Decimal,
    pub total: Decimal,
    pub items: Vec<OrderItemResponse>,
    pub created_at: String,
    pub metadata: serde_json::Value,
}

/// Checkout result response
#[derive(Debug, Clone, Serialize)]
pub struct CheckoutResultResponse {
    pub order: OrderResponse,
    pub payment_id: String,
    pub total_charged: Decimal,
    pub currency: String,
    pub client_secret: Option<String>,
    pub payment_intent_id: Option<String>,
    pub requires_confirmation: bool,
}

impl From<CheckoutResult> for CheckoutResultResponse {
    fn from(result: CheckoutResult) -> Self {
        let order_detail = result.order;
        Self {
            order: OrderResponse {
                id: order_detail.order.id,
                order_number: order_detail.order.order_number,
                customer_id: order_detail.order.customer_id,
                customer_email: order_detail.order.customer_email.clone(),
                status: format!("{:?}", order_detail.order.status).to_lowercase(),
                payment_status: format!("{:?}", order_detail.order.payment_status).to_lowercase(),
                fulfillment_status: format!("{:?}", order_detail.order.fulfillment_status)
                    .to_lowercase(),
                currency: order_detail.order.currency.to_string(),
                subtotal: order_detail.order.subtotal,
                tax_total: order_detail.order.tax_total,
                shipping_total: order_detail.order.shipping_total,
                discount_total: order_detail.order.discount_total,
                total: order_detail.order.total,
                items: order_detail
                    .items
                    .into_iter()
                    .map(|item| OrderItemResponse {
                        id: item.id,
                        product_id: item.product_id,
                        variant_id: item.variant_id,
                        title: item.name,
                        sku: item.sku,
                        quantity: item.quantity,
                        price: item.price,
                        total: item.total,
                        tax_amount: item.tax_amount,
                    })
                    .collect(),
                created_at: order_detail.order.created_at.to_rfc3339(),
                metadata: serde_json::json!({}),
            },
            payment_id: result.payment_id,
            total_charged: result.total_charged,
            currency: format!("{:?}", result.currency),
            client_secret: result.client_secret,
            payment_intent_id: result.payment_intent_id,
            requires_confirmation: result.requires_confirmation,
        }
    }
}

/// Initiate checkout endpoint
///
/// Calculates totals, tax, and available shipping rates for the customer's cart
pub async fn initiate_checkout(
    State(state): State<AppState>,
    Extension(auth): Extension<Option<JwtAuth>>,
    Json(request): Json<InitiateCheckoutApiRequest>,
) -> Result<Json<CheckoutSummaryResponse>, (StatusCode, Json<serde_json::Value>)> {
    super::require_address_phone(&request.shipping_address.phone).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;
    if let Some(ref billing) = request.billing_address {
        super::require_address_phone(&billing.phone).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        })?;
    }

    // Build the core request
    let currency = request
        .currency
        .and_then(|c| c.parse().ok())
        .unwrap_or_default();

    let core_request = InitiateCheckoutRequest {
        cart_id: request.cart_id,
        shipping_address: request.shipping_address,
        billing_address: request.billing_address,
        vat_id: request.vat_id,
        customer_id: auth.map(|a| a.customer_id),
        currency: Some(currency),
    };

    // Call checkout service
    match state.checkout_service.initiate_checkout(core_request).await {
        Ok(summary) => {
            let response: CheckoutSummaryResponse = summary.into();
            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("Failed to initiate checkout: {}", e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            ))
        }
    }
}

/// Select shipping endpoint
///
/// Updates the checkout with the selected shipping method and recalculates totals
pub async fn select_shipping(
    State(state): State<AppState>,
    Extension(_auth): Extension<Option<JwtAuth>>,
    Json(request): Json<SelectShippingApiRequest>,
) -> Result<Json<CheckoutSummaryResponse>, (StatusCode, Json<serde_json::Value>)> {
    super::require_address_phone(&request.shipping_address.phone).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;

    // Convert shipping rate and create package estimate
    let shipping_rate: rcommerce_core::shipping::ShippingRate = request.shipping_rate.into();

    // Estimate package dimensions (simplified - in production, calculate from cart items)
    let package = rcommerce_core::shipping::Package {
        weight: Decimal::from(1), // Default weight, should be calculated
        weight_unit: "kg".to_string(),
        length: Some(Decimal::from(30)),
        width: Some(Decimal::from(20)),
        height: Some(Decimal::from(15)),
        dimension_unit: Some("cm".to_string()),
        predefined_package: None,
    };

    let core_request = SelectShippingRequest {
        cart_id: request.cart_id,
        shipping_address: request.shipping_address,
        shipping_rate,
        package,
    };

    // Call checkout service
    match state.checkout_service.select_shipping(core_request).await {
        Ok(summary) => {
            let response: CheckoutSummaryResponse = summary.into();
            Ok(Json(response))
        }
        Err(e) => {
            tracing::error!("Failed to select shipping: {}", e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            ))
        }
    }
}

/// Complete checkout endpoint
///
/// Finalizes the checkout, creates an order, and processes payment
pub async fn complete_checkout(
    State(state): State<AppState>,
    Extension(auth): Extension<Option<JwtAuth>>,
    Json(request): Json<CompleteCheckoutApiRequest>,
) -> Result<(StatusCode, Json<CheckoutResultResponse>), (StatusCode, Json<serde_json::Value>)> {
    // Validate email
    if request.customer_email.is_empty() || !request.customer_email.contains('@') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Invalid email address"})),
        ));
    }

    // Phone is required on the shipping address (and billing, when supplied).
    super::require_address_phone(&request.shipping_address.phone).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e.to_string()})),
        )
    })?;
    if let Some(ref billing) = request.billing_address {
        super::require_address_phone(&billing.phone).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        })?;
    }

    // Authenticated customers must have a verified email to check out.
    if let Some(ref auth) = auth {
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

        if !customer.email_verified {
            return Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "Please verify your email before checking out.",
                    "code": "email_verification_required"
                })),
            ));
        }
    }

    // Build the core request
    let core_request = CompleteCheckoutRequest {
        cart_id: request.cart_id,
        shipping_address: request.shipping_address,
        billing_address: request.billing_address,
        payment_method: request.payment_method.into(),
        customer_email: request.customer_email,
        customer_id: auth.map(|a| a.customer_id),
        vat_id: request.vat_id,
        notes: request.notes,
        selected_shipping_rate: request.selected_shipping_rate.into(),
        test: request.test,
    };

    // Call checkout service
    match state.checkout_service.complete_checkout(core_request).await {
        Ok(result) => {
            let response: CheckoutResultResponse = result.into();
            // No admin notification here: orders that still require payment
            // confirmation are not real orders yet. The shop operator is
            // emailed exactly when payment confirms (webhook or /checkout/confirm).

            // Fire-and-forget order confirmation email if SMTP configured
            if !response.requires_confirmation {
                if let Some(ref svc) = state.notification_service {
                    let order_id = response.order.id;
                    let customer_email = response.order.customer_email.clone();
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

                // Fire-and-forget Xero invoice sync for paid orders
                if let Some(ref xero) = state.xero_client {
                    let xero = xero.clone();
                    let order_service = state.order_service.clone();
                    let order_id = response.order.id;
                    tokio::spawn(async move {
                        if let Err(e) = sync_order_to_xero(&xero, &order_service, order_id).await {
                            tracing::error!("Failed to sync order {} to Xero: {}", order_id, e);
                        }
                    });
                }
            }
            Ok((StatusCode::CREATED, Json(response)))
        }
        Err(e) => {
            tracing::error!("Failed to complete checkout: {}", e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            ))
        }
    }
}

/// Confirm checkout endpoint
///
/// Verifies a client-side confirmed payment (e.g., Stripe Elements) and finalizes the order
pub async fn confirm_checkout(
    State(state): State<AppState>,
    Extension(_auth): Extension<Option<JwtAuth>>,
    Json(request): Json<ConfirmCheckoutApiRequest>,
) -> Result<(StatusCode, Json<CheckoutResultResponse>), (StatusCode, Json<serde_json::Value>)> {
    let core_request = rcommerce_core::services::ConfirmCheckoutRequest {
        order_id: request.order_id,
        payment_intent_id: request.payment_intent_id,
        cart_id: request.cart_id,
    };

    // Remember whether this call is the one transitioning the order: only a
    // pending order is a real payment-confirmation transition; a racing
    // duplicate (webhook beating us, or a client retry) must not re-notify.
    let was_pending = matches!(
        sqlx::query_scalar::<_, String>("SELECT status::text FROM orders WHERE id = $1")
            .bind(request.order_id)
            .fetch_optional(state.db.pool())
            .await,
        Ok(Some(s)) if s == "pending"
    );

    match state.checkout_service.confirm_checkout(core_request).await {
        Ok(result) => {
            let response: CheckoutResultResponse = result.into();
            // Only a pending order is a real payment-confirmation transition. A
            // racing duplicate (webhook beating us, or a client retry) must not
            // re-notify: the admin email, customer confirmation and Xero sync
            // all fire exactly once per order.
            if was_pending {
                super::email_helpers::send_admin_order_email(&state, response.order.id).await;

                // Fire-and-forget order confirmation email if SMTP configured
                if let Some(ref svc) = state.notification_service {
                    let order_id = response.order.id;
                    let customer_email = response.order.customer_email.clone();
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

                // Fire-and-forget Xero invoice sync for paid orders
                if let Some(ref xero) = state.xero_client {
                    let xero = xero.clone();
                    let order_service = state.order_service.clone();
                    let order_id = response.order.id;
                    tokio::spawn(async move {
                        if let Err(e) = sync_order_to_xero(&xero, &order_service, order_id).await {
                            tracing::error!("Failed to sync order {} to Xero: {}", order_id, e);
                        }
                    });
                }
            }
            Ok((StatusCode::OK, Json(response)))
        }
        Err(e) => {
            tracing::error!("Failed to confirm checkout: {}", e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e.to_string()})),
            ))
        }
    }
}

async fn send_order_confirmation_email(
    notification_service: &NotificationService,
    order_service: &OrderService,
    customer_service: &CustomerService,
    db: &rcommerce_core::repository::Database,
    branding: &rcommerce_core::BrandingConfig,
    order_id: Uuid,
    customer_email: &str,
) -> rcommerce_core::Result<()> {
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

    // Load the full order row for the JSON address snapshot (works for guest
    // orders where customer_id is NULL) and the shipping method label.
    let invoice_order = super::payment::fetch_invoice_order(db, order_id).await.ok();

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
    let email_items: Vec<rcommerce_core::notification::OrderItem> = order_detail
        .items
        .iter()
        .map(|item| rcommerce_core::notification::OrderItem {
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

    // Prefer the order's JSON address snapshot; fall back to the customer's
    // address book for registered customers.
    let json_shipping = invoice_order.as_ref().and_then(|(order, _)| {
        order
            .shipping_address
            .as_ref()
            .and_then(super::payment::email_address_from_json)
            .or_else(|| {
                order
                    .billing_address
                    .as_ref()
                    .and_then(super::payment::email_address_from_json)
            })
    });
    let json_billing = invoice_order.as_ref().and_then(|(order, _)| {
        order
            .billing_address
            .as_ref()
            .and_then(super::payment::email_address_from_json)
            .or_else(|| {
                order
                    .shipping_address
                    .as_ref()
                    .and_then(super::payment::email_address_from_json)
            })
    });

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
                .map(|a| rcommerce_core::notification::Address {
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
            from_book.unwrap_or_else(|| rcommerce_core::notification::Address {
                name: full_name.clone().unwrap_or_else(|| "there".to_string()),
                street: String::new(),
                city: String::new(),
                state: String::new(),
                zip: String::new(),
                country: String::new(),
            })
        }
    };
    let billing_addr = json_billing.unwrap_or_else(|| shipping_addr.clone());

    let shipping_method = invoice_order
        .as_ref()
        .and_then(|(order, _)| order.shipping_method.clone())
        .map(|m| rcommerce_core::invoice::display_shipping_method(&m))
        .unwrap_or_default();

    let params = rcommerce_core::notification::OrderConfirmationParams {
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

    let notification =
        rcommerce_core::notification::EmailNotificationFactory::order_confirmation(params)?;

    notification_service.send(&notification).await?;
    Ok(())
}

/// Sync a paid order to Xero: create an ACCREC invoice and reconcile it
/// with a matching payment. Runs fire-and-forget after checkout completes.
async fn sync_order_to_xero(
    xero: &rcommerce_core::accounting::XeroClient,
    order_service: &OrderService,
    order_id: Uuid,
) -> rcommerce_core::Result<()> {
    use rcommerce_core::accounting::{XeroInvoiceLine, XeroInvoiceParams};
    use rust_decimal::Decimal;

    let order_detail = match order_service.get_order(order_id).await? {
        Some(d) => d,
        None => return Ok(()),
    };

    let order = &order_detail.order;

    // Build invoice lines from order items
    let mut lines: Vec<XeroInvoiceLine> = order_detail
        .items
        .iter()
        .map(|item| XeroInvoiceLine {
            description: format!(
                "{}{}",
                item.name,
                item.sku
                    .as_ref()
                    .map(|s| format!(" ({})", s))
                    .unwrap_or_default()
            ),
            quantity: Decimal::from(item.quantity),
            unit_amount: item.price,
            account_code: "200".to_string(),
        })
        .collect();

    // Add shipping as a separate line item
    if order.shipping_total > Decimal::ZERO {
        lines.push(XeroInvoiceLine {
            description: "Shipping".to_string(),
            quantity: Decimal::ONE,
            unit_amount: order.shipping_total,
            account_code: "200".to_string(),
        });
    }

    // Add tax as a separate line item (LineAmountTypes is Inclusive, so tax
    // is reported as part of line amounts in Xero — this line keeps the
    // invoice total aligned with the order when tax applies)
    if order.tax_total > Decimal::ZERO {
        lines.push(XeroInvoiceLine {
            description: "Tax / VAT".to_string(),
            quantity: Decimal::ONE,
            unit_amount: order.tax_total,
            account_code: "200".to_string(),
        });
    }

    // Apply discount as a negative line item
    if order.discount_total > Decimal::ZERO {
        lines.push(XeroInvoiceLine {
            description: "Discount".to_string(),
            quantity: Decimal::ONE,
            unit_amount: -order.discount_total,
            account_code: "200".to_string(),
        });
    }

    let params = XeroInvoiceParams {
        order_id: order.id,
        // The Xero invoice number must match the accounting export's order_id
        // (store-prefixed reference like "88-1024", spec 2.3).
        order_number: order
            .invoice_number
            .clone()
            .unwrap_or_else(|| order.order_number.clone()),
        customer_name: order
            .customer_email
            .split('@')
            .next()
            .unwrap_or("Customer")
            .to_string(),
        customer_email: order.customer_email.clone(),
        currency: order.currency.clone(),
        lines,
        total: order.total,
        date: order.created_at.format("%Y-%m-%d").to_string(),
        reference: format!("rCommerce order {}", order.order_number),
    };

    let invoice_id = xero.sync_paid_order(&params).await?;

    tracing::info!(
        "Order {} synced to Xero invoice {}",
        order.order_number,
        invoice_id
    );

    Ok(())
}

/// GET /api/v1/shipping/countries - public list of shippable country codes
/// (the storefront filters its country selector to these).
pub async fn shipping_countries() -> Json<serde_json::Value> {
    let provider = rcommerce_core::shipping::carriers::WeightBasedProvider::new();
    Json(serde_json::json!({ "countries": provider.shippable_countries() }))
}

/// Router for checkout routes
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/checkout/initiate", post(initiate_checkout))
        .route("/checkout/shipping", post(select_shipping))
        .route("/checkout/complete", post(complete_checkout))
        .route("/checkout/confirm", post(confirm_checkout))
        .route("/shipping/countries", get(shipping_countries))
}
