//! WooCommerce-compatible order endpoints.
//!
//! GET /wp-json/wc/v3/orders
//! GET /wp-json/wc/v3/orders/:id
//! PUT /wp-json/wc/v3/orders/:id

use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    Extension, Json,
};
use chrono;
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::middleware::api_key_auth::ApiKeyAuth;
use crate::state::AppState;

use super::pagination;
use super::types::*;

#[derive(Debug, Deserialize, Default)]
pub struct OrderListParams {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub status: Option<String>,
    pub after: Option<String>,
    #[serde(alias = "modified_after")]
    pub modified_after: Option<String>,
    pub before: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateOrderRequest {
    pub status: Option<String>,
}

/// WooCommerce order-note payload (extra fields are ignored)
#[derive(Debug, Deserialize)]
pub struct CreateOrderNoteRequest {
    pub note: String,
    #[serde(default)]
    pub customer_note: Option<bool>,
}

/// Convert a UUID to a deterministic integer ID for WooCommerce compatibility
fn uuid_to_wc_int(id: uuid::Uuid) -> i64 {
    i64::from_be_bytes([
        id.as_bytes()[0],
        id.as_bytes()[1],
        id.as_bytes()[2],
        id.as_bytes()[3],
        id.as_bytes()[4],
        id.as_bytes()[5],
        id.as_bytes()[6],
        id.as_bytes()[7],
    ])
    .unsigned_abs() as i64
}

/// Map a single order item to the WooCommerce line-item shape.
///
/// `variant_options` carries the option name/value pairs for the item's
/// variant (from product_option_values/product_options); each pair is exposed
/// as a WooCommerce `meta_data` entry, which is how Dianxiaomi reads the
/// customer's option selection (e.g. "Degree: 50").
fn build_wc_line_item(
    item: &rcommerce_core::order::OrderItem,
    image_src: Option<&str>,
    weight_info: Option<&(Option<Decimal>, Option<String>)>,
    variant_options: Option<&[(String, String)]>,
) -> WcLineItem {
    let item_id_int = uuid_to_wc_int(item.id);
    let weight_val = weight_info
        .and_then(|(w, _)| w.as_ref().copied())
        .unwrap_or_default();
    let weight_unit = weight_info.and_then(|(_, u)| u.as_deref()).unwrap_or("kg");

    let mut meta: Vec<serde_json::Value> = Vec::new();
    if let Some(options) = variant_options {
        for (name, value) in options {
            meta.push(serde_json::json!({"key": name, "value": value}));
        }
    }
    if weight_val > rust_decimal::Decimal::ZERO {
        meta.push(serde_json::json!({"key": "_weight", "value": weight_val.to_string()}));
        meta.push(serde_json::json!({"key": "_weight_unit", "value": weight_unit}));
        meta.push(serde_json::json!({"key": "_length", "value": ""}));
        meta.push(serde_json::json!({"key": "_width", "value": ""}));
        meta.push(serde_json::json!({"key": "_height", "value": ""}));
    }

    let variation_id = item.variant_id.map(uuid_to_wc_int).unwrap_or(0);

    WcLineItem {
        id: item_id_int.to_string(),
        name: item.name.clone(),
        product_id: uuid_to_wc_int(item.product_id).to_string(),
        variation_id,
        quantity: item.quantity,
        tax_class: String::new(),
        subtotal: format_price(&item.subtotal),
        subtotal_tax: "0.00".to_string(),
        total: format_price(&item.total),
        total_tax: format_price(&item.tax_amount),
        taxes: vec![],
        meta_data: meta,
        sku: item.sku.clone().unwrap_or_default(),
        price: format_price(&item.price),
        image: WcLineItemImage {
            id: item.product_id.to_string(),
            src: image_src.unwrap_or_default().to_string(),
        },
    }
}

/// Map an rCommerce order to WooCommerce format
async fn map_order(
    order: &rcommerce_core::models::Order,
    items: &[rcommerce_core::order::OrderItem],
    pool: &sqlx::PgPool,
) -> WcOrder {
    let billing_json = order
        .billing_address
        .as_ref()
        .or(order.shipping_address.as_ref());

    let shipping_json = order.shipping_address.as_ref();

    // Fetch product images for all items in this order
    let product_ids: Vec<uuid::Uuid> = items.iter().map(|i| i.product_id).collect();
    let image_map: std::collections::HashMap<uuid::Uuid, String> = if !product_ids.is_empty() {
        sqlx::query_as::<_, (uuid::Uuid, String)>(
            "SELECT DISTINCT ON (product_id) product_id, src FROM product_images WHERE product_id = ANY($1) ORDER BY product_id, position"
        )
        .bind(&product_ids)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .collect()
    } else {
        std::collections::HashMap::new()
    };

    // Fetch product weight for all items
    let weight_map: std::collections::HashMap<
        uuid::Uuid,
        (Option<rust_decimal::Decimal>, Option<String>),
    > = if !product_ids.is_empty() {
        sqlx::query_as::<_, (uuid::Uuid, Option<rust_decimal::Decimal>, Option<String>)>(
            "SELECT id, weight, weight_unit FROM products WHERE id = ANY($1)",
        )
        .bind(&product_ids)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(id, w, u)| (id, (w, u)))
        .collect()
    } else {
        std::collections::HashMap::new()
    };

    fn extract_address(json: Option<&serde_json::Value>, email: &str) -> WcAddress {
        match json {
            Some(v) => WcAddress {
                first_name: v
                    .get("first_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                last_name: v
                    .get("last_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                address_1: v
                    .get("address1")
                    .and_then(|v| v.as_str())
                    .or_else(|| v.get("address_1").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string(),
                address_2: v
                    .get("address2")
                    .and_then(|v| v.as_str())
                    .or_else(|| v.get("address_2").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string(),
                city: v
                    .get("city")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                state: v
                    .get("state")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                postcode: v
                    .get("zip")
                    .and_then(|v| v.as_str())
                    .or_else(|| v.get("postcode").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_string(),
                country: v
                    .get("country")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                email: email.to_string(),
                phone: v
                    .get("phone")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            },
            None => WcAddress {
                first_name: String::new(),
                last_name: String::new(),
                address_1: String::new(),
                address_2: String::new(),
                city: String::new(),
                state: String::new(),
                postcode: String::new(),
                country: String::new(),
                email: email.to_string(),
                phone: String::new(),
            },
        }
    }

    // Convert UUID to a deterministic integer ID for WooCommerce compatibility
    let order_id_int = {
        let bytes = order.id.as_bytes();
        i64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ])
        .unsigned_abs() as i64
    };

    // Fetch option name/value pairs for all variants in this order in one
    // query (Dianxiaomi reads the customer's selection from meta_data).
    let variant_ids: Vec<uuid::Uuid> = items.iter().filter_map(|i| i.variant_id).collect();
    let variant_options: std::collections::HashMap<uuid::Uuid, Vec<(String, String)>> =
        if !variant_ids.is_empty() {
            sqlx::query_as::<_, (uuid::Uuid, String, String)>(
                "SELECT pov.variant_id, po.name, pov.value \
                 FROM product_option_values pov \
                 JOIN product_options po ON po.id = pov.option_id \
                 WHERE pov.variant_id = ANY($1) \
                 ORDER BY po.position, po.name",
            )
            .bind(&variant_ids)
            .fetch_all(pool)
            .await
            .unwrap_or_default()
            .into_iter()
            .fold(std::collections::HashMap::new(), |mut map, (vid, name, value)| {
                map.entry(vid).or_insert_with(Vec::new).push((name, value));
                map
            })
        } else {
            std::collections::HashMap::new()
        };

    let line_items: Vec<WcLineItem> = items
        .iter()
        .map(|item| {
            build_wc_line_item(
                item,
                image_map.get(&item.product_id).map(|s| s.as_str()),
                weight_map.get(&item.product_id),
                item.variant_id
                    .and_then(|vid| variant_options.get(&vid))
                    .map(|v| v.as_slice()),
            )
        })
        .collect();

    WcOrder {
        id: order_id_int.to_string(),
        parent_id: 0,
        number: order.order_number.clone(),
        order_key: format!(
            "wc_order_{}",
            order.id.to_string().replace("-", "")[..12].to_string()
        ),
        created_via: "checkout".to_string(),
        version: "9.9.0".to_string(),
        status: map_order_status(&format!("{:?}", order.status).to_lowercase()),
        currency: order.currency.to_string(),
        date_created: order.created_at.to_rfc3339(),
        date_created_gmt: order.created_at.to_rfc3339().replace("+00:00", "Z"),
        date_modified: order.updated_at.to_rfc3339(),
        date_modified_gmt: order.updated_at.to_rfc3339().replace("+00:00", "Z"),
        discount_total: format_price(&order.discount_total),
        discount_tax: "0.00".to_string(),
        shipping_total: format_price(&order.shipping_total),
        shipping_tax: "0.00".to_string(),
        cart_tax: "0.00".to_string(),
        total: format_price(&order.total),
        total_tax: format_price(&order.tax_total),
        prices_include_tax: false,
        customer_id: 0,
        customer_note: String::new(),
        payment_method: "stripe".to_string(),
        payment_method_title: "Credit Card (Stripe)".to_string(),
        transaction_id: String::new(),
        billing: extract_address(billing_json, &order.customer_email),
        shipping: extract_address(shipping_json, &order.customer_email),
        date_completed: order.completed_at.map(|dt| dt.to_rfc3339()),
        date_completed_gmt: order
            .completed_at
            .map(|dt| dt.to_rfc3339().replace("+00:00", "Z")),
        date_paid: order.completed_at.map(|dt| dt.to_rfc3339()),
        date_paid_gmt: order
            .completed_at
            .map(|dt| dt.to_rfc3339().replace("+00:00", "Z")),
        cart_hash: order.id.to_string().replace("-", "")[..32].to_string(),
        meta_data: vec![],
        line_items,
        tax_lines: vec![],
        shipping_lines: if order.shipping_total > Decimal::ZERO {
            vec![WcShippingLine {
                id: "1".to_string(),
                method_title: "Standard Shipping".to_string(),
                method_id: "manual:standard".to_string(),
                total: format_price(&order.shipping_total),
                total_tax: "0.00".to_string(),
            }]
        } else {
            vec![]
        },
        fee_lines: vec![],
        coupon_lines: vec![],
        refunds: vec![],
    }
}

/// GET /wp-json/wc/v3/orders
pub async fn list_orders(
    State(state): State<AppState>,
    Extension(auth): Extension<ApiKeyAuth>,
    Query(params): Query<OrderListParams>,
) -> Response {
    let (page, per_page) = pagination::parse_pagination(params.page, params.per_page);
    let customer_id = auth.customer_id;

    // Map WooCommerce status to rCommerce status
    let rcommerce_status = params.status.as_ref().map(|s| unmap_order_status(s));

    // Parse after date filter (ISO8601 format)
    let after_date = params
        .after
        .as_ref()
        .or(params.modified_after.as_ref())
        .and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc))
        });

    // If querying for "processing" status and no after parameter, filter to last 72 hours
    let effective_after =
        if rcommerce_status.as_deref() == Some("confirmed") && after_date.is_none() {
            Some(chrono::Utc::now() - chrono::Duration::hours(72))
        } else {
            after_date
        };

    let orders = if let Some(cid) = customer_id {
        if let Some(ref status) = rcommerce_status {
            if let Some(after) = effective_after {
                sqlx::query_as::<_, rcommerce_core::models::Order>(
                    "SELECT id, order_number, customer_id, customer_email, currency::text AS currency_text, order_type, subtotal, tax_total, shipping_total, discount_total, total, status, payment_status, fulfillment_status, billing_address_id, shipping_address_id, payment_method, shipping_method, notes, tags, draft, subscription_id, billing_cycle, coupon_code, coupon_id, ip_address, user_agent, metadata, shipping_address, billing_address, created_at, updated_at, completed_at, cancelled_at FROM orders WHERE customer_id = $1 AND status = $2::order_status AND created_at >= $3 ORDER BY created_at DESC LIMIT $4 OFFSET $5",
                )
                .bind(cid)
                .bind(status)
                .bind(after)
                .bind(per_page)
                .bind(pagination::offset(page, per_page))
                .fetch_all(state.db.pool())
                .await
            } else {
                sqlx::query_as::<_, rcommerce_core::models::Order>(
                    "SELECT id, order_number, customer_id, customer_email, currency::text AS currency_text, order_type, subtotal, tax_total, shipping_total, discount_total, total, status, payment_status, fulfillment_status, billing_address_id, shipping_address_id, payment_method, shipping_method, notes, tags, draft, subscription_id, billing_cycle, coupon_code, coupon_id, ip_address, user_agent, metadata, shipping_address, billing_address, created_at, updated_at, completed_at, cancelled_at FROM orders WHERE customer_id = $1 AND status = $2::order_status ORDER BY created_at DESC LIMIT $3 OFFSET $4",
                )
                .bind(cid)
                .bind(status)
                .bind(per_page)
                .bind(pagination::offset(page, per_page))
                .fetch_all(state.db.pool())
                .await
            }
        } else {
            if let Some(after) = effective_after {
                sqlx::query_as::<_, rcommerce_core::models::Order>(
                    "SELECT id, order_number, customer_id, customer_email, currency::text AS currency_text, order_type, subtotal, tax_total, shipping_total, discount_total, total, status, payment_status, fulfillment_status, billing_address_id, shipping_address_id, payment_method, shipping_method, notes, tags, draft, subscription_id, billing_cycle, coupon_code, coupon_id, ip_address, user_agent, metadata, shipping_address, billing_address, created_at, updated_at, completed_at, cancelled_at FROM orders WHERE customer_id = $1 AND created_at >= $2 ORDER BY created_at DESC LIMIT $3 OFFSET $4",
                )
                .bind(cid)
                .bind(after)
                .bind(per_page)
                .bind(pagination::offset(page, per_page))
                .fetch_all(state.db.pool())
                .await
            } else {
                sqlx::query_as::<_, rcommerce_core::models::Order>(
                    "SELECT id, order_number, customer_id, customer_email, currency::text AS currency_text, order_type, subtotal, tax_total, shipping_total, discount_total, total, status, payment_status, fulfillment_status, billing_address_id, shipping_address_id, payment_method, shipping_method, notes, tags, draft, subscription_id, billing_cycle, coupon_code, coupon_id, ip_address, user_agent, metadata, shipping_address, billing_address, created_at, updated_at, completed_at, cancelled_at FROM orders WHERE customer_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
                )
                .bind(cid)
                .bind(per_page)
                .bind(pagination::offset(page, per_page))
                .fetch_all(state.db.pool())
                .await
            }
        }
    } else {
        if let Some(ref status) = rcommerce_status {
            if let Some(after) = effective_after {
                sqlx::query_as::<_, rcommerce_core::models::Order>(
                    "SELECT id, order_number, customer_id, customer_email, currency::text AS currency_text, order_type, subtotal, tax_total, shipping_total, discount_total, total, status, payment_status, fulfillment_status, billing_address_id, shipping_address_id, payment_method, shipping_method, notes, tags, draft, subscription_id, billing_cycle, coupon_code, coupon_id, ip_address, user_agent, metadata, shipping_address, billing_address, created_at, updated_at, completed_at, cancelled_at FROM orders WHERE status = $1::order_status AND created_at >= $2 ORDER BY created_at DESC LIMIT $3 OFFSET $4",
                )
                .bind(status)
                .bind(after)
                .bind(per_page)
                .bind(pagination::offset(page, per_page))
                .fetch_all(state.db.pool())
                .await
            } else {
                sqlx::query_as::<_, rcommerce_core::models::Order>(
                    "SELECT id, order_number, customer_id, customer_email, currency::text AS currency_text, order_type, subtotal, tax_total, shipping_total, discount_total, total, status, payment_status, fulfillment_status, billing_address_id, shipping_address_id, payment_method, shipping_method, notes, tags, draft, subscription_id, billing_cycle, coupon_code, coupon_id, ip_address, user_agent, metadata, shipping_address, billing_address, created_at, updated_at, completed_at, cancelled_at FROM orders WHERE status = $1::order_status ORDER BY created_at DESC LIMIT $2 OFFSET $3",
                )
                .bind(status)
                .bind(per_page)
                .bind(pagination::offset(page, per_page))
                .fetch_all(state.db.pool())
                .await
            }
        } else {
            if let Some(after) = effective_after {
                sqlx::query_as::<_, rcommerce_core::models::Order>(
                    "SELECT id, order_number, customer_id, customer_email, currency::text AS currency_text, order_type, subtotal, tax_total, shipping_total, discount_total, total, status, payment_status, fulfillment_status, billing_address_id, shipping_address_id, payment_method, shipping_method, notes, tags, draft, subscription_id, billing_cycle, coupon_code, coupon_id, ip_address, user_agent, metadata, shipping_address, billing_address, created_at, updated_at, completed_at, cancelled_at FROM orders WHERE created_at >= $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
                )
                .bind(after)
                .bind(per_page)
                .bind(pagination::offset(page, per_page))
                .fetch_all(state.db.pool())
                .await
            } else {
                sqlx::query_as::<_, rcommerce_core::models::Order>(
                    "SELECT id, order_number, customer_id, customer_email, currency::text AS currency_text, order_type, subtotal, tax_total, shipping_total, discount_total, total, status, payment_status, fulfillment_status, billing_address_id, shipping_address_id, payment_method, shipping_method, notes, tags, draft, subscription_id, billing_cycle, coupon_code, coupon_id, ip_address, user_agent, metadata, shipping_address, billing_address, created_at, updated_at, completed_at, cancelled_at FROM orders ORDER BY created_at DESC LIMIT $1 OFFSET $2",
                )
                .bind(per_page)
                .bind(pagination::offset(page, per_page))
                .fetch_all(state.db.pool())
                .await
            }
        }
    };

    let orders = match orders {
        Ok(o) => o,
        Err(e) => {
            tracing::error!("WC orders list: {}", e);
            return super::super::woocommerce_error(
                500,
                "internal_server_error",
                "Internal server error.",
            );
        }
    };

    tracing::info!(
        "[WC] Orders query: status={:?}, after={:?}, modified_after={:?}, page={}, per_page={} → {} orders returned",
        params.status, params.after, params.modified_after, page, per_page, orders.len()
    );

    // Get total count
    let total: i64 = if customer_id.is_some() {
        if let Some(ref status) = rcommerce_status {
            if let Some(after) = effective_after {
                sqlx::query_scalar("SELECT COUNT(*) FROM orders WHERE customer_id = $1 AND status = $2::order_status AND created_at >= $3")
                    .bind(customer_id)
                    .bind(status)
                    .bind(after)
                    .fetch_one(state.db.pool())
                    .await
                    .unwrap_or(0)
            } else {
                sqlx::query_scalar("SELECT COUNT(*) FROM orders WHERE customer_id = $1 AND status = $2::order_status")
                    .bind(customer_id)
                    .bind(status)
                    .fetch_one(state.db.pool())
                    .await
                    .unwrap_or(0)
            }
        } else {
            if let Some(after) = effective_after {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM orders WHERE customer_id = $1 AND created_at >= $2",
                )
                .bind(customer_id)
                .bind(after)
                .fetch_one(state.db.pool())
                .await
                .unwrap_or(0)
            } else {
                sqlx::query_scalar("SELECT COUNT(*) FROM orders WHERE customer_id = $1")
                    .bind(customer_id)
                    .fetch_one(state.db.pool())
                    .await
                    .unwrap_or(0)
            }
        }
    } else {
        if let Some(ref status) = rcommerce_status {
            if let Some(after) = effective_after {
                sqlx::query_scalar("SELECT COUNT(*) FROM orders WHERE status = $1::order_status AND created_at >= $2")
                    .bind(status)
                    .bind(after)
                    .fetch_one(state.db.pool())
                    .await
                    .unwrap_or(0)
            } else {
                sqlx::query_scalar("SELECT COUNT(*) FROM orders WHERE status = $1::order_status")
                    .bind(status)
                    .fetch_one(state.db.pool())
                    .await
                    .unwrap_or(0)
            }
        } else {
            if let Some(after) = effective_after {
                sqlx::query_scalar("SELECT COUNT(*) FROM orders WHERE created_at >= $1")
                    .bind(after)
                    .fetch_one(state.db.pool())
                    .await
                    .unwrap_or(0)
            } else {
                sqlx::query_scalar("SELECT COUNT(*) FROM orders")
                    .fetch_one(state.db.pool())
                    .await
                    .unwrap_or(0)
            }
        }
    };

    let mut wc_orders = Vec::new();
    for order in &orders {
        let items = sqlx::query_as::<_, rcommerce_core::order::OrderItem>(
            "SELECT * FROM order_items WHERE order_id = $1",
        )
        .bind(order.id)
        .fetch_all(state.db.pool())
        .await
        .unwrap_or_default();

        wc_orders.push(map_order(order, &items, state.db.pool()).await);
    }

    let mut response = Json(serde_json::to_value(&wc_orders).unwrap_or_default()).into_response();
    let headers = pagination::pagination_headers(
        total,
        page,
        per_page,
        "https://example.com/wp-json/wc/v3/orders",
    );
    response.headers_mut().extend(headers);
    response
}

/// Resolve an order identifier from the WC API path: either a raw UUID or the
/// WC-compatible integer id (`i64::from_be_bytes(first 8 bytes of the UUID)`)
/// that `map_order` exposes in list responses — Dianxiaomi only knows the
/// integer form.
async fn resolve_order_id(
    state: &AppState,
    id: &str,
) -> Result<uuid::Uuid, axum::response::Response> {
    if let Ok(uuid) = uuid::Uuid::parse_str(id) {
        return Ok(uuid);
    }
    if let Ok(int_id) = id.parse::<i64>() {
        let lookup: Option<(uuid::Uuid,)> = sqlx::query_as(
            // map_order exposes unsigned_abs(i64) — match with abs() since
            // high-bit UUIDs produce negative bit(64)::bigint values
            r#"SELECT id FROM orders
               WHERE abs(('x' || encode(substring(uuid_send(id) from 1 for 8), 'hex'))::bit(64)::bigint) = $1"#,
        )
        .bind(int_id)
        .fetch_optional(state.db.pool())
        .await
        .unwrap_or(None);
        if let Some((uuid,)) = lookup {
            return Ok(uuid);
        }
    }
    Err(super::super::woocommerce_error(
        400,
        "woocommerce_rest_invalid_id",
        "Invalid order ID.",
    ))
}

/// Extract tracking details from a Dianxiaomi shipment note (the last note
/// line in `orders.notes`). Returns (tracking_number, provider, link).
fn extract_tracking_from_note(note: &str) -> (String, Option<String>, Option<String>) {
    // Prefer the 17track URL parameter: ...t.17track.net/#nums=<code>...
    let link = extract_17track_url(note);
    let tracking_number = link
        .as_deref()
        .and_then(|url| url.split("#nums=").nth(1))
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect::<String>()
        })
        .filter(|s| s.len() >= 4)
        .or_else(|| {
            // Fallback: longest alphanumeric run of length >= 8
            let mut best = String::new();
            let mut run = String::new();
            for c in note.chars() {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    run.push(c);
                    if run.len() > best.len() {
                        best = run.clone();
                    }
                } else {
                    run.clear();
                }
            }
            (best.len() >= 8).then_some(best)
        });

    let provider = extract_shipped_by_provider(note);

    (tracking_number.unwrap_or_default(), provider, link)
}

/// Full http(s) URL of a 17track link, if present
fn extract_17track_url(note: &str) -> Option<String> {
    let marker = "t.17track.net/#nums=";
    let idx = note.find(marker)?;
    let start = idx.saturating_sub("https://".len());
    let url = &note[start..];
    let end = url
        .find(|c: char| c.is_whitespace() || c == '"' || c == '<')
        .unwrap_or(url.len());
    let url = &url[..end];
    if url.starts_with("http://") || url.starts_with("https://") {
        Some(url.to_string())
    } else {
        None
    }
}

/// Provider name after "shipped by ", up to the next `.`/`<`/whitespace,
/// title-cased (yunexpress -> YunExpress)
fn extract_shipped_by_provider(note: &str) -> Option<String> {
    let rest = note.split("shipped by ").nth(1)?;
    let raw: String = rest
        .chars()
        .take_while(|c| !c.is_whitespace() && *c != '.' && *c != ',' && *c != '<')
        .collect();
    if raw.is_empty() {
        return None;
    }
    Some(raw[..1].to_uppercase() + &raw[1..])
}

/// Last non-empty line of the order notes column
fn last_note_line(notes: &str) -> &str {
    notes
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
}

/// Assemble tracking details and send the customer "order shipped" email.
/// Prefers a `shipment_tracking` row (the WC trackings endpoint); otherwise
/// parses the last note line Dianxiaomi appended via /orders/:id/NOTES.
async fn notify_customer_shipped(state: &AppState, order_id: uuid::Uuid) {
    // shipment_tracking stores the WC integer order id
    let tracking_row: Option<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT tracking_number, tracking_provider, tracking_link FROM shipment_tracking \
         WHERE order_id = abs(('x' || encode(substring(uuid_send($1::uuid) from 1 for 8), 'hex'))::bit(64)::bigint) \
         ORDER BY date_shipped DESC NULLS LAST, id DESC LIMIT 1",
    )
    .bind(order_id)
    .fetch_optional(state.db.pool())
    .await
    .unwrap_or(None);

    let (tracking_number, provider, link) = match tracking_row {
        Some((Some(number), provider, link)) => (number, provider, link),
        _ => {
            let notes: Option<String> =
                sqlx::query_scalar("SELECT notes FROM orders WHERE id = $1")
                    .bind(order_id)
                    .fetch_optional(state.db.pool())
                    .await
                    .ok()
                    .flatten()
                    .flatten();
            let (number, provider, link) = notes
                .as_deref()
                .map(last_note_line)
                .map(extract_tracking_from_note)
                .unwrap_or_default();
            if number.is_empty() {
                // Placeholder so the email builder gets a non-empty number
                let order_number: Option<String> =
                    sqlx::query_scalar("SELECT order_number FROM orders WHERE id = $1")
                        .bind(order_id)
                        .fetch_optional(state.db.pool())
                        .await
                        .ok()
                        .flatten();
                (
                    order_number.unwrap_or_else(|| order_id.to_string()),
                    provider,
                    None,
                )
            } else {
                (number, provider, link)
            }
        }
    };

    super::super::email_helpers::send_order_shipped_email(
        state,
        order_id,
        &tracking_number,
        provider.as_deref(),
        link.as_deref(),
    )
    .await;
}

/// POST /wp-json/wc/v3/orders/:id/notes — Dianxiaomi posts a shipment note
/// when marking an order shipped. Notes are appended to the orders.notes
/// text column, one `[timestamp] text` line per note.
pub async fn create_order_note(
    State(state): State<AppState>,
    Extension(_auth): Extension<ApiKeyAuth>,
    Path(id): Path<String>,
    Json(body): Json<CreateOrderNoteRequest>,
) -> Response {
    let order_id = match resolve_order_id(&state, &id).await {
        Ok(uuid) => uuid,
        Err(response) => return response,
    };
    if body.note.trim().is_empty() {
        return super::super::woocommerce_error(
            400,
            "woocommerce_rest_invalid_note",
            "Note content is required.",
        );
    }

    let existing: Option<String> = sqlx::query_scalar("SELECT notes FROM orders WHERE id = $1")
        .bind(order_id)
        .fetch_optional(state.db.pool())
        .await
        .unwrap_or(None)
        .flatten();
    let note_id = existing
        .as_deref()
        .map(str::lines)
        .map(|mut l| l.filter(|line| !line.trim().is_empty()).count() as i64)
        .unwrap_or(0)
        + 1;

    let now = chrono::Utc::now();
    let line = format!(
        "[{}] {}",
        now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        body.note.trim()
    );
    let result = sqlx::query(
        "UPDATE orders SET notes = CASE             WHEN notes IS NULL OR notes = '' THEN $2             ELSE notes || E'\n' || $2 END,          updated_at = NOW() WHERE id = $1",
    )
    .bind(order_id)
    .bind(&line)
    .execute(state.db.pool())
    .await;
    if let Err(e) = result {
        tracing::error!("WC order note: {}", e);
        return super::super::woocommerce_error(
            500,
            "internal_server_error",
            "Internal server error.",
        );
    }

    axum::http::Response::builder()
        .status(axum::http::StatusCode::CREATED)
        .header("content-type", "application/json; charset=UTF-8")
        .body(axum::body::Body::from(
            serde_json::json!({
                "id": note_id,
                "note": body.note.trim(),
                "date_created": now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            })
            .to_string(),
        ))
        .unwrap()
}

/// GET /wp-json/wc/v3/orders/:id
pub async fn get_order(
    State(state): State<AppState>,
    Extension(auth): Extension<ApiKeyAuth>,
    Path(id): Path<String>,
) -> Response {
    let order_id = match resolve_order_id(&state, &id).await {
        Ok(uuid) => uuid,
        Err(response) => return response,
    };

    let order =
        match sqlx::query_as::<_, rcommerce_core::models::Order>("SELECT id, order_number, customer_id, customer_email, currency::text AS currency_text, order_type, subtotal, tax_total, shipping_total, discount_total, total, status, payment_status, fulfillment_status, billing_address_id, shipping_address_id, payment_method, shipping_method, notes, tags, draft, subscription_id, billing_cycle, coupon_code, coupon_id, ip_address, user_agent, metadata, shipping_address, billing_address, created_at, updated_at, completed_at, cancelled_at FROM orders WHERE id = $1")
            .bind(order_id)
            .fetch_optional(state.db.pool())
            .await
        {
            Ok(Some(o)) => o,
            Ok(None) => {
                return super::super::woocommerce_error(
                    404,
                    "woocommerce_rest_invalid_id",
                    "Invalid ID.",
                );
            }
            Err(e) => {
                tracing::error!("WC order get: {}", e);
                return super::super::woocommerce_error(500, "internal_server_error", "Internal server error.");
            }
        };

    // Check ownership (non-admin can only see their own orders)
    if let Some(cid) = auth.customer_id {
        if order.customer_id != Some(cid) && !auth.is_admin() {
            return super::super::woocommerce_error(
                403,
                "woocommerce_rest_cannot_view",
                "Sorry, you cannot view this resource.",
            );
        }
    }

    let items = sqlx::query_as::<_, rcommerce_core::order::OrderItem>(
        "SELECT * FROM order_items WHERE order_id = $1",
    )
    .bind(order.id)
    .fetch_all(state.db.pool())
    .await
    .unwrap_or_default();

    let wc = map_order(&order, &items, state.db.pool()).await;
    Json(serde_json::to_value(&wc).unwrap_or_default()).into_response()
}

/// PUT /wp-json/wc/v3/orders/:id
pub async fn update_order(
    State(state): State<AppState>,
    Extension(auth): Extension<ApiKeyAuth>,
    Path(id): Path<String>,
    Json(body): Json<UpdateOrderRequest>,
) -> Response {
    let order_id = match resolve_order_id(&state, &id).await {
        Ok(uuid) => uuid,
        Err(response) => return response,
    };

    if let Some(new_status) = body.status {
        let rcommerce_status = unmap_order_status(&new_status);
        // Woo "completed" means shipped; "processing" means being prepared
        let fulfillment_status = fulfillment_for_wc_status(&new_status).map(str::to_string);

        // Transition detection for the shipped notification: only email when
        // the order was not already shipped. Read-only; does not change the
        // UPDATE semantics below.
        let was_shipped: bool = sqlx::query_scalar::<_, Option<String>>(
            "SELECT fulfillment_status::text FROM orders WHERE id = $1",
        )
        .bind(order_id)
        .fetch_optional(state.db.pool())
        .await
        .ok()
        .flatten()
        .flatten()
        .map(|s| s == "shipped")
        .unwrap_or(false);

        let result = sqlx::query(
            "UPDATE orders SET status = $1::order_status, \
             fulfillment_status = COALESCE($2::fulfillment_status, fulfillment_status), \
             updated_at = NOW() WHERE id = $3",
        )
        .bind(&rcommerce_status)
        .bind(&fulfillment_status)
        .bind(order_id)
        .execute(state.db.pool())
        .await;

        match result {
            Ok(_) => {}
            Err(e) => {
                tracing::error!("WC order update: {}", e);
                return super::super::woocommerce_error(
                    500,
                    "internal_server_error",
                    "Internal server error.",
                );
            }
        }

        if fulfillment_status.as_deref() == Some("shipped") && !was_shipped {
            notify_customer_shipped(&state, order_id).await;
        }
    }

    // Return updated order
    get_order(State(state), Extension(auth), Path(order_id.to_string())).await
}

#[cfg(test)]
mod line_item_tests {
    use super::*;

    fn sample_order_item(variant_id: Option<uuid::Uuid>) -> rcommerce_core::order::OrderItem {
        rcommerce_core::order::OrderItem {
            id: uuid::Uuid::new_v4(),
            order_id: uuid::Uuid::new_v4(),
            product_id: uuid::Uuid::new_v4(),
            variant_id,
            quantity: 3,
            price: Decimal::new(7500, 2),
            subtotal: Decimal::new(22500, 2),
            tax_amount: Decimal::ZERO,
            total: Decimal::new(22500, 2),
            sku: Some("CALIBER3-50".to_string()),
            name: "Caliber 3 Precision Baseplate".to_string(),
            variant_name: Some("Degree: 50".to_string()),
            weight: None,
            weight_unit: None,
            is_bundle_component: None,
            metadata: serde_json::json!({}),
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_variant_line_item_exposes_variation_id_and_option_meta() {
        let variant_id = uuid::Uuid::new_v4();
        let item = sample_order_item(Some(variant_id));
        let options = vec![("Degree".to_string(), "50".to_string())];

        let line = build_wc_line_item(&item, None, None, Some(&options));

        assert_eq!(
            line.variation_id,
            uuid_to_wc_int(variant_id),
            "variation_id must be the deterministic int of the variant uuid"
        );
        assert!(
            line.variation_id != 0,
            "variation_id must be non-zero for variant items"
        );
        let meta = &line.meta_data;
        assert!(
            meta.iter()
                .any(|m| m["key"] == "Degree" && m["value"] == "50"),
            "meta_data must carry the option name/value pair, got: {meta:?}"
        );
        assert_eq!(line.name, "Caliber 3 Precision Baseplate");
        assert_eq!(line.quantity, 3);
    }

    #[test]
    fn test_simple_line_item_has_zero_variation_and_no_option_meta() {
        let item = sample_order_item(None);

        let line = build_wc_line_item(&item, None, None, None);

        assert_eq!(line.variation_id, 0);
        assert!(
            line.meta_data
                .iter()
                .all(|m| m["key"].as_str().unwrap_or("").starts_with('_')),
            "no option meta may leak for simple items, got: {:?}",
            line.meta_data
        );
    }

    #[test]
    fn test_variant_options_fold_groups_by_variant() {
        // Mirrors the fold in map_order: rows for two variants must land in
        // separate buckets keyed by variant_id.
        let v1 = uuid::Uuid::new_v4();
        let v2 = uuid::Uuid::new_v4();
        let rows = vec![
            (v1, "Degree".to_string(), "50".to_string()),
            (v1, "Color".to_string(), "Orange".to_string()),
            (v2, "Degree".to_string(), "30".to_string()),
        ];
        let map: std::collections::HashMap<uuid::Uuid, Vec<(String, String)>> = rows
            .into_iter()
            .fold(std::collections::HashMap::new(), |mut map, (vid, name, value)| {
                map.entry(vid).or_insert_with(Vec::new).push((name, value));
                map
            });
        assert_eq!(map[&v1].len(), 2);
        assert_eq!(map[&v2].len(), 1);
    }
}

#[cfg(test)]
mod note_extraction_tests {
    use super::*;

    const DXM_NOTE: &str = "[2026-10-12T03:54:35Z] <p>Your order has been shipped by yunexpress. The tracking number is <span class=\"x\">YT2628500707105752</span>. Track it <a href=\"https://t.17track.net/#nums=YT2628500707105752\">here</a></p>";

    #[test]
    fn test_extract_tracking_prefers_17track_url() {
        let (number, provider, link) = extract_tracking_from_note(DXM_NOTE);
        assert_eq!(number, "YT2628500707105752");
        assert_eq!(provider.as_deref(), Some("Yunexpress"));
        assert_eq!(
            link.as_deref(),
            Some("https://t.17track.net/#nums=YT2628500707105752")
        );
    }

    #[test]
    fn test_extract_tracking_fallback_run_and_no_link() {
        let (number, provider, link) =
            extract_tracking_from_note("Package AB123456789CD shipped by dhl, enjoy!");
        assert_eq!(number, "AB123456789CD");
        assert_eq!(provider.as_deref(), Some("Dhl"));
        assert_eq!(link, None);
    }

    #[test]
    fn test_extract_tracking_no_tracking_anywhere() {
        let (number, provider, link) = extract_tracking_from_note("order packed and ready");
        assert_eq!(number, "");
        assert_eq!(provider, None);
        assert_eq!(link, None);
    }

    #[test]
    fn test_last_note_line_skips_blank_tail() {
        assert_eq!(last_note_line("a\n\nb\n\n"), "b");
        assert_eq!(last_note_line(""), "");
    }
}
