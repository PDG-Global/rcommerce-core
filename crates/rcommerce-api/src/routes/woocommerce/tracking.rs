//! WooCommerce Shipment Tracking API
//!
//! Compatible with the WooCommerce Shipment Tracking plugin:
//! https://woocommerce.com/document/shipment-tracking/shipment-tracking-rest-api/
//!
//! POST   /wp-json/wc/v3/orders/{order_id}/shipment-trackings
//! GET    /wp-json/wc/v3/orders/{order_id}/shipment-trackings
//! PUT    /wp-json/wc/v3/orders/{order_id}/shipment-trackings/{id}

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};

use crate::middleware::api_key_auth::ApiKeyAuth;
use crate::state::AppState;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ShipmentTracking {
    pub id: uuid::Uuid,
    pub order_id: i64,
    pub tracking_number: String,
    pub tracking_provider: Option<String>,
    pub tracking_link: Option<String>,
    pub date_shipped: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTrackingRequest {
    pub tracking_number: String,
    pub tracking_provider: Option<String>,
    pub tracking_link: Option<String>,
    pub date_shipped: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTrackingRequest {
    pub tracking_number: Option<String>,
    pub tracking_provider: Option<String>,
    pub tracking_link: Option<String>,
}

impl ShipmentTracking {
    fn to_wc_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id.to_string(),
            "tracking_number": self.tracking_number,
            "tracking_provider": self.tracking_provider.as_deref().unwrap_or(""),
            "tracking_link": self.tracking_link.as_deref().unwrap_or(""),
            "date_shipped": self.date_shipped.map(|d| d.to_rfc3339()),
            "date_shipped_gmt": self.date_shipped.map(|d| d.to_rfc3339().replace("+00:00", "Z")),
        })
    }
}

/// POST /wp-json/wc/v3/orders/:order_id/shipment-trackings
pub async fn create_tracking(
    State(state): State<AppState>,
    Extension(_auth): Extension<ApiKeyAuth>,
    Path(order_id): Path<String>,
    Json(body): Json<CreateTrackingRequest>,
) -> Response {
    let order_id_int: i64 = match order_id.parse() {
        Ok(n) => n,
        Err(_) => {
            return super::super::woocommerce_error(
                400,
                "woocommerce_rest_invalid_id",
                "Invalid order ID.",
            )
        }
    };

    let tracking_id = uuid::Uuid::new_v4();
    let date_shipped = body
        .date_shipped
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    match sqlx::query_as::<_, ShipmentTracking>(
        r#"INSERT INTO shipment_tracking (id, order_id, tracking_number, tracking_provider, tracking_link, date_shipped)
           VALUES ($1, $2, $3, $4, $5, $6) RETURNING *"#,
    )
    .bind(tracking_id)
    .bind(order_id_int)
    .bind(&body.tracking_number)
    .bind(&body.tracking_provider)
    .bind(&body.tracking_link)
    .bind(date_shipped)
    .fetch_one(state.db.pool())
    .await
    {
        Ok(record) => {
            tracing::info!("[WC] Tracking created for order {}: {}", order_id, body.tracking_number);

            // Resolve the WC-compatible integer id back to the order (the WC API
            // exposes i64::from_be_bytes(first 8 bytes of the order UUID); see
            // woocommerce/orders.rs map_order), then mark the order shipped and
            // notify the customer.
            let order_lookup: Option<(uuid::Uuid,)> = sqlx::query_as(
                // abs(): map_order exposes unsigned_abs(i64); high-bit UUIDs
                // produce negative bit(64)::bigint values
                r#"SELECT id FROM orders
                   WHERE abs(('x' || encode(substring(uuid_send(id) from 1 for 8), 'hex'))::bit(64)::bigint) = $1"#,
            )
            .bind(order_id_int)
            .fetch_optional(state.db.pool())
            .await
            .unwrap_or(None);

            if let Some((order_uuid,)) = order_lookup {
                let updated = sqlx::query(
                    "UPDATE orders SET fulfillment_status = 'shipped', updated_at = NOW() \
                     WHERE id = $1 AND fulfillment_status IN ('pending', 'processing')",
                )
                .bind(order_uuid)
                .execute(state.db.pool())
                .await;
                if let Err(e) = updated {
                    tracing::warn!("[WC] Failed to mark order {} shipped: {}", order_uuid, e);
                }
                crate::routes::email_helpers::send_order_shipped_email(
                    &state,
                    order_uuid,
                    &body.tracking_number,
                    body.tracking_provider.as_deref(),
                    body.tracking_link.as_deref(),
                )
                .await;
            }

            Json(record.to_wc_json()).into_response()
        }
        Err(e) => {
            tracing::error!("[WC] Failed to create tracking: {}", e);
            super::super::woocommerce_error(500, "internal_server_error", "Failed to create tracking.")
        }
    }
}

/// GET /wp-json/wc/v3/orders/:order_id/shipment-trackings
pub async fn list_trackings(
    State(state): State<AppState>,
    Extension(_auth): Extension<ApiKeyAuth>,
    Path(order_id): Path<String>,
) -> Response {
    let order_id_int: i64 = match order_id.parse() {
        Ok(n) => n,
        Err(_) => {
            return super::super::woocommerce_error(
                400,
                "woocommerce_rest_invalid_id",
                "Invalid order ID.",
            )
        }
    };

    match sqlx::query_as::<_, ShipmentTracking>(
        "SELECT * FROM shipment_tracking WHERE order_id = $1 ORDER BY created_at DESC",
    )
    .bind(order_id_int)
    .fetch_all(state.db.pool())
    .await
    {
        Ok(records) => {
            let json: Vec<_> = records.iter().map(|r| r.to_wc_json()).collect();
            Json(json).into_response()
        }
        Err(e) => {
            tracing::error!("[WC] Failed to list trackings: {}", e);
            super::super::woocommerce_error(
                500,
                "internal_server_error",
                "Failed to list trackings.",
            )
        }
    }
}

/// PUT /wp-json/wc/v3/orders/:order_id/shipment-trackings/:id
pub async fn update_tracking(
    State(state): State<AppState>,
    Extension(_auth): Extension<ApiKeyAuth>,
    Path((order_id, tracking_id)): Path<(String, String)>,
    Json(body): Json<UpdateTrackingRequest>,
) -> Response {
    let tracking_uuid = match uuid::Uuid::parse_str(&tracking_id) {
        Ok(u) => u,
        Err(_) => {
            return super::super::woocommerce_error(
                400,
                "woocommerce_rest_invalid_id",
                "Invalid tracking ID.",
            )
        }
    };

    let order_id_int: i64 = match order_id.parse() {
        Ok(n) => n,
        Err(_) => {
            return super::super::woocommerce_error(
                400,
                "woocommerce_rest_invalid_id",
                "Invalid order ID.",
            )
        }
    };

    // Fetch existing
    let existing = match sqlx::query_as::<_, ShipmentTracking>(
        "SELECT * FROM shipment_tracking WHERE id = $1 AND order_id = $2",
    )
    .bind(tracking_uuid)
    .bind(order_id_int)
    .fetch_optional(state.db.pool())
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("[WC] Failed to fetch tracking: {}", e);
            return super::super::woocommerce_error(
                500,
                "internal_server_error",
                "Failed to fetch tracking.",
            );
        }
    };

    let mut record = match existing {
        Some(r) => r,
        None => {
            return super::super::woocommerce_error(
                404,
                "woocommerce_rest_invalid_id",
                "Tracking not found.",
            )
        }
    };

    // Apply updates
    if let Some(tn) = body.tracking_number {
        record.tracking_number = tn;
    }
    if let Some(tp) = body.tracking_provider {
        record.tracking_provider = Some(tp);
    }
    if let Some(tl) = body.tracking_link {
        record.tracking_link = Some(tl);
    }

    match sqlx::query(
        "UPDATE shipment_tracking SET tracking_number=$1, tracking_provider=$2, tracking_link=$3, updated_at=NOW() WHERE id=$4",
    )
    .bind(&record.tracking_number)
    .bind(&record.tracking_provider)
    .bind(&record.tracking_link)
    .bind(tracking_uuid)
    .execute(state.db.pool())
    .await
    {
        Ok(_) => {
            tracing::info!("[WC] Tracking {} updated for order {}", tracking_id, order_id);
            Json(record.to_wc_json()).into_response()
        }
        Err(e) => {
            tracing::error!("[WC] Failed to update tracking: {}", e);
            super::super::woocommerce_error(500, "internal_server_error", "Failed to update tracking.")
        }
    }
}
