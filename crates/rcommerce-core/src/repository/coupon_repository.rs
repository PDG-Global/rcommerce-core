//! Coupon Repository

use async_trait::async_trait;
use rust_decimal::Decimal;
use sqlx::{Pool, Postgres};
use uuid::Uuid;

use crate::{
    models::{Coupon, CouponApplication},
    Error, Result,
};

/// Coupon repository trait
#[async_trait]
pub trait CouponRepository: Send + Sync {
    /// Find coupon by ID
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Coupon>>;

    /// Find coupon by code
    async fn find_by_code(&self, code: &str) -> Result<Option<Coupon>>;

    /// Find all active coupons
    async fn find_active(&self) -> Result<Vec<Coupon>>;

    /// Find all coupons
    async fn find_all(&self) -> Result<Vec<Coupon>>;

    /// Create a new coupon
    async fn create(&self, coupon: &Coupon) -> Result<()>;

    /// Update coupon
    async fn update(&self, coupon: &Coupon) -> Result<()>;

    /// Delete coupon
    async fn delete(&self, id: Uuid) -> Result<()>;

    /// Get usage count for a coupon
    async fn get_usage_count(&self, coupon_id: Uuid) -> Result<i32>;

    /// Get customer usage count for a coupon
    async fn get_customer_usage_count(&self, coupon_id: Uuid, customer_id: Uuid) -> Result<i32>;

    /// Record coupon usage
    async fn record_usage(
        &self,
        coupon_id: Uuid,
        customer_id: Option<Uuid>,
        order_id: Uuid,
        discount_amount: Decimal,
    ) -> Result<()>;

    /// Increment usage count
    async fn increment_usage_count(&self, coupon_id: Uuid) -> Result<()>;

    /// Get total discount amount for a coupon
    async fn get_total_discount_amount(&self, coupon_id: Uuid) -> Result<Decimal>;

    /// Get coupon applications (product/collection restrictions)
    async fn get_applications(&self, coupon_id: Uuid) -> Result<Vec<CouponApplication>>;

    /// Add application to coupon
    async fn add_application(
        &self,
        coupon_id: Uuid,
        product_id: Option<Uuid>,
        collection_id: Option<Uuid>,
        is_exclusion: bool,
    ) -> Result<()>;

    /// Remove application from coupon
    async fn remove_application(&self, application_id: Uuid) -> Result<()>;
}

/// PostgreSQL implementation of CouponRepository
pub struct PgCouponRepository {
    pool: Pool<Postgres>,
}

impl PgCouponRepository {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CouponRepository for PgCouponRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Coupon>> {
        let coupon = sqlx::query_as::<_, Coupon>(
            r#"
            SELECT * FROM coupons WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(coupon)
    }

    async fn find_by_code(&self, code: &str) -> Result<Option<Coupon>> {
        let coupon = sqlx::query_as::<_, Coupon>(
            r#"
            SELECT * FROM coupons WHERE code = $1
            "#,
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(coupon)
    }

    async fn find_active(&self) -> Result<Vec<Coupon>> {
        let coupons = sqlx::query_as::<_, Coupon>(
            r#"
            SELECT * FROM coupons 
            WHERE is_active = true
            AND (expires_at IS NULL OR expires_at > NOW())
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(coupons)
    }

    async fn find_all(&self) -> Result<Vec<Coupon>> {
        let coupons = sqlx::query_as::<_, Coupon>(
            r#"
            SELECT * FROM coupons ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(coupons)
    }

    async fn create(&self, coupon: &Coupon) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO coupons (
                id, code, description, discount_type, discount_value, minimum_purchase,
                maximum_discount, is_active, starts_at, expires_at, usage_limit,
                usage_limit_per_customer, usage_count, applies_to_specific_products,
                applies_to_specific_collections, can_combine, created_by, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
            "#
        )
        .bind(coupon.id)
        .bind(&coupon.code)
        .bind(&coupon.description)
        .bind(coupon.discount_type)
        .bind(coupon.discount_value)
        .bind(coupon.minimum_purchase)
        .bind(coupon.maximum_discount)
        .bind(coupon.is_active)
        .bind(coupon.starts_at)
        .bind(coupon.expires_at)
        .bind(coupon.usage_limit)
        .bind(coupon.usage_limit_per_customer)
        .bind(coupon.usage_count)
        .bind(coupon.applies_to_specific_products)
        .bind(coupon.applies_to_specific_collections)
        .bind(coupon.can_combine)
        .bind(coupon.created_by)
        .bind(coupon.created_at)
        .bind(coupon.updated_at)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn update(&self, coupon: &Coupon) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE coupons SET
                description = $2,
                discount_type = $3,
                discount_value = $4,
                minimum_purchase = $5,
                maximum_discount = $6,
                is_active = $7,
                starts_at = $8,
                expires_at = $9,
                usage_limit = $10,
                usage_limit_per_customer = $11,
                applies_to_specific_products = $12,
                applies_to_specific_collections = $13,
                can_combine = $14,
                updated_at = $15
            WHERE id = $1
            "#,
        )
        .bind(coupon.id)
        .bind(&coupon.description)
        .bind(coupon.discount_type)
        .bind(coupon.discount_value)
        .bind(coupon.minimum_purchase)
        .bind(coupon.maximum_discount)
        .bind(coupon.is_active)
        .bind(coupon.starts_at)
        .bind(coupon.expires_at)
        .bind(coupon.usage_limit)
        .bind(coupon.usage_limit_per_customer)
        .bind(coupon.applies_to_specific_products)
        .bind(coupon.applies_to_specific_collections)
        .bind(coupon.can_combine)
        .bind(coupon.updated_at)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM coupons WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }

    async fn get_usage_count(&self, coupon_id: Uuid) -> Result<i32> {
        let count: i32 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(SUM(usage_count), 0) FROM coupons WHERE id = $1
            "#,
        )
        .bind(coupon_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(count)
    }

    async fn get_customer_usage_count(&self, coupon_id: Uuid, customer_id: Uuid) -> Result<i32> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM coupon_usages WHERE coupon_id = $1 AND customer_id = $2
            "#,
        )
        .bind(coupon_id)
        .bind(customer_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(count as i32)
    }

    async fn record_usage(
        &self,
        coupon_id: Uuid,
        customer_id: Option<Uuid>,
        order_id: Uuid,
        discount_amount: Decimal,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO coupon_usages (id, coupon_id, customer_id, order_id, discount_amount, used_at)
            VALUES ($1, $2, $3, $4, $5, NOW())
            "#
        )
        .bind(Uuid::new_v4())
        .bind(coupon_id)
        .bind(customer_id)
        .bind(order_id)
        .bind(discount_amount)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn increment_usage_count(&self, coupon_id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE coupons SET usage_count = usage_count + 1 WHERE id = $1
            "#,
        )
        .bind(coupon_id)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn get_total_discount_amount(&self, coupon_id: Uuid) -> Result<Decimal> {
        let total: Option<Decimal> = sqlx::query_scalar(
            r#"
            SELECT COALESCE(SUM(discount_amount), 0) FROM coupon_usages WHERE coupon_id = $1
            "#,
        )
        .bind(coupon_id)
        .fetch_one(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(total.unwrap_or(Decimal::ZERO))
    }

    async fn get_applications(&self, coupon_id: Uuid) -> Result<Vec<CouponApplication>> {
        let applications = sqlx::query_as::<_, CouponApplication>(
            r#"
            SELECT * FROM coupon_applications WHERE coupon_id = $1
            "#,
        )
        .bind(coupon_id)
        .fetch_all(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(applications)
    }

    async fn add_application(
        &self,
        coupon_id: Uuid,
        product_id: Option<Uuid>,
        collection_id: Option<Uuid>,
        is_exclusion: bool,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO coupon_applications (id, coupon_id, product_id, collection_id, is_exclusion, created_at)
            VALUES ($1, $2, $3, $4, $5, NOW())
            "#
        )
        .bind(Uuid::new_v4())
        .bind(coupon_id)
        .bind(product_id)
        .bind(collection_id)
        .bind(is_exclusion)
        .execute(&self.pool)
        .await
        .map_err(Error::Database)?;

        Ok(())
    }

    async fn remove_application(&self, application_id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM coupon_applications WHERE id = $1")
            .bind(application_id)
            .execute(&self.pool)
            .await
            .map_err(Error::Database)?;

        Ok(())
    }
}
