//! Product review routes
//!
//! - GET  /api/v1/products/:id/reviews              (public)
//! - POST /api/v1/products/:id/reviews              (JWT required)
//! - DELETE /api/v1/admin/products/:id/reviews/:review_id (admin)

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Extension, Json, Router,
};
use uuid::Uuid;

use crate::middleware::JwtAuth;
use crate::state::AppState;
use rcommerce_core::models::CreateProductReviewRequest;
use rcommerce_core::repository::{Database, PostgresProductReviewRepository};
use rcommerce_core::services::ReviewService;
use rcommerce_core::Error;

fn review_service(state: &AppState) -> ReviewService {
    ReviewService::new(PostgresProductReviewRepository::new(Database::new(
        state.db.pool().clone(),
    )))
}

/// GET /api/v1/products/:id/reviews - List approved reviews for a product
pub async fn list_product_reviews(
    State(state): State<AppState>,
    Path(product_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, Error> {
    // Ensure the product exists
    let product_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM products WHERE id = $1)")
            .bind(product_id)
            .fetch_one(state.db.pool())
            .await?;
    if !product_exists {
        return Err(Error::not_found("Product not found"));
    }

    let listing = review_service(&state).list_for_product(product_id).await?;

    Ok(Json(serde_json::json!({
        "reviews": listing.reviews,
        "average_rating": listing.average_rating,
        "total_reviews": listing.total_reviews,
    })))
}

/// POST /api/v1/products/:id/reviews - Create or update the customer's review
///
/// One review per customer per product: if the customer already reviewed the
/// product, their existing review is updated instead (200 either way).
pub async fn create_product_review(
    State(state): State<AppState>,
    Extension(auth): Extension<JwtAuth>,
    Path(product_id): Path<Uuid>,
    Json(request): Json<CreateProductReviewRequest>,
) -> Result<Json<serde_json::Value>, Error> {
    // Validate and persist
    let service = review_service(&state);
    // Pre-validate before touching the DB so bad payloads get a clean 400
    use validator::Validate;
    request
        .validate()
        .map_err(|e| Error::validation(format!("Invalid review: {}", e)))?;

    // Ensure the product exists
    let product_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM products WHERE id = $1)")
            .bind(product_id)
            .fetch_one(state.db.pool())
            .await?;
    if !product_exists {
        return Err(Error::not_found("Product not found"));
    }

    // Resolve the display name from the customer's record
    let customer = sqlx::query_as::<_, rcommerce_core::models::Customer>(
        "SELECT * FROM customers WHERE id = $1",
    )
    .bind(auth.customer_id)
    .fetch_optional(state.db.pool())
    .await?
    .ok_or_else(|| Error::not_found("Customer not found"))?;

    let author_name = format!("{} {}", customer.first_name, customer.last_name)
        .trim()
        .to_string();
    let author_name = if author_name.is_empty() {
        customer.email.clone()
    } else {
        author_name
    };

    let review = service
        .create_or_update(product_id, auth.customer_id, &author_name, request)
        .await?;

    Ok(Json(serde_json::json!({
        "review": {
            "id": review.id,
            "author_name": review.author_name,
            "rating": review.rating,
            "title": review.title,
            "content": review.content,
            "created_at": review.created_at,
        }
    })))
}

/// DELETE /api/v1/admin/products/:id/reviews/:review_id - Delete a review (admin)
///
/// Admin authorization is enforced by the admin_middleware route layer.
pub async fn delete_product_review(
    State(state): State<AppState>,
    Path((product_id, review_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, Error> {
    review_service(&state)
        .delete_review(product_id, review_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Router for public review routes
pub fn public_router() -> Router<AppState> {
    Router::new().route("/products/:id/reviews", get(list_product_reviews))
}

/// Router for review routes requiring JWT authentication
pub fn protected_router() -> Router<AppState> {
    Router::new().route("/products/:id/reviews", post(create_product_review))
}

/// Router for admin review routes (mounted under the admin middleware layer)
pub fn admin_router() -> Router<AppState> {
    Router::new().route(
        "/admin/products/:id/reviews/:review_id",
        delete(delete_product_review),
    )
}
