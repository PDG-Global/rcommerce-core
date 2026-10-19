use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

/// Product review entity
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ProductReview {
    pub id: Uuid,
    pub product_id: Uuid,
    /// Nullable: imported reviews may not map to a local customer account
    pub customer_id: Option<Uuid>,
    /// Denormalized display name (customer first + last name at submit time)
    pub author_name: String,
    pub rating: i32,
    pub title: Option<String>,
    pub content: String,
    /// 'pending' | 'approved' | 'rejected'
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Create product review request (storefront submission)
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateProductReviewRequest {
    #[validate(range(min = 1, max = 5))]
    pub rating: i32,

    #[validate(length(max = 500))]
    pub title: Option<String>,

    #[validate(length(min = 1))]
    pub content: String,
}

/// Public review payload returned by the reviews API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductReviewSummary {
    pub id: Uuid,
    pub author_name: String,
    pub rating: i32,
    pub title: Option<String>,
    pub content: String,
    pub created_at: DateTime<Utc>,
}
