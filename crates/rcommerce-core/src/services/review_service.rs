use uuid::Uuid;
use validator::Validate;

use crate::{
    models::{CreateProductReviewRequest, ProductReview, ProductReviewSummary},
    repository::PostgresProductReviewRepository,
    Error, Result,
};

/// Product review service
///
/// Handles storefront review submissions with one-review-per-customer
/// upsert semantics and public read access to approved reviews.
#[derive(Clone)]
pub struct ReviewService {
    repository: PostgresProductReviewRepository,
}

impl ReviewService {
    pub fn new(repository: PostgresProductReviewRepository) -> Self {
        Self { repository }
    }

    /// List approved reviews for a product (newest first) with aggregate stats
    pub async fn list_for_product(&self, product_id: Uuid) -> Result<ProductReviewsListing> {
        let reviews = self.repository.list_approved(product_id).await?;
        let (average_rating, total_reviews) = self.repository.approved_stats(product_id).await?;

        Ok(ProductReviewsListing {
            reviews: reviews
                .into_iter()
                .map(ProductReviewSummary::from)
                .collect(),
            average_rating,
            total_reviews,
        })
    }

    /// Create a review, or update the customer's existing review for the
    /// product (one review per customer per product).
    pub async fn create_or_update(
        &self,
        product_id: Uuid,
        customer_id: Uuid,
        author_name: &str,
        request: CreateProductReviewRequest,
    ) -> Result<ProductReview> {
        request
            .validate()
            .map_err(|e| Error::validation(format!("Invalid review: {}", e)))?;

        if request.content.trim().is_empty() {
            return Err(Error::validation("Review content cannot be empty"));
        }

        let rating = request.rating.clamp(1, 5);
        let title = request
            .title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty());

        if let Some(existing) = self
            .repository
            .find_by_customer_and_product(product_id, customer_id)
            .await?
        {
            self.repository
                .update(existing.id, rating, title, request.content.trim())
                .await
        } else {
            self.repository
                .create(
                    product_id,
                    Some(customer_id),
                    author_name,
                    rating,
                    title,
                    request.content.trim(),
                )
                .await
        }
    }

    /// Delete a review belonging to a product (admin)
    pub async fn delete_review(&self, product_id: Uuid, review_id: Uuid) -> Result<()> {
        let deleted = self.repository.delete(product_id, review_id).await?;
        if !deleted {
            return Err(Error::not_found("Review not found"));
        }
        Ok(())
    }
}

/// Public reviews listing with aggregate stats
#[derive(Debug, Clone)]
pub struct ProductReviewsListing {
    pub reviews: Vec<ProductReviewSummary>,
    pub average_rating: f64,
    pub total_reviews: i64,
}
