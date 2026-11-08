//! Download routes for digital products
//!
//! Provides endpoints for:
//! - Downloading files with a download token (public: the token is the secret)
//! - Listing an order's downloads (the order's customer, or an admin)
//! - Generating a download token for an order item (admin)

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::Response,
    routing::{get, post},
    Extension, Json, Router,
};
use uuid::Uuid;

use crate::middleware::JwtAuth;
use crate::state::AppState;
use rcommerce_core::models::{OrderItem, Product};
use rcommerce_core::Error;

/// JSON body on success, the error's status and message otherwise
type JsonResult = Result<Json<serde_json::Value>, Error>;

/// Download URL for a token
fn download_url(token: &str) -> String {
    format!("/api/v1/downloads/{}", token)
}

/// List downloads for an order (its customer, or an admin)
///
/// GET /api/v1/orders/:id/downloads
pub async fn list_order_downloads(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(order_id): Path<Uuid>,
) -> JsonResult {
    let owner: Option<Option<Uuid>> =
        sqlx::query_scalar("SELECT customer_id FROM orders WHERE id = $1")
            .bind(order_id)
            .fetch_optional(state.db.pool())
            .await?;
    // Someone else's order answers as a missing one
    match owner {
        Some(owner) if owner == Some(auth.customer_id) || auth.is_admin() => {}
        _ => return Err(Error::not_found("Order not found")),
    }

    let downloads: Vec<serde_json::Value> = state
        .digital_product_service
        .get_order_downloads(order_id)
        .await?
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
                "download_url": download_url(&d.download_token),
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "downloads": downloads })))
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
) -> JsonResult {
    let download = state
        .digital_product_service
        .get_download_by_token(&token)
        .await?
        .ok_or_else(|| Error::not_found("Download not found"))?;
    Ok(Json(serde_json::json!({
        "id": download.id,
        "download_token": download.download_token,
        "download_count": download.download_count,
        "download_limit": download.download_limit,
        "expires_at": download.expires_at,
        "is_expired": download.expires_at.map(|exp| chrono::Utc::now() > exp).unwrap_or(false),
        "downloads_remaining": download.download_limit.map(|limit| limit - download.download_count),
    })))
}

/// Generate a new download token for an order item, with the product's
/// download limit and expiry (admin: a customer creating its own would reset
/// its download limit)
///
/// POST /api/v1/admin/orders/:id/items/:item_id/downloads
pub async fn create_download(
    State(state): State<AppState>,
    Path((order_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<(StatusCode, Json<serde_json::Value>), Error> {
    let item =
        sqlx::query_as::<_, OrderItem>("SELECT * FROM order_items WHERE id = $1 AND order_id = $2")
            .bind(item_id)
            .bind(order_id)
            .fetch_optional(state.db.pool())
            .await?
            .ok_or_else(|| Error::not_found("Order item not found"))?;
    let product = sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = $1")
        .bind(item.product_id)
        .fetch_optional(state.db.pool())
        .await?
        .ok_or_else(|| Error::not_found("Product not found"))?;
    let customer_id: Option<Uuid> =
        sqlx::query_scalar("SELECT customer_id FROM orders WHERE id = $1")
            .bind(order_id)
            .fetch_one(state.db.pool())
            .await?;

    let download = state
        .digital_product_service
        .create_download(
            item_id,
            customer_id,
            product.download_limit,
            product.download_expiry_days,
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": download.id,
            "download_token": download.download_token,
            "download_url": download_url(&download.download_token),
            "expires_at": download.expires_at,
            "download_limit": download.download_limit,
        })),
    ))
}

/// Public download routes: the token in the path is the credential
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/downloads/:token", get(download_file))
        .route("/downloads/:token/info", get(get_download_info))
}

/// Download routes for signed-in customers (mounted behind auth_middleware)
pub fn protected_router() -> Router<AppState> {
    Router::new().route("/orders/:id/downloads", get(list_order_downloads))
}

/// Admin download routes (mounted behind admin_middleware)
pub fn admin_router() -> Router<AppState> {
    Router::new().route(
        "/admin/orders/:id/items/:item_id/downloads",
        post(create_download),
    )
}
