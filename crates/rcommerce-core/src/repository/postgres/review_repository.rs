//! PostgreSQL repository for product reviews

use sqlx::Row;
use uuid::Uuid;

use super::PostgresDb;
use crate::{
    models::{ProductReview, ProductReviewSummary},
    Result,
};

/// PostgreSQL product review repository
#[derive(Clone)]
pub struct PostgresProductReviewRepository {
    db: PostgresDb,
}

impl PostgresProductReviewRepository {
    pub fn new(db: PostgresDb) -> Self {
        Self { db }
    }

    /// List approved reviews for a product, newest first
    pub async fn list_approved(&self, product_id: Uuid) -> Result<Vec<ProductReview>> {
        let reviews = sqlx::query_as::<_, ProductReview>(
            r#"
            SELECT * FROM product_reviews
            WHERE product_id = $1 AND status = 'approved'
            ORDER BY created_at DESC
            "#,
        )
        .bind(product_id)
        .fetch_all(self.db.pool())
        .await?;

        Ok(reviews)
    }

    /// Aggregate stats for a product's approved reviews: (average_rating, total_reviews)
    pub async fn approved_stats(&self, product_id: Uuid) -> Result<(f64, i64)> {
        let row = sqlx::query(
            r#"
            SELECT AVG(rating)::float8 AS average_rating, COUNT(*)::int8 AS total_reviews
            FROM product_reviews
            WHERE product_id = $1 AND status = 'approved'
            "#,
        )
        .bind(product_id)
        .fetch_one(self.db.pool())
        .await?;

        let average_rating: Option<f64> = row.get("average_rating");
        let total_reviews: i64 = row.get("total_reviews");

        Ok((average_rating.unwrap_or(0.0), total_reviews))
    }

    /// Find an existing review by customer and product (for upsert semantics)
    pub async fn find_by_customer_and_product(
        &self,
        product_id: Uuid,
        customer_id: Uuid,
    ) -> Result<Option<ProductReview>> {
        let review = sqlx::query_as::<_, ProductReview>(
            r#"
            SELECT * FROM product_reviews
            WHERE product_id = $1 AND customer_id = $2
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(product_id)
        .bind(customer_id)
        .fetch_optional(self.db.pool())
        .await?;

        Ok(review)
    }

    /// Create a new review
    pub async fn create(
        &self,
        product_id: Uuid,
        customer_id: Option<Uuid>,
        author_name: &str,
        rating: i32,
        title: Option<&str>,
        content: &str,
    ) -> Result<ProductReview> {
        let review = sqlx::query_as::<_, ProductReview>(
            r#"
            INSERT INTO product_reviews (
                id, product_id, customer_id, author_name, rating, title, content,
                status, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'approved', NOW(), NOW())
            RETURNING *
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(product_id)
        .bind(customer_id)
        .bind(author_name)
        .bind(rating)
        .bind(title)
        .bind(content)
        .fetch_one(self.db.pool())
        .await?;

        Ok(review)
    }

    /// Update an existing review (one review per customer per product)
    pub async fn update(
        &self,
        review_id: Uuid,
        rating: i32,
        title: Option<&str>,
        content: &str,
    ) -> Result<ProductReview> {
        let review = sqlx::query_as::<_, ProductReview>(
            r#"
            UPDATE product_reviews
            SET rating = $2, title = $3, content = $4, updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(review_id)
        .bind(rating)
        .bind(title)
        .bind(content)
        .fetch_one(self.db.pool())
        .await?;

        Ok(review)
    }

    /// Delete a review belonging to a product; returns true when a row was removed
    pub async fn delete(&self, product_id: Uuid, review_id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM product_reviews WHERE id = $1 AND product_id = $2")
            .bind(review_id)
            .bind(product_id)
            .execute(self.db.pool())
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

impl From<ProductReview> for ProductReviewSummary {
    fn from(review: ProductReview) -> Self {
        ProductReviewSummary {
            id: review.id,
            author_name: review.author_name,
            rating: review.rating,
            title: review.title,
            content: review.content,
            created_at: review.created_at,
        }
    }
}
