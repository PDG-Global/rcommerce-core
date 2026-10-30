use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use rust_decimal::Decimal;

use rcommerce_core::order::OrderStatus as LifecycleOrderStatus;
use rcommerce_core::tax::TaxService;
use rcommerce_core::Error;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::middleware::JwtAuth;
use crate::state::AppState;
use axum::Extension;

/// Create order request from API
#[derive(Debug, Deserialize)]
pub struct CreateOrderRequest {
    pub customer_id: Option<Uuid>,
    pub customer_email: String,
    pub billing_address_id: Option<Uuid>,
    pub shipping_address_id: Option<Uuid>,
    /// Shipping address for tax and shipping calculation
    pub shipping_address: Option<rcommerce_core::models::Address>,
    pub items: Vec<CreateOrderItem>,
    pub notes: Option<String>,
    pub coupon_code: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateOrderItem {
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub quantity: i32,
}

/// Order response
#[derive(Debug, Serialize)]
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
    pub coupon_code: Option<String>,
    pub total: Decimal,
    pub items: Vec<OrderItemResponse>,
    pub payments: Vec<OrderPaymentResponse>,
    pub billing_address: Option<serde_json::Value>,
    pub shipping_address: Option<serde_json::Value>,
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fulfillments: Vec<OrderFulfillmentResponse>,
    pub created_at: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OrderFulfillmentResponse {
    pub id: Uuid,
    pub status: String,
    pub tracking_number: Option<String>,
    pub tracking_url: Option<String>,
    pub tracking_company: Option<String>,
    pub shipped_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct OrderPaymentResponse {
    pub id: Uuid,
    pub amount: Decimal,
    pub currency: String,
    pub status: String,
    pub gateway: String,
    pub card_brand: Option<String>,
    pub card_last_four: Option<String>,
    pub processed_at: Option<String>,
    pub refunds: Vec<OrderRefundResponse>,
}

#[derive(Debug, Serialize)]
pub struct OrderRefundResponse {
    pub id: Uuid,
    pub amount: Decimal,
    pub currency: String,
    pub status: String,
    pub reason: Option<String>,
    pub gateway_refund_id: Option<String>,
    pub created_at: String,
    pub settled_at: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct RefundRow {
    id: Uuid,
    payment_id: Uuid,
    amount: Decimal,
    currency_text: String,
    status_text: String,
    reason: Option<String>,
    gateway_refund_id: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    settled_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, sqlx::FromRow)]
struct PaymentRow {
    id: Uuid,
    amount: Decimal,
    currency_text: String,
    status_text: String,
    gateway: String,
    card_brand: Option<String>,
    card_last_four: Option<String>,
    processed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
pub struct OrderItemResponse {
    pub id: Uuid,
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub name: String,
    pub sku: Option<String>,
    pub quantity: i32,
    pub price: Decimal,
    pub total: Decimal,
}

const ORDER_COLUMNS: &str = "id, order_number, customer_id, customer_email, currency::text AS currency_text, order_type, subtotal, tax_total, shipping_total, discount_total, total, status, payment_status, fulfillment_status, shipping_address_id, billing_address_id, payment_method, shipping_method, notes, tags, draft, subscription_id, billing_cycle, coupon_code, coupon_id, ip_address, user_agent, metadata, shipping_address, billing_address, created_at, updated_at, completed_at, cancelled_at";

/// List orders for the authenticated customer (or all orders for admin)
pub async fn list_orders(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
) -> Json<serde_json::Value> {
    let query = if auth.is_admin() {
        format!(
            "SELECT {} FROM orders ORDER BY created_at DESC LIMIT 50",
            ORDER_COLUMNS
        )
    } else {
        format!(
            "SELECT {} FROM orders WHERE customer_id = $1 ORDER BY created_at DESC LIMIT 50",
            ORDER_COLUMNS
        )
    };

    let result = if auth.is_admin() {
        sqlx::query_as::<_, rcommerce_core::models::Order>(&query)
            .fetch_all(state.db.pool())
            .await
    } else {
        sqlx::query_as::<_, rcommerce_core::models::Order>(&query)
            .bind(auth.customer_id)
            .fetch_all(state.db.pool())
            .await
    };

    match result {
        Ok(orders) => {
            let order_responses: Vec<OrderResponse> = orders
                .into_iter()
                .map(|o| OrderResponse {
                    id: o.id,
                    order_number: o.order_number,
                    customer_id: o.customer_id,
                    customer_email: o.customer_email,
                    status: format!("{:?}", o.status).to_lowercase(),
                    payment_status: format!("{:?}", o.payment_status).to_lowercase(),
                    fulfillment_status: format!("{:?}", o.fulfillment_status).to_lowercase(),
                    currency: o.currency.to_string(),
                    subtotal: o.subtotal,
                    tax_total: o.tax_total,
                    shipping_total: o.shipping_total,
                    discount_total: o.discount_total,
                    coupon_code: o.coupon_code.clone(),
                    total: o.total,
                    items: vec![], // Will be populated separately
                    payments: vec![],
                    billing_address: o.billing_address.clone(),
                    shipping_address: o.shipping_address.clone(),
                    notes: o.notes.clone(),
                    fulfillments: vec![],
                    created_at: o.created_at.to_rfc3339(),
                })
                .collect();

            Json(serde_json::json!({
                "orders": order_responses,
                "meta": {
                    "total": order_responses.len(),
                    "page": 1,
                    "per_page": 50,
                }
            }))
        }
        Err(e) => {
            tracing::error!("Failed to list orders: {}", e);
            Json(serde_json::json!({
                "orders": [],
                "meta": {
                    "total": 0,
                    "page": 1,
                    "per_page": 50,
                }
            }))
        }
    }
}

/// Get order by ID
pub async fn get_order(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
) -> Result<Json<OrderResponse>, (StatusCode, Json<serde_json::Value>)> {
    let order = match sqlx::query_as::<_, rcommerce_core::models::Order>(&format!(
        "SELECT {} FROM orders WHERE id = $1",
        ORDER_COLUMNS
    ))
    .bind(id)
    .fetch_optional(state.db.pool())
    .await
    {
        Ok(Some(o)) => o,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("Order {} not found", id)})),
            ));
        }
        Err(e) => {
            tracing::error!("Database error: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            ));
        }
    };

    // Customers can only access their own orders unless admin
    if order.customer_id != Some(auth.customer_id) && !auth.is_admin() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Access denied"})),
        ));
    }

    // Get order items
    let items = match sqlx::query_as::<_, rcommerce_core::models::OrderItem>(
        "SELECT * FROM order_items WHERE order_id = $1",
    )
    .bind(id)
    .fetch_all(state.db.pool())
    .await
    {
        Ok(items) => items,
        Err(e) => {
            tracing::error!("Database error: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Database error"})),
            ));
        }
    };

    let item_responses: Vec<OrderItemResponse> = items
        .into_iter()
        .map(|i| OrderItemResponse {
            id: i.id,
            product_id: i.product_id,
            variant_id: i.variant_id,
            name: i.title.clone(),
            sku: i.sku,
            quantity: i.quantity,
            price: i.price,
            total: i.total,
        })
        .collect();

    // Get payment records for the order
    let payment_rows = match sqlx::query_as::<_, PaymentRow>(
        r#"
        SELECT id, amount, currency::text AS currency_text, status::text AS status_text,
               gateway, card_brand, card_last_four, processed_at
        FROM payments
        WHERE order_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(id)
    .fetch_all(state.db.pool())
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!("Failed to load payments for order {}: {}", id, e);
            vec![]
        }
    };

    let mut payment_responses: Vec<OrderPaymentResponse> = payment_rows
        .into_iter()
        .map(|p| OrderPaymentResponse {
            id: p.id,
            amount: p.amount,
            currency: p.currency_text,
            status: p.status_text,
            gateway: p.gateway,
            card_brand: p.card_brand,
            card_last_four: p.card_last_four,
            processed_at: p.processed_at.map(|t| t.to_rfc3339()),
            refunds: vec![],
        })
        .collect();
    attach_refunds(&state, order.id, &mut payment_responses).await;
    let fulfillments = load_fulfillments(&state, order.id).await.map_err(|e| {
        tracing::error!("Failed to load fulfillments for order {}: {}", order.id, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Database error"})),
        )
    })?;

    Ok(Json(OrderResponse {
        id: order.id,
        order_number: order.order_number,
        customer_id: order.customer_id,
        customer_email: order.customer_email,
        status: format!("{:?}", order.status).to_lowercase(),
        payment_status: format!("{:?}", order.payment_status).to_lowercase(),
        fulfillment_status: format!("{:?}", order.fulfillment_status).to_lowercase(),
        currency: order.currency.to_string(),
        subtotal: order.subtotal,
        tax_total: order.tax_total,
        shipping_total: order.shipping_total,
        discount_total: order.discount_total,
        coupon_code: order.coupon_code.clone(),
        total: order.total,
        items: item_responses,
        payments: payment_responses,
        billing_address: order.billing_address.clone(),
        shipping_address: order.shipping_address.clone(),
        notes: order.notes.clone(),
        fulfillments,
        created_at: order.created_at.to_rfc3339(),
    }))
}

/// Create a new order
///
/// This endpoint now integrates with TaxService and ShippingService for accurate calculations.
/// For a full checkout flow with shipping selection, use the /checkout endpoints.
pub async fn create_order(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Json(request): Json<CreateOrderRequest>,
) -> Result<(StatusCode, Json<OrderResponse>), (StatusCode, Json<serde_json::Value>)> {
    let customer_id = resolve_order_customer(&auth, request.customer_id);
    let currency = state
        .default_currency
        .parse::<rcommerce_core::models::Currency>()
        .map_err(|e| {
            tracing::error!("Invalid configured store currency: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Invalid store currency configuration"})),
            )
        })?;

    // Validate email
    if request.customer_email.is_empty() || !request.customer_email.contains('@') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Invalid email address"})),
        ));
    }

    // Validate items
    if request.items.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Order must have at least one item"})),
        ));
    }

    let mut order_items = Vec::new();
    let mut subtotal = Decimal::ZERO;
    let mut taxable_items = Vec::new();

    // Process each item
    for item in &request.items {
        if item.quantity <= 0 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Quantity must be positive"})),
            ));
        }

        // Get product details
        let product = match sqlx::query_as::<_, rcommerce_core::models::Product>(
            "SELECT * FROM products WHERE id = $1 AND is_active = true",
        )
        .bind(item.product_id)
        .fetch_optional(state.db.pool())
        .await
        {
            Ok(Some(p)) => p,
            Ok(None) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        serde_json::json!({"error": format!("Product {} not found or inactive", item.product_id)}),
                    ),
                ));
            }
            Err(e) => {
                tracing::error!("Database error: {}", e);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Database error"})),
                ));
            }
        };

        // Check inventory
        let inventory_qty: i32 =
            match sqlx::query_scalar("SELECT inventory_quantity FROM products WHERE id = $1")
                .bind(item.product_id)
                .fetch_one(state.db.pool())
                .await
            {
                Ok(qty) => qty,
                Err(e) => {
                    tracing::error!("Database error: {}", e);
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "Database error"})),
                    ));
                }
            };

        if inventory_qty < item.quantity {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!(
                        "Insufficient inventory for product {}. Available: {}, Requested: {}",
                        product.title, inventory_qty, item.quantity
                    )
                })),
            ));
        }

        let item_subtotal = product.price * Decimal::from(item.quantity);
        subtotal += item_subtotal;

        // Build taxable item for tax calculation
        taxable_items.push(rcommerce_core::tax::TaxableItem {
            id: Uuid::new_v4(),
            product_id: product.id,
            quantity: item.quantity,
            unit_price: product.price,
            total_price: item_subtotal,
            tax_category_id: None, // TODO: Get from product
            is_digital: false,     // TODO: Get from product
            title: product.title.clone(),
            sku: product.sku.clone(),
        });

        order_items.push((product, item.quantity, item_subtotal));
    }

    // Calculate tax using TaxService if shipping address is provided
    let (tax_total, shipping_total) = if let Some(ref address) = request.shipping_address {
        // Calculate tax based on shipping address
        let tax_context = rcommerce_core::tax::TaxContext {
            customer: rcommerce_core::tax::CustomerTaxInfo {
                customer_id,
                is_tax_exempt: false,
                vat_id: None,
                exemptions: vec![],
            },
            shipping_address: rcommerce_core::tax::TaxAddress {
                country_code: address.country.clone(),
                region_code: address.state.clone(),
                postal_code: Some(address.zip.clone()),
                city: Some(address.city.clone()),
            },
            billing_address: rcommerce_core::tax::TaxAddress {
                country_code: address.country.clone(),
                region_code: address.state.clone(),
                postal_code: Some(address.zip.clone()),
                city: Some(address.city.clone()),
            },
            currency,
            transaction_type: rcommerce_core::tax::TransactionType::B2C,
        };

        let tax_calculation = match state
            .tax_service
            .calculate_tax(&taxable_items, &tax_context)
            .await
        {
            Ok(calc) => calc,
            Err(e) => {
                tracing::warn!("Tax calculation failed, using fallback: {}", e);
                // Fallback to simple percentage
                let rate = Decimal::from_str_exact("0.10").unwrap();
                rcommerce_core::tax::TaxCalculation {
                    total_tax: (subtotal * rate).round_dp(2),
                    line_items: vec![],
                    shipping_tax: Decimal::ZERO,
                    tax_breakdown: vec![],
                }
            }
        };

        // Calculate shipping cost
        let package = rcommerce_core::shipping::Package {
            weight: Decimal::from_str_exact("0.5").unwrap()
                * Decimal::from(order_items.iter().map(|(_, qty, _)| qty).sum::<i32>()),
            weight_unit: "kg".to_string(),
            length: Some(Decimal::from(30)),
            width: Some(Decimal::from(20)),
            height: Some(Decimal::from(15)),
            dimension_unit: Some("cm".to_string()),
            predefined_package: None,
        };

        let origin = rcommerce_core::models::Address {
            id: Uuid::new_v4(),
            customer_id: Uuid::nil(),
            first_name: "Store".to_string(),
            last_name: "Warehouse".to_string(),
            company: None,
            phone: None,
            address1: "123 Warehouse St".to_string(),
            address2: None,
            city: "Los Angeles".to_string(),
            state: Some("CA".to_string()),
            country: "US".to_string(),
            zip: "90210".to_string(),
            is_default_shipping: false,
            is_default_billing: false,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let options = rcommerce_core::shipping::RateOptions {
            carriers: None,
            services: None,
            include_insurance: false,
            insurance_value: None,
            signature_confirmation: false,
            adult_signature: false,
            saturday_delivery: false,
            hold_for_pickup: false,
            currency: Some(currency.to_string()),
        };

        let shipping_cost = match state.shipping_factory.get_available().first() {
            Some(provider) => {
                match provider
                    .get_rates(&origin, address, &package, &options)
                    .await
                {
                    Ok(rates) if !rates.is_empty() => rates[0].total_cost,
                    _ => Decimal::ZERO, // Free shipping if no rates available
                }
            }
            None => Decimal::ZERO, // Free shipping if no providers configured
        };

        (tax_calculation.total_tax, shipping_cost)
    } else {
        // Fallback to simple calculations if no address provided
        let tax_rate = Decimal::from_str_exact("0.10").unwrap();
        let tax = (subtotal * tax_rate).round_dp(2);
        (tax, Decimal::ZERO)
    };

    let total = subtotal + tax_total + shipping_total;

    // Generate order number
    let order_number = generate_order_number().await;

    let order_id = Uuid::new_v4();

    // Create order
    let create_order_query = format!(
        r#"
        INSERT INTO orders (
            id, order_number, customer_id, customer_email,
            status, payment_status, fulfillment_status,
            currency, subtotal, tax_total, shipping_total, discount_total, total,
            notes, tags, metadata, draft, order_type
        )
        VALUES (
            $1, $2, $3, $4,
            'pending', 'pending', 'pending',
            $5, $6, $7, $8, 0, $9,
            $10, ARRAY[]::TEXT[], '{{}}'::JSONB, false, 'one_time'
        )
        RETURNING {}
        "#,
        ORDER_COLUMNS
    );
    let order = match sqlx::query_as::<_, rcommerce_core::models::Order>(&create_order_query)
        .bind(order_id)
        .bind(&order_number)
        .bind(customer_id)
        .bind(&request.customer_email)
        .bind(currency)
        .bind(subtotal)
        .bind(tax_total)
        .bind(shipping_total)
        .bind(total)
        .bind(request.notes)
        .fetch_one(state.db.pool())
        .await
    {
        Ok(o) => o,
        Err(e) => {
            tracing::error!("Failed to create order: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to create order"})),
            ));
        }
    };

    // Create order items and update inventory
    for (product, quantity, item_total) in order_items {
        let item_id = Uuid::new_v4();

        if let Err(e) = sqlx::query(
            r#"
            INSERT INTO order_items (
                id, order_id, product_id, variant_id,
                quantity, price, total,
                sku, title, variant_title, requires_shipping
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, true)
            "#,
        )
        .bind(item_id)
        .bind(order_id)
        .bind(product.id)
        .bind(None::<Uuid>) // variant_id
        .bind(quantity)
        .bind(product.price)
        .bind(item_total)
        .bind(&product.sku)
        .bind(&product.title)
        .bind(None::<String>) // variant_title
        .execute(state.db.pool())
        .await
        {
            tracing::error!("Failed to create order item: {}", e);
        }

        // Update inventory on product
        if let Err(e) = sqlx::query(
            "UPDATE products SET inventory_quantity = inventory_quantity - $1, updated_at = NOW() WHERE id = $2"
        )
        .bind(quantity)
        .bind(product.id)
        .execute(state.db.pool())
        .await {
            tracing::error!("Failed to update inventory: {}", e);
        }
    }

    // Get items for response
    let items = match sqlx::query_as::<_, rcommerce_core::models::OrderItem>(
        "SELECT * FROM order_items WHERE order_id = $1",
    )
    .bind(order_id)
    .fetch_all(state.db.pool())
    .await
    {
        Ok(items) => items,
        Err(e) => {
            tracing::error!("Database error: {}", e);
            vec![]
        }
    };

    let item_responses: Vec<OrderItemResponse> = items
        .into_iter()
        .map(|i| OrderItemResponse {
            id: i.id,
            product_id: i.product_id,
            variant_id: i.variant_id,
            name: i.title,
            sku: i.sku,
            quantity: i.quantity,
            price: i.price,
            total: i.total,
        })
        .collect();

    let response = OrderResponse {
        id: order.id,
        order_number: order.order_number,
        customer_id: order.customer_id,
        customer_email: order.customer_email,
        status: format!("{:?}", order.status).to_lowercase(),
        payment_status: format!("{:?}", order.payment_status).to_lowercase(),
        fulfillment_status: format!("{:?}", order.fulfillment_status).to_lowercase(),
        currency: order.currency.to_string(),
        subtotal: order.subtotal,
        tax_total: order.tax_total,
        shipping_total: order.shipping_total,
        discount_total: order.discount_total,
        coupon_code: order.coupon_code.clone(),
        total: order.total,
        items: item_responses,
        payments: vec![],
        billing_address: order.billing_address.clone(),
        shipping_address: order.shipping_address.clone(),
        notes: order.notes.clone(),
        fulfillments: vec![],
        created_at: order.created_at.to_rfc3339(),
    };

    Ok((StatusCode::CREATED, Json(response)))
}

fn resolve_order_customer(auth: &JwtAuth, requested_customer_id: Option<Uuid>) -> Option<Uuid> {
    if auth.is_admin() {
        requested_customer_id
    } else {
        Some(auth.customer_id)
    }
}

/// Generate unique order number
async fn generate_order_number() -> String {
    let prefix = "ORD";
    let timestamp = chrono::Utc::now().timestamp();
    let random = rand::random::<u16>();
    format!("{}{}{:05}", prefix, timestamp % 100000, random % 10000)
}

/// GET /orders/:id/invoice - Generate PDF invoice for an order
async fn get_invoice(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
) -> Result<axum::response::Response, (StatusCode, Json<serde_json::Value>)> {
    let order = sqlx::query_as::<_, rcommerce_core::models::Order>(
        "SELECT id, order_number, customer_id, customer_email, currency::text AS currency_text, order_type, subtotal, tax_total, shipping_total, discount_total, total, status, payment_status, fulfillment_status, billing_address_id, shipping_address_id, payment_method, shipping_method, notes, tags, draft, subscription_id, billing_cycle, coupon_code, coupon_id, ip_address, user_agent, metadata, shipping_address, billing_address, created_at, updated_at, completed_at, cancelled_at FROM orders WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Database error"})))
    })?
    .ok_or_else(|| (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Order not found"}))))?;

    // Check access
    if order.customer_id != Some(auth.customer_id) && !auth.is_admin() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Access denied"})),
        ));
    }

    // Get order items
    let items = sqlx::query_as::<_, rcommerce_core::order::OrderItem>(
        "SELECT * FROM order_items WHERE order_id = $1",
    )
    .bind(id)
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Database error"})),
        )
    })?;

    // Build invoice data
    let data = super::invoice::invoice_data_from_order(&order, &items, &state.branding);

    match super::invoice::generate_invoice_html(&data) {
        Ok(html) => {
            let headers: [(axum::http::HeaderName, axum::http::HeaderValue); 1] = [(
                axum::http::header::CONTENT_TYPE,
                "text/html; charset=utf-8".parse().unwrap(),
            )];
            Ok((headers, html).into_response())
        }
        Err(e) => {
            tracing::error!("Failed to generate invoice: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to generate invoice"})),
            ))
        }
    }
}

/// Cancel order request
#[derive(Debug, Deserialize)]
pub struct CancelOrderRequest {
    pub reason: Option<String>,
}

/// POST /orders/:id/cancel - Cancel a pending/confirmed order
///
/// Releases inventory reservations and voids/refunds any payment via the
/// order service. Admin can cancel any order; customers can cancel their own.
pub async fn cancel_order(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
    Json(request): Json<CancelOrderRequest>,
) -> Result<Json<OrderResponse>, Error> {
    // Access check: admin or the order's owner
    let customer_id: Option<Uuid> =
        sqlx::query_scalar("SELECT customer_id FROM orders WHERE id = $1")
            .bind(id)
            .fetch_optional(state.db.pool())
            .await?
            .ok_or_else(|| Error::not_found("Order not found"))?;

    if customer_id != Some(auth.customer_id) && !auth.is_admin() {
        return Err(Error::forbidden("Access denied"));
    }

    let reason = request
        .reason
        .unwrap_or_else(|| "Canceled via API".to_string());

    let order_before = fetch_order(&state, id).await?;
    let payment_captured = matches!(
        order_before.payment_status,
        rcommerce_core::models::PaymentStatus::Paid
    );

    state.order_service.cancel_order(id, reason).await?;

    let order = fetch_order(&state, id).await?;
    if !order.customer_email.is_empty() {
        let notification =
            super::email_helpers::build_order_cancelled_notification(&order, payment_captured);
        super::email_helpers::spawn_send(&state, notification, "order-cancelled email");
    }
    Ok(Json(build_order_response(&state, order).await?))
}

/// Refund order request
#[derive(Debug, Deserialize)]
pub struct RefundOrderRequest {
    /// Optional partial refund amount; full refund when omitted
    pub amount: Option<String>,
    pub reason: Option<String>,
}

/// POST /orders/:id/refund - Refund a paid order through the payment gateway (admin only)
///
/// Refunds via the gateway using the recorded payment transaction id, restocks
/// the ordered items, and sets the order status to 'refunded'.
pub async fn refund_order(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
    Json(request): Json<RefundOrderRequest>,
) -> Result<Json<OrderResponse>, Error> {
    if !auth.is_admin() {
        return Err(Error::forbidden("Admin access required"));
    }

    let amount = match request.amount {
        Some(amt_str) => Some(
            amt_str
                .parse::<Decimal>()
                .map_err(|_| Error::validation("Invalid amount format"))?,
        ),
        None => None,
    };

    let reason = request
        .reason
        .unwrap_or_else(|| "Refunded via API".to_string());

    let order_before = fetch_order(&state, id).await?;
    let refund_amount = amount.unwrap_or(order_before.total);

    state.order_service.refund_order(id, amount, reason).await?;

    let order = fetch_order(&state, id).await?;
    if !order.customer_email.is_empty() {
        super::email_helpers::send_order_refunded_email(&state, id, refund_amount).await;
    }
    Ok(Json(build_order_response(&state, order).await?))
}

#[derive(Debug, Deserialize)]
pub struct CreateFulfillmentRequest {
    pub carrier: String,
    pub tracking_number: String,
}

/// POST /orders/:id/fulfillments - Manually mark an entire order as shipped (admin only)
pub async fn create_fulfillment(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
    Json(request): Json<CreateFulfillmentRequest>,
) -> Result<(StatusCode, Json<OrderResponse>), Error> {
    if !auth.is_admin() {
        return Err(Error::forbidden("Admin access required"));
    }

    let carrier = canonical_carrier(&request.carrier)
        .ok_or_else(|| Error::validation("Unsupported shipping carrier"))?;
    let tracking_number = validate_tracking_number(&request.tracking_number)?;
    let tracking_url = carrier_tracking_url(carrier, &tracking_number);

    persist_manual_fulfillment(&state, id, carrier, &tracking_number, &tracking_url).await?;
    super::email_helpers::send_order_shipped_email(
        &state,
        id,
        &tracking_number,
        Some(carrier),
        Some(&tracking_url),
    )
    .await;

    let order = fetch_order(&state, id).await?;
    Ok((
        StatusCode::CREATED,
        Json(build_order_response(&state, order).await?),
    ))
}

async fn persist_manual_fulfillment(
    state: &AppState,
    order_id: Uuid,
    carrier: &str,
    tracking_number: &str,
    tracking_url: &str,
) -> Result<(), Error> {
    let mut tx = state.db.pool().begin().await?;
    let order = sqlx::query_as::<_, rcommerce_core::models::Order>(&format!(
        "SELECT {} FROM orders WHERE id = $1 FOR UPDATE",
        ORDER_COLUMNS
    ))
    .bind(order_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| Error::not_found("Order not found"))?;

    let order_eligible = matches!(
        order.status,
        rcommerce_core::models::OrderStatus::Confirmed
            | rcommerce_core::models::OrderStatus::Processing
    );
    let fulfillment_eligible = matches!(
        order.fulfillment_status,
        rcommerce_core::models::FulfillmentStatus::Pending
            | rcommerce_core::models::FulfillmentStatus::Processing
    );
    if !order_eligible || !fulfillment_eligible {
        return Err(Error::validation(
            "Order is not eligible for manual fulfillment",
        ));
    }

    let fulfillment_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO fulfillments (id, order_id, status, tracking_number, tracking_url, tracking_company, shipped_at) VALUES ($1, $2, 'shipped', $3, $4, $5, NOW())",
    )
    .bind(fulfillment_id)
    .bind(order_id)
    .bind(tracking_number)
    .bind(tracking_url)
    .bind(carrier)
    .execute(&mut *tx)
    .await?;

    let fulfilled_items = sqlx::query(
        "INSERT INTO fulfillment_items (id, fulfillment_id, order_item_id, quantity) SELECT gen_random_uuid(), $1, id, quantity FROM order_items WHERE order_id = $2",
    )
    .bind(fulfillment_id)
    .bind(order_id)
    .execute(&mut *tx)
    .await?;
    if fulfilled_items.rows_affected() == 0 {
        return Err(Error::validation("Order has no items to fulfill"));
    }

    let note = format!(
        "[{}] Shipped manually via {}. Tracking number: {}. Tracking URL: {}",
        chrono::Utc::now().to_rfc3339(),
        carrier,
        tracking_number,
        tracking_url
    );
    sqlx::query(
        "UPDATE orders SET fulfillment_status = 'shipped', notes = CASE WHEN notes IS NULL OR btrim(notes) = '' THEN $2 ELSE notes || E'\\n' || $2 END, updated_at = NOW() WHERE id = $1",
    )
    .bind(order_id)
    .bind(note)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

fn canonical_carrier(value: &str) -> Option<&'static str> {
    match value
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '-'], "_")
        .as_str()
    {
        "dhl" => Some("DHL"),
        "ups" => Some("UPS"),
        "fedex" => Some("FedEx"),
        "sf_express" => Some("SF Express"),
        _ => None,
    }
}

fn validate_tracking_number(value: &str) -> Result<String, Error> {
    let tracking_number = value.trim();
    if tracking_number.is_empty()
        || tracking_number.len() > 128
        || !tracking_number
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(Error::validation(
            "Tracking number must contain only letters, numbers, and hyphens (maximum 128 characters)",
        ));
    }
    Ok(tracking_number.to_string())
}

fn carrier_tracking_url(carrier: &str, tracking_number: &str) -> String {
    let encoded: String =
        url::form_urlencoded::byte_serialize(tracking_number.as_bytes()).collect();
    match carrier {
        "DHL" => format!(
            "https://www.dhl.com/en/express/tracking.html?AWB={encoded}"
        ),
        "UPS" => format!("https://www.ups.com/track?tracknum={encoded}"),
        "FedEx" => format!("https://www.fedex.com/fedextrack/?trknbr={encoded}"),
        "SF Express" => format!(
            "https://www.sf-express.com/us/en/dynamic_function/waybill/#search/bill-number/{encoded}"
        ),
        _ => unreachable!("carrier was validated"),
    }
}

async fn load_fulfillments(
    state: &AppState,
    order_id: Uuid,
) -> Result<Vec<OrderFulfillmentResponse>, Error> {
    sqlx::query_as::<_, OrderFulfillmentResponse>(
        "SELECT id, status::text AS status, tracking_number, tracking_url, tracking_company, shipped_at, created_at FROM fulfillments WHERE order_id = $1 ORDER BY created_at DESC",
    )
    .bind(order_id)
    .fetch_all(state.db.pool())
    .await
    .map_err(Error::from)
}

/// Update order status request
#[derive(Debug, Deserialize)]
pub struct UpdateOrderStatusRequest {
    pub status: String,
    pub reason: Option<String>,
}

/// PUT /orders/:id/status - Admin order status transition (admin only)
///
/// The requested transition is validated against the allowed transitions in
/// `order::lifecycle::OrderStatus::can_transition_to`.
pub async fn update_order_status(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateOrderStatusRequest>,
) -> Result<Json<OrderResponse>, Error> {
    if !auth.is_admin() {
        return Err(Error::forbidden("Admin access required"));
    }

    let new_status = parse_order_status(&request.status)
        .ok_or_else(|| Error::validation(format!("Invalid order status: {}", request.status)))?;

    // The consolidated schema has no 'shipped'/'delivered' order_status labels;
    // shipping progress is tracked via fulfillments
    if matches!(
        new_status,
        LifecycleOrderStatus::Shipped | LifecycleOrderStatus::Delivered
    ) {
        return Err(Error::validation(
            "Status not supported for orders; use the fulfillments API instead",
        ));
    }

    let order = fetch_order(&state, id).await?;
    let current_status = map_models_status(order.status);

    if !current_status.can_transition_to(new_status) {
        return Err(Error::validation(format!(
            "Invalid status transition from {:?} to {:?}",
            current_status, new_status
        )));
    }

    state
        .order_service
        .update_order_status(id, new_status)
        .await?;

    let order = fetch_order(&state, id).await?;
    Ok(Json(build_order_response(&state, order).await?))
}

/// Fetch an order by id (models::Order shape used by the response builders)
async fn fetch_order(state: &AppState, id: Uuid) -> Result<rcommerce_core::models::Order, Error> {
    sqlx::query_as::<_, rcommerce_core::models::Order>(&format!(
        "SELECT {} FROM orders WHERE id = $1",
        ORDER_COLUMNS
    ))
    .bind(id)
    .fetch_optional(state.db.pool())
    .await?
    .ok_or_else(|| Error::not_found("Order not found"))
}

/// Load refunds for an order and attach them to their parent payment responses.
async fn attach_refunds(state: &AppState, order_id: Uuid, payments: &mut [OrderPaymentResponse]) {
    let refund_rows = sqlx::query_as::<_, RefundRow>(
        r#"
        SELECT id, payment_id, amount, currency::text AS currency_text,
               status::text AS status_text, reason, gateway_refund_id,
               created_at, settled_at
        FROM refunds
        WHERE order_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(order_id)
    .fetch_all(state.db.pool())
    .await
    .unwrap_or_default();

    for payment in payments.iter_mut() {
        payment.refunds = refund_rows
            .iter()
            .filter(|r| r.payment_id == payment.id)
            .map(|r| OrderRefundResponse {
                id: r.id,
                amount: r.amount,
                currency: r.currency_text.clone(),
                status: r.status_text.clone(),
                reason: r.reason.clone(),
                gateway_refund_id: r.gateway_refund_id.clone(),
                created_at: r.created_at.to_rfc3339(),
                settled_at: r.settled_at.map(|t| t.to_rfc3339()),
            })
            .collect();
    }
}

/// Build an order response with items and payment records loaded
async fn build_order_response(
    state: &AppState,
    order: rcommerce_core::models::Order,
) -> Result<OrderResponse, Error> {
    let items = sqlx::query_as::<_, rcommerce_core::models::OrderItem>(
        "SELECT * FROM order_items WHERE order_id = $1",
    )
    .bind(order.id)
    .fetch_all(state.db.pool())
    .await?;

    let item_responses: Vec<OrderItemResponse> = items
        .into_iter()
        .map(|i| OrderItemResponse {
            id: i.id,
            product_id: i.product_id,
            variant_id: i.variant_id,
            name: i.title,
            sku: i.sku,
            quantity: i.quantity,
            price: i.price,
            total: i.total,
        })
        .collect();

    let payment_rows = sqlx::query_as::<_, PaymentRow>(
        r#"
        SELECT id, amount, currency::text AS currency_text, status::text AS status_text,
               gateway, card_brand, card_last_four, processed_at
        FROM payments
        WHERE order_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(order.id)
    .fetch_all(state.db.pool())
    .await
    .unwrap_or_default();

    let mut payment_responses: Vec<OrderPaymentResponse> = payment_rows
        .into_iter()
        .map(|p| OrderPaymentResponse {
            id: p.id,
            amount: p.amount,
            currency: p.currency_text,
            status: p.status_text,
            gateway: p.gateway,
            card_brand: p.card_brand,
            card_last_four: p.card_last_four,
            processed_at: p.processed_at.map(|t| t.to_rfc3339()),
            refunds: vec![],
        })
        .collect();
    attach_refunds(state, order.id, &mut payment_responses).await;
    let fulfillments = load_fulfillments(state, order.id).await?;

    Ok(OrderResponse {
        id: order.id,
        order_number: order.order_number,
        customer_id: order.customer_id,
        customer_email: order.customer_email,
        status: format!("{:?}", order.status).to_lowercase(),
        payment_status: format!("{:?}", order.payment_status).to_lowercase(),
        fulfillment_status: format!("{:?}", order.fulfillment_status).to_lowercase(),
        currency: order.currency.to_string(),
        subtotal: order.subtotal,
        tax_total: order.tax_total,
        shipping_total: order.shipping_total,
        discount_total: order.discount_total,
        coupon_code: order.coupon_code.clone(),
        total: order.total,
        items: item_responses,
        payments: payment_responses,
        billing_address: order.billing_address.clone(),
        shipping_address: order.shipping_address.clone(),
        notes: order.notes.clone(),
        fulfillments,
        created_at: order.created_at.to_rfc3339(),
    })
}

/// Parse a status string into the lifecycle order status enum.
/// Accepts both 'cancelled' (database spelling) and 'canceled'.
fn parse_order_status(status: &str) -> Option<LifecycleOrderStatus> {
    match status.to_lowercase().as_str() {
        "pending" => Some(LifecycleOrderStatus::Pending),
        "confirmed" => Some(LifecycleOrderStatus::Confirmed),
        "processing" => Some(LifecycleOrderStatus::Processing),
        "on_hold" => Some(LifecycleOrderStatus::OnHold),
        "shipped" => Some(LifecycleOrderStatus::Shipped),
        "delivered" => Some(LifecycleOrderStatus::Delivered),
        "completed" => Some(LifecycleOrderStatus::Completed),
        "cancelled" | "canceled" => Some(LifecycleOrderStatus::Canceled),
        "refunded" => Some(LifecycleOrderStatus::Refunded),
        _ => None,
    }
}

/// Map the models order status (as read from the database) to the lifecycle enum
fn map_models_status(status: rcommerce_core::models::OrderStatus) -> LifecycleOrderStatus {
    match status {
        rcommerce_core::models::OrderStatus::Pending => LifecycleOrderStatus::Pending,
        rcommerce_core::models::OrderStatus::Confirmed => LifecycleOrderStatus::Confirmed,
        rcommerce_core::models::OrderStatus::Processing => LifecycleOrderStatus::Processing,
        rcommerce_core::models::OrderStatus::OnHold => LifecycleOrderStatus::OnHold,
        rcommerce_core::models::OrderStatus::Completed => LifecycleOrderStatus::Completed,
        rcommerce_core::models::OrderStatus::Cancelled => LifecycleOrderStatus::Canceled,
        rcommerce_core::models::OrderStatus::Refunded => LifecycleOrderStatus::Refunded,
    }
}

/// Router for order routes
pub fn router() -> Router<AppState> {
    axum::Router::new()
        .route("/orders", get(list_orders).post(create_order))
        .route("/orders/:id", get(get_order))
        .route("/orders/:id/invoice", get(get_invoice))
        .route("/orders/:id/cancel", post(cancel_order))
        .route("/orders/:id/refund", post(refund_order))
        .route("/orders/:id/fulfillments", post(create_fulfillment))
        .route("/orders/:id/status", put(update_order_status))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth(permissions: &[&str]) -> JwtAuth {
        JwtAuth {
            customer_id: Uuid::new_v4(),
            email: "actor@example.com".to_string(),
            permissions: permissions.iter().map(|p| p.to_string()).collect(),
        }
    }

    #[test]
    fn non_admin_order_owner_is_always_the_authenticated_customer() {
        for permissions in [vec!["read"], vec!["read", "write"]] {
            let auth = auth(&permissions);
            assert_eq!(
                resolve_order_customer(&auth, Some(Uuid::new_v4())),
                Some(auth.customer_id)
            );
            assert_eq!(resolve_order_customer(&auth, None), Some(auth.customer_id));
        }
    }

    #[test]
    fn admin_order_owner_can_be_another_customer_or_guest() {
        let auth = auth(&["read", "write", "admin"]);
        let customer_id = Uuid::new_v4();
        assert_eq!(
            resolve_order_customer(&auth, Some(customer_id)),
            Some(customer_id)
        );
        assert_eq!(resolve_order_customer(&auth, None), None);
    }

    #[test]
    fn manual_carriers_generate_official_tracking_links() {
        for (input, carrier, prefix) in [
            (
                "dhl",
                "DHL",
                "https://www.dhl.com/en/express/tracking.html?AWB=",
            ),
            ("UPS", "UPS", "https://www.ups.com/track?tracknum="),
            (
                "fedex",
                "FedEx",
                "https://www.fedex.com/fedextrack/?trknbr=",
            ),
            (
                "sf express",
                "SF Express",
                "https://www.sf-express.com/us/en/dynamic_function/waybill/#search/bill-number/",
            ),
        ] {
            let canonical = canonical_carrier(input).unwrap();
            assert_eq!(canonical, carrier);
            assert_eq!(
                carrier_tracking_url(canonical, "1Z-999"),
                format!("{prefix}1Z-999")
            );
        }
        assert_eq!(canonical_carrier("sf-express"), Some("SF Express"));
        assert_eq!(canonical_carrier("other"), None);
    }

    #[test]
    fn manual_tracking_numbers_are_trimmed_and_bounded() {
        assert_eq!(
            validate_tracking_number("  1Z999AA99999999999  ").unwrap(),
            "1Z999AA99999999999"
        );
        for invalid in ["", "  ", "TRACK/123", "TRACK 123", "轨道123"] {
            assert!(validate_tracking_number(invalid).is_err(), "{invalid}");
        }
        assert!(validate_tracking_number(&"A".repeat(129)).is_err());
    }
}
