//! Download routes for digital products
//!
//! Provides endpoints for:
//! - Listing available downloads for an order
//! - Downloading files with token validation
//! - Generating download tokens

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::state::AppState;
use rcommerce_core::Error;

/// List downloads for an order
///
/// GET /api/v1/orders/:order_id/downloads
pub async fn list_order_downloads(
    State(state): State<AppState>,
    Path(order_id): Path<String>,
) -> Json<serde_json::Value> {
    let order_id = match Uuid::parse_str(&order_id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Json(serde_json::json!({
                "error": "Invalid order ID format"
            }));
        }
    };

    // This would need authentication middleware to get customer_id
    // For now, return downloads for the order
    match state
        .digital_product_service
        .get_order_downloads(order_id)
        .await
    {
        Ok(downloads) => {
            let downloads_json: Vec<serde_json::Value> = downloads
                .into_iter()
                .map(|d| {
                    serde_json::json!({
                        "id": d.id,
                        "order_item_id": d.order_item_id,
                        "download_token": d.download_token,
                        "download_count": d.download_count,
                        "download_limit": d.download_limit,
                        "expires_at": d.expires_at,
                        "created_at": d.created_at,
                        "download_url": format!("/api/v1/downloads/{}", d.download_token)
                    })
                })
                .collect();

            Json(serde_json::json!({
                "downloads": downloads_json
            }))
        }
        Err(e) => {
            tracing::error!("Failed to get order downloads: {}", e);
            Json(serde_json::json!({
                "error": "Failed to retrieve downloads"
            }))
        }
    }
}

/// Download a file using a token
///
/// GET /api/v1/downloads/:token
pub async fn download_file(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Response<Body> {
    match state.digital_product_service.record_download(&token).await {
        Ok(download_response) => {
            // For local storage, stream the file
            match state
                .file_upload_service
                .get_file_stream(&download_response.download_url)
                .await
            {
                Ok(file_data) => Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/octet-stream")
                    .header(
                        header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{}\"", download_response.file_name),
                    )
                    .header(header::CONTENT_LENGTH, file_data.len())
                    .body(Body::from(file_data))
                    .unwrap(),
                Err(e) => {
                    tracing::error!("Failed to read file: {}", e);
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("Failed to read file"))
                        .unwrap()
                }
            }
        }
        Err(Error::NotFound(_)) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Download not found"))
            .unwrap(),
        Err(Error::Validation(msg)) => Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(msg))
            .unwrap(),
        Err(e) => {
            tracing::error!("Download error: {}", e);
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from("Download failed"))
                .unwrap()
        }
    }
}

/// Get download info (without actually downloading)
///
/// GET /api/v1/downloads/:token/info
pub async fn get_download_info(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Json<serde_json::Value> {
    match state
        .digital_product_service
        .get_download_by_token(&token)
        .await
    {
        Ok(Some(download)) => Json(serde_json::json!({
            "id": download.id,
            "download_token": download.download_token,
            "download_count": download.download_count,
            "download_limit": download.download_limit,
            "expires_at": download.expires_at,
            "is_expired": download.expires_at.map(|exp| chrono::Utc::now() > exp).unwrap_or(false),
            "downloads_remaining": download.download_limit.map(|limit| limit - download.download_count),
        })),
        Ok(None) => Json(serde_json::json!({
            "error": "Download not found"
        })),
        Err(e) => {
            tracing::error!("Failed to get download info: {}", e);
            Json(serde_json::json!({
                "error": "Failed to retrieve download info"
            }))
        }
    }
}

/// Generate a new download token (for admin or re-download scenarios)
///
/// POST /api/v1/orders/:order_id/items/:item_id/downloads
pub async fn create_download(
    State(state): State<AppState>,
    Path((order_id, item_id)): Path<(String, String)>,
) -> Json<serde_json::Value> {
    let order_id = match Uuid::parse_str(&order_id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Json(serde_json::json!({
                "error": "Invalid order ID format"
            }));
        }
    };

    let item_id = match Uuid::parse_str(&item_id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return Json(serde_json::json!({
                "error": "Invalid item ID format"
            }));
        }
    };

    // Get the order item to find product details for download limits
    let order_item = match sqlx::query_as::<_, rcommerce_core::models::OrderItem>(
        "SELECT * FROM order_items WHERE id = $1 AND order_id = $2",
    )
    .bind(item_id)
    .bind(order_id)
    .fetch_one(state.db.pool())
    .await
    {
        Ok(item) => item,
        Err(_) => {
            return Json(serde_json::json!({
                "error": "Order item not found"
            }));
        }
    };

    // Get product details
    let product = match sqlx::query_as::<_, rcommerce_core::models::Product>(
        "SELECT * FROM products WHERE id = $1",
    )
    .bind(order_item.product_id)
    .fetch_one(state.db.pool())
    .await
    {
        Ok(product) => product,
        Err(_) => {
            return Json(serde_json::json!({
                "error": "Product not found"
            }));
        }
    };

    // Create download
    match state
        .digital_product_service
        .create_download(
            item_id,
            None, // customer_id would come from auth
            product.download_limit,
            product.download_expiry_days,
        )
        .await
    {
        Ok(download) => Json(serde_json::json!({
            "id": download.id,
            "download_token": download.download_token,
            "download_url": format!("/api/v1/downloads/{}", download.download_token),
            "expires_at": download.expires_at,
            "download_limit": download.download_limit,
        })),
        Err(e) => {
            tracing::error!("Failed to create download: {}", e);
            Json(serde_json::json!({
                "error": "Failed to create download"
            }))
        }
    }
}

/// Router for download routes
pub fn router() -> Router<AppState> {
    Router::new()
        // Public download endpoint (with token)
        .route("/downloads/:token", get(download_file))
        .route("/downloads/:token/info", get(get_download_info))
        // Protected endpoints (would need auth middleware)
        .route("/orders/:order_id/downloads", get(list_order_downloads))
        .route(
            "/orders/:order_id/items/:item_id/downloads",
            post(create_download),
        )
}
