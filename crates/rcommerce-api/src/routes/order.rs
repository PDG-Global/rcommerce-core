use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use rust_decimal::Decimal;

use rcommerce_core::tax::TaxService;
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
    pub total: Decimal,
    pub items: Vec<OrderItemResponse>,
    pub created_at: String,
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
        format!("SELECT {} FROM orders ORDER BY created_at DESC LIMIT 50", ORDER_COLUMNS)
    } else {
        format!("SELECT {} FROM orders WHERE customer_id = $1 ORDER BY created_at DESC LIMIT 50", ORDER_COLUMNS)
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
                    total: o.total,
                    items: vec![], // Will be populated separately
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
    let order = match sqlx::query_as::<_, rcommerce_core::models::Order>(
        &format!("SELECT {} FROM orders WHERE id = $1", ORDER_COLUMNS),
    )
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
        total: order.total,
        items: item_responses,
        created_at: order.created_at.to_rfc3339(),
    }))
}

/// Create a new order
///
/// This endpoint now integrates with TaxService and ShippingService for accurate calculations.
/// For a full checkout flow with shipping selection, use the /checkout endpoints.
pub async fn create_order(
    State(state): State<AppState>,
    Extension(_auth): Extension<JwtAuth>,
    Json(request): Json<CreateOrderRequest>,
) -> Result<(StatusCode, Json<OrderResponse>), (StatusCode, Json<serde_json::Value>)> {
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
                customer_id: request.customer_id,
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
            currency: rcommerce_core::models::Currency::USD,
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
            currency: Some("USD".to_string()),
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
    let order = match sqlx::query_as::<_, rcommerce_core::models::Order>(
        r#"
        INSERT INTO orders (
            id, order_number, customer_id, email,
            status, payment_status, fulfillment_status,
            currency, subtotal, tax_total, shipping_total, discount_total, total,
            notes, tags, metadata, draft, order_type
        )
        VALUES (
            $1, $2, $3, $4,
            'pending', 'pending', 'pending',
            'USD', $5, $6, $7, 0, $8,
            $9, ARRAY[]::TEXT[], '{}'::JSONB, false, 'one_time'
        )
        RETURNING *
        "#,
    )
    .bind(order_id)
    .bind(&order_number)
    .bind(request.customer_id)
    .bind(&request.customer_email)
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
        total: order.total,
        items: item_responses,
        created_at: order.created_at.to_rfc3339(),
    };

    Ok((StatusCode::CREATED, Json(response)))
}

/// Generate unique order number
async fn generate_order_number() -> String {
    let prefix = "ORD";
    let timestamp = chrono::Utc::now().timestamp();
    let random = rand::random::<u16>();
    format!("{}{}{:05}", prefix, timestamp % 100000, random % 10000)
}

/// Router for order routes
pub fn router() -> Router<AppState> {
    axum::Router::new()
        .route("/orders", get(list_orders).post(create_order))
        .route("/orders/:id", get(get_order))
}
