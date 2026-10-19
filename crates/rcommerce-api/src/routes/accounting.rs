//! Accounting export API (see docs/api/05-accounting-export-api.md).
//!
//! `GET /api/v1/accounting/orders` — reconciliation-ready order/payment/
//! refund export for accounting automation (`accounty`). Authenticated with
//! an API key scoped `exports:read` (or the `read` wildcard).
//!
//! `POST /api/v1/accounting/sync-settlements` — pulls Airwallex settlement
//! records to backfill fees and settlement batch ids onto payments.
//! Requires `exports:write` (or `write`/`admin`).

use axum::{
    body::Body,
    extract::{Query, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Extension, Json, Router,
};
use base64::Engine;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::middleware::{api_key_auth_middleware, ApiKeyAuth};
use crate::state::AppState;
use rcommerce_core::repository::PostgresApiKeyRepository;
use rcommerce_core::services::{Action, Resource};

pub fn accounting_router() -> Router<AppState> {
    Router::new()
        .route("/accounting/orders", axum::routing::get(export_orders))
        .route(
            "/accounting/sync-settlements",
            axum::routing::post(sync_settlements),
        )
}

/// Injects the API key repository and validates the bearer API key.
/// Route handlers additionally check the required scope.
pub async fn accounting_auth_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    api_key_auth_middleware(
        Extension(state.api_key_repository.clone() as Arc<PostgresApiKeyRepository>),
        request,
        next,
    )
    .await
}

fn require_scope(auth: &ApiKeyAuth, action: Action) -> Result<(), Response> {
    let ok = match action {
        Action::Read => auth.can_read(Resource::Exports),
        Action::Write => auth.can_write(Resource::Exports),
        Action::Admin => auth.is_admin(),
    };
    if ok {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "API key lacks the required exports scope"
            })),
        )
            .into_response())
    }
}

// ---------------------------------------------------------------------------
// GET /api/v1/accounting/orders
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    /// Inclusive lower bound on updated_at (incremental pulls re-emit
    /// status changes like pending→settled and new refunds).
    pub since: Option<String>,
    /// Inclusive upper bound on updated_at.
    pub until: Option<String>,
    /// Opaque cursor from `next_cursor`.
    pub cursor: Option<String>,
    /// all (default) | only | exclude — test-mode orders (spec 2.2).
    #[serde(default)]
    pub test: Option<String>,
    /// Filter payment attempts by status, e.g. `succeeded` to drop failed
    /// attempts from the payments array.
    pub payment_status: Option<String>,
    /// Page size (default 200, max 1000).
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct ExportResponse {
    data: Vec<ExportOrder>,
    next_cursor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExportOrder {
    order_id: String,
    store: String,
    test: bool,
    status: String,
    order_status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    currency: String,
    total: Decimal,
    customer: ExportCustomer,
    items: Vec<ExportItem>,
    payments: Vec<ExportPayment>,
    refunds: Vec<ExportRefund>,
}

#[derive(Debug, Serialize)]
pub struct ExportCustomer {
    email: String,
    name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExportItem {
    sku: Option<String>,
    name: String,
    qty: i32,
    unit_price: Decimal,
}

#[derive(Debug, Serialize)]
pub struct ExportPayment {
    attempt_id: Option<String>,
    intent_id: Option<String>,
    provider: String,
    method: Option<String>,
    status: String,
    amount: Decimal,
    currency: String,
    captured_at: Option<DateTime<Utc>>,
    fee: Option<ExportFee>,
    net: Option<Decimal>,
    settlement_id: Option<String>,
    settled_at: Option<DateTime<Utc>>,
    settlement_currency: Option<String>,
    fx_rate: Option<Decimal>,
    /// True for imported historical payments where no provider fee/settlement
    /// data is available — distinguishes "legacy, unknown" from "not yet
    /// settled" (accounting feedback P3).
    legacy: bool,
}

#[derive(Debug, Serialize)]
pub struct ExportFee {
    total: Decimal,
    components: Vec<ExportFeeComponent>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportFeeComponent {
    #[serde(rename = "type")]
    kind: String,
    amount: Decimal,
}

#[derive(Debug, Serialize)]
pub struct ExportRefund {
    refund_id: Option<String>,
    attempt_id: Option<String>,
    amount: Decimal,
    currency: String,
    status: String,
    reason: Option<String>,
    created_at: DateTime<Utc>,
    settled_at: Option<DateTime<Utc>>,
}

/// Cursor payload: keyset on (updated_at, id) for deterministic ordering.
#[derive(Debug, Serialize, Deserialize)]
struct Cursor {
    u: DateTime<Utc>,
    i: Uuid,
}

fn encode_cursor(c: &Cursor) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(c).unwrap_or_default())
}

fn decode_cursor(s: &str) -> Option<Cursor> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn map_payment_status(status: &str) -> &'static str {
    match status {
        "paid" => "succeeded",
        "authorized" => "pending",
        other => match other {
            "pending" => "pending",
            "failed" => "failed",
            "cancelled" => "cancelled",
            "refunded" => "succeeded",
            _ => "pending",
        },
    }
}

fn map_refund_status(status: &str) -> &'static str {
    match status {
        "refunded" | "paid" => "succeeded",
        "failed" | "cancelled" => "failed",
        _ => "pending",
    }
}

/// Payment attempt status filter → stored payment_status value.
fn payment_status_filter(value: &str) -> Option<&'static str> {
    match value {
        "succeeded" => Some("paid"),
        "pending" => Some("pending"),
        "failed" => Some("failed"),
        "cancelled" => Some("cancelled"),
        "refunded" => Some("refunded"),
        _ => None,
    }
}

fn parse_time(value: &str, param: &str) -> Result<Option<DateTime<Utc>>, Response> {
    match DateTime::parse_from_rfc3339(value) {
        Ok(dt) => Ok(Some(dt.with_timezone(&Utc))),
        Err(_) => Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("invalid {} timestamp: {}", param, value)})),
        )
            .into_response()),
    }
}

pub async fn export_orders(
    State(state): State<AppState>,
    Extension(auth): Extension<ApiKeyAuth>,
    Query(query): Query<ExportQuery>,
) -> Result<Json<ExportResponse>, Response> {
    require_scope(&auth, Action::Read)?;

    let since = match query.since.as_deref() {
        Some(v) => parse_time(v, "since")?,
        None => None,
    };
    let until = match query.until.as_deref() {
        Some(v) => parse_time(v, "until")?,
        None => None,
    };
    let test_mode = query.test.as_deref().unwrap_or("all");
    if !matches!(test_mode, "all" | "only" | "exclude") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "test must be all|only|exclude"})),
        )
            .into_response());
    }
    let payment_filter: Option<&'static str> = match query.payment_status.as_deref() {
        None => None,
        Some(v) => Some(payment_status_filter(v).ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "payment_status must be pending|succeeded|failed|cancelled|refunded"})),
            )
                .into_response()
        })?),
    };
    let limit = query.limit.unwrap_or(200).clamp(1, 1000);
    let cursor = match query.cursor.as_deref() {
        Some(c) => Some(decode_cursor(c).ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "invalid cursor"})),
            )
                .into_response()
        })?),
        None => None,
    };

    // Page orders by keyset on (updated_at, id). Fetch one extra row to
    // know whether a next page exists.
    let orders = sqlx::query_as::<_, ExportOrderRow>(
        r#"
        SELECT o.id, COALESCE(o.invoice_number, o.order_number) AS reference,
               o.order_number, o.test,
               o.payment_status::text AS payment_status,
               o.status::text AS order_status,
               o.created_at, o.updated_at, o.currency::text AS currency, o.total,
               o.customer_email, o.shipping_address,
               c.first_name AS customer_first_name, c.last_name AS customer_last_name
        FROM orders o
        LEFT JOIN customers c ON c.id = o.customer_id
        WHERE o.draft = false
          AND ($1::timestamptz IS NULL OR o.updated_at >= $1)
          AND ($2::timestamptz IS NULL OR o.updated_at <= $2)
          AND ($3::text = 'all' OR ($3::text = 'only' AND o.test) OR ($3::text = 'exclude' AND NOT o.test))
          AND ($4::timestamptz IS NULL OR (o.updated_at, o.id) > ($4, $5::uuid))
        ORDER BY o.updated_at ASC, o.id ASC
        LIMIT $6
        "#,
    )
    .bind(since)
    .bind(until)
    .bind(test_mode)
    .bind(cursor.as_ref().map(|c| c.u))
    .bind(cursor.as_ref().map(|c| c.i))
    .bind((limit + 1) as i64)
    .fetch_all(state.db.pool())
    .await
    .map_err(internal_error)?;

    let has_more = orders.len() > limit;
    let page: Vec<&ExportOrderRow> = orders.iter().take(limit).collect();
    let ids: Vec<Uuid> = page.iter().map(|o| o.id).collect();

    let next_cursor = if has_more {
        page.last().map(|o| {
            encode_cursor(&Cursor {
                u: o.updated_at,
                i: o.id,
            })
        })
    } else {
        None
    };

    // Batch-fetch related records for the page.
    let payments = sqlx::query_as::<_, PaymentRow>(
        r#"
        SELECT id, order_id, amount, currency::text AS currency, status::text AS status,
               gateway, gateway_payment_id, payment_method::text AS payment_method,
               card_last_four, card_brand, processed_at, created_at,
               transaction_id, fee_amount, fee_currency, fee_components,
               settlement_id, settled_at, settlement_currency, fx_rate,
               COALESCE(legacy, false) AS legacy
        FROM payments
        WHERE order_id = ANY($1)
        ORDER BY created_at, id
        "#,
    )
    .bind(&ids)
    .fetch_all(state.db.pool())
    .await
    .map_err(internal_error)?;

    let refunds = sqlx::query_as::<_, RefundRow>(
        r#"
        SELECT r.id, r.order_id, r.amount, r.currency::text AS currency,
               r.status::text AS status, r.reason, r.gateway_refund_id,
               r.created_at, r.settled_at,
               p.transaction_id AS parent_transaction_id
        FROM refunds r
        LEFT JOIN payments p ON p.id = r.payment_id
        WHERE r.order_id = ANY($1)
        ORDER BY r.created_at, r.id
        "#,
    )
    .bind(&ids)
    .fetch_all(state.db.pool())
    .await
    .map_err(internal_error)?;

    let items = sqlx::query_as::<_, ItemRow>(
        r#"
        SELECT id, order_id, sku, title, quantity, price
        FROM order_items
        WHERE order_id = ANY($1)
        ORDER BY created_at, id
        "#,
    )
    .bind(&ids)
    .fetch_all(state.db.pool())
    .await
    .map_err(internal_error)?;

    let store = state
        .accounting
        .store_name
        .clone()
        .unwrap_or_else(|| state.branding.name.clone());

    let data = page
        .into_iter()
        .map(|order| {
            let name = customer_name(order);
            ExportOrder {
                order_id: order.reference.clone(),
                store: store.clone(),
                test: order.test,
                status: order.payment_status.clone(),
                order_status: order.order_status.clone(),
                created_at: order.created_at,
                updated_at: order.updated_at,
                currency: order.currency.clone(),
                total: order.total,
                customer: ExportCustomer {
                    email: order.customer_email.clone(),
                    name,
                },
                items: items
                    .iter()
                    .filter(|i| i.order_id == order.id)
                    .map(|i| ExportItem {
                        sku: i.sku.clone(),
                        name: i.title.clone(),
                        qty: i.quantity,
                        unit_price: i.price,
                    })
                    .collect(),
                payments: payments
                    .iter()
                    .filter(|p| p.order_id == order.id)
                    .filter(|p| payment_filter.map(|f| p.status == f).unwrap_or(true))
                    .map(|p| {
                        // Fees are booked in settlement currency; net is only
                        // meaningful when it matches the charge currency.
                        let same_currency = p
                            .settlement_currency
                            .as_deref()
                            .map(|sc| sc == p.currency)
                            .unwrap_or(true);
                        let fee = p.fee_amount.map(|total| ExportFee {
                            total,
                            components: p
                                .fee_components
                                .as_ref()
                                .and_then(|v| serde_json::from_value(v.clone()).ok())
                                .unwrap_or_default(),
                        });
                        let net = fee
                            .as_ref()
                            .filter(|_| same_currency)
                            .map(|f| p.amount - f.total);
                        ExportPayment {
                            attempt_id: p.transaction_id.clone(),
                            intent_id: p.gateway_payment_id.clone(),
                            provider: p.gateway.clone(),
                            method: p.payment_method.clone(),
                            status: map_payment_status(&p.status).to_string(),
                            amount: p.amount,
                            currency: p.currency.clone(),
                            captured_at: p.processed_at,
                            fee,
                            net,
                            settlement_id: p.settlement_id.clone(),
                            settled_at: p.settled_at,
                            settlement_currency: p.settlement_currency.clone(),
                            fx_rate: p.fx_rate,
                            legacy: p.legacy,
                        }
                    })
                    .collect(),
                refunds: refunds
                    .iter()
                    .filter(|r| r.order_id == order.id)
                    .map(|r| ExportRefund {
                        refund_id: r.gateway_refund_id.clone(),
                        attempt_id: r.parent_transaction_id.clone(),
                        amount: r.amount,
                        currency: r.currency.clone(),
                        status: map_refund_status(&r.status).to_string(),
                        reason: r.reason.clone(),
                        created_at: r.created_at,
                        settled_at: r.settled_at,
                    })
                    .collect(),
            }
        })
        .collect();

    Ok(Json(ExportResponse { data, next_cursor }))
}

fn customer_name(order: &ExportOrderRow) -> Option<String> {
    let from_customer = match (&order.customer_first_name, &order.customer_last_name) {
        (Some(f), Some(l)) => Some(format!("{} {}", f, l).trim().to_string()),
        (Some(f), None) => Some(f.clone()),
        (None, Some(l)) => Some(l.clone()),
        (None, None) => None,
    };
    from_customer
        .filter(|n| !n.is_empty())
        .or_else(|| json_string(&order.shipping_address, "name"))
        .or_else(|| json_string(&order.shipping_address, "recipient_name"))
        .or_else(|| {
            let f = json_string(&order.shipping_address, "first_name");
            let l = json_string(&order.shipping_address, "last_name");
            match (f, l) {
                (Some(f), Some(l)) => Some(format!("{} {}", f, l).trim().to_string()),
                (f, l) => f.or(l),
            }
        })
        .filter(|n| !n.is_empty())
}

fn json_string(value: &Option<serde_json::Value>, key: &str) -> Option<String> {
    value
        .as_ref()
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn internal_error(e: sqlx::Error) -> Response {
    tracing::error!("Accounting export failed: {}", e);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": "export query failed"})),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// POST /api/v1/accounting/sync-settlements
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SyncRequest {
    /// Maximum number of unsettled payment intents to query (0 = all).
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct SyncResponse {
    queried: usize,
    payments_updated: usize,
    refunds_updated: usize,
    not_settled: usize,
    unmatched: Vec<String>,
}

pub async fn sync_settlements(
    State(state): State<AppState>,
    Extension(auth): Extension<ApiKeyAuth>,
    Json(request): Json<SyncRequest>,
) -> Result<Json<SyncResponse>, Response> {
    require_scope(&auth, Action::Write)?;

    let sync = state.settlement_sync.as_ref().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Airwallex is not configured"})),
        )
            .into_response()
    })?;

    let report = sync
        .sync_pending(state.db.pool(), request.limit.unwrap_or(0))
        .await
        .map_err(|e| {
            tracing::error!("Settlement sync failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "settlement sync failed"})),
            )
                .into_response()
        })?;

    Ok(Json(SyncResponse {
        queried: report.queried,
        payments_updated: report.payments_updated,
        refunds_updated: report.refunds_updated,
        not_settled: report.not_settled,
        unmatched: report.unmatched,
    }))
}

// ---------------------------------------------------------------------------
// Row types
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct ExportOrderRow {
    id: Uuid,
    reference: String,
    #[allow(dead_code)]
    order_number: String,
    test: bool,
    payment_status: String,
    order_status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    currency: String,
    total: Decimal,
    customer_email: String,
    shipping_address: Option<serde_json::Value>,
    customer_first_name: Option<String>,
    customer_last_name: Option<String>,
}

#[derive(sqlx::FromRow)]
struct PaymentRow {
    id: Uuid,
    order_id: Uuid,
    amount: Decimal,
    currency: String,
    status: String,
    gateway: String,
    gateway_payment_id: Option<String>,
    payment_method: Option<String>,
    #[allow(dead_code)]
    card_last_four: Option<String>,
    #[allow(dead_code)]
    card_brand: Option<String>,
    processed_at: Option<DateTime<Utc>>,
    #[allow(dead_code)]
    created_at: DateTime<Utc>,
    transaction_id: Option<String>,
    fee_amount: Option<Decimal>,
    #[allow(dead_code)]
    fee_currency: Option<String>,
    fee_components: Option<serde_json::Value>,
    settlement_id: Option<String>,
    settled_at: Option<DateTime<Utc>>,
    settlement_currency: Option<String>,
    fx_rate: Option<Decimal>,
    legacy: bool,
}

#[derive(sqlx::FromRow)]
struct RefundRow {
    #[allow(dead_code)]
    id: Uuid,
    order_id: Uuid,
    amount: Decimal,
    currency: String,
    status: String,
    reason: Option<String>,
    gateway_refund_id: Option<String>,
    created_at: DateTime<Utc>,
    settled_at: Option<DateTime<Utc>>,
    parent_transaction_id: Option<String>,
}

#[derive(sqlx::FromRow)]
struct ItemRow {
    #[allow(dead_code)]
    id: Uuid,
    order_id: Uuid,
    sku: Option<String>,
    title: String,
    quantity: i32,
    price: Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_roundtrip() {
        let cursor = Cursor {
            u: Utc::now(),
            i: Uuid::new_v4(),
        };
        let encoded = encode_cursor(&cursor);
        let decoded = decode_cursor(&encoded).unwrap();
        assert_eq!(cursor.u, decoded.u);
        assert_eq!(cursor.i, decoded.i);
        assert!(decode_cursor("garbage!").is_none());
    }

    #[test]
    fn test_status_mappings() {
        assert_eq!(map_payment_status("paid"), "succeeded");
        assert_eq!(map_payment_status("refunded"), "succeeded");
        assert_eq!(map_payment_status("authorized"), "pending");
        assert_eq!(map_payment_status("failed"), "failed");
        assert_eq!(map_refund_status("refunded"), "succeeded");
        assert_eq!(map_refund_status("pending"), "pending");
        assert_eq!(map_refund_status("cancelled"), "failed");
    }

    #[test]
    fn test_payment_status_filter_alias() {
        assert_eq!(payment_status_filter("succeeded"), Some("paid"));
        assert_eq!(payment_status_filter("failed"), Some("failed"));
        assert_eq!(payment_status_filter("bogus"), None);
    }
}
