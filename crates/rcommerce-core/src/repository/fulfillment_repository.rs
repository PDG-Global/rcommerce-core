//! Fulfillment repository for database operations

use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    order::fulfillment::{Fulfillment, FulfillmentItem, FulfillmentStatus},
    Error, Result,
};

/// Repository trait for fulfillment operations
#[async_trait]
pub trait FulfillmentRepository: Send + Sync {
    /// Get a fulfillment by ID
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Fulfillment>>;

    /// Get all fulfillments for an order
    async fn get_by_order(&self, order_id: Uuid) -> Result<Vec<Fulfillment>>;

    /// Create a new fulfillment
    async fn create(&self, fulfillment: &Fulfillment) -> Result<Fulfillment>;

    /// Update a fulfillment
    async fn update(&self, fulfillment: &Fulfillment) -> Result<Fulfillment>;

    /// Update fulfillment status
    async fn update_status(&self, id: Uuid, status: FulfillmentStatus) -> Result<Fulfillment>;

    /// Delete a fulfillment
    async fn delete(&self, id: Uuid) -> Result<bool>;

    /// Add items to a fulfillment
    async fn add_items(&self, items: &[FulfillmentItem]) -> Result<Vec<FulfillmentItem>>;

    /// Get items for a fulfillment
    async fn get_items(&self, fulfillment_id: Uuid) -> Result<Vec<FulfillmentItem>>;

    /// Update tracking information
    async fn update_tracking(
        &self,
        id: Uuid,
        tracking_number: Option<&str>,
        tracking_url: Option<&str>,
        tracking_company: Option<&str>,
    ) -> Result<Fulfillment>;

    /// Mark as shipped
    async fn mark_shipped(&self, id: Uuid, tracking_number: Option<&str>) -> Result<Fulfillment>;

    /// Mark as delivered
    async fn mark_delivered(&self, id: Uuid) -> Result<Fulfillment>;

    /// List fulfillments by status
    async fn list_by_status(
        &self,
        status: FulfillmentStatus,
        limit: i64,
    ) -> Result<Vec<Fulfillment>>;

    /// Get pending fulfillments (not yet shipped)
    async fn get_pending(&self, limit: i64) -> Result<Vec<Fulfillment>>;
}

/// PostgreSQL implementation of FulfillmentRepository
pub struct PostgresFulfillmentRepository {
    db: sqlx::PgPool,
}

impl PostgresFulfillmentRepository {
    /// Create a new PostgreSQL fulfillment repository
    pub fn new(db: sqlx::PgPool) -> Self {
        Self { db }
    }
}

#[async_trait]
impl FulfillmentRepository for PostgresFulfillmentRepository {
    async fn get_by_id(&self, id: Uuid) -> Result<Option<Fulfillment>> {
        let fulfillment =
            sqlx::query_as::<_, Fulfillment>("SELECT * FROM fulfillments WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.db)
                .await
                .map_err(|e| Error::Other(format!("Failed to fetch fulfillment: {}", e)))?;

        Ok(fulfillment)
    }

    async fn get_by_order(&self, order_id: Uuid) -> Result<Vec<Fulfillment>> {
        let fulfillments = sqlx::query_as::<_, Fulfillment>(
            "SELECT * FROM fulfillments WHERE order_id = $1 ORDER BY created_at DESC",
        )
        .bind(order_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch fulfillments: {}", e)))?;

        Ok(fulfillments)
    }

    async fn create(&self, fulfillment: &Fulfillment) -> Result<Fulfillment> {
        let fulfillment = sqlx::query_as::<_, Fulfillment>(
            r#"
            INSERT INTO fulfillments (
                id, order_id, status, tracking_number, tracking_url, 
                tracking_company, shipped_at, delivered_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            "#,
        )
        .bind(fulfillment.id)
        .bind(fulfillment.order_id)
        .bind(fulfillment.status)
        .bind(&fulfillment.tracking_number)
        .bind(&fulfillment.tracking_url)
        .bind(&fulfillment.tracking_company)
        .bind(fulfillment.shipped_at)
        .bind(fulfillment.delivered_at)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to create fulfillment: {}", e)))?;

        Ok(fulfillment)
    }

    async fn update(&self, fulfillment: &Fulfillment) -> Result<Fulfillment> {
        let fulfillment = sqlx::query_as::<_, Fulfillment>(
            r#"
            UPDATE fulfillments 
            SET status = $1,
                tracking_number = $2,
                tracking_url = $3,
                tracking_company = $4,
                shipped_at = $5,
                delivered_at = $6,
                updated_at = NOW()
            WHERE id = $7
            RETURNING *
            "#,
        )
        .bind(fulfillment.status)
        .bind(&fulfillment.tracking_number)
        .bind(&fulfillment.tracking_url)
        .bind(&fulfillment.tracking_company)
        .bind(fulfillment.shipped_at)
        .bind(fulfillment.delivered_at)
        .bind(fulfillment.id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to update fulfillment: {}", e)))?;

        Ok(fulfillment)
    }

    async fn update_status(&self, id: Uuid, status: FulfillmentStatus) -> Result<Fulfillment> {
        let fulfillment = sqlx::query_as::<_, Fulfillment>(
            "UPDATE fulfillments SET status = $1, updated_at = NOW() WHERE id = $2 RETURNING *",
        )
        .bind(status)
        .bind(id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to update fulfillment status: {}", e)))?;

        Ok(fulfillment)
    }

    async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM fulfillments WHERE id = $1")
            .bind(id)
            .execute(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to delete fulfillment: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    async fn add_items(&self, items: &[FulfillmentItem]) -> Result<Vec<FulfillmentItem>> {
        let mut inserted_items = Vec::with_capacity(items.len());

        for item in items {
            let inserted = sqlx::query_as::<_, FulfillmentItem>(
                r#"
                INSERT INTO fulfillment_items (id, fulfillment_id, order_item_id, quantity)
                VALUES ($1, $2, $3, $4)
                RETURNING *
                "#,
            )
            .bind(item.id)
            .bind(item.fulfillment_id)
            .bind(item.order_item_id)
            .bind(item.quantity)
            .fetch_one(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to add fulfillment item: {}", e)))?;

            inserted_items.push(inserted);
        }

        Ok(inserted_items)
    }

    async fn get_items(&self, fulfillment_id: Uuid) -> Result<Vec<FulfillmentItem>> {
        let items = sqlx::query_as::<_, FulfillmentItem>(
            "SELECT * FROM fulfillment_items WHERE fulfillment_id = $1",
        )
        .bind(fulfillment_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch fulfillment items: {}", e)))?;

        Ok(items)
    }

    async fn update_tracking(
        &self,
        id: Uuid,
        tracking_number: Option<&str>,
        tracking_url: Option<&str>,
        tracking_company: Option<&str>,
    ) -> Result<Fulfillment> {
        let fulfillment = sqlx::query_as::<_, Fulfillment>(
            r#"
            UPDATE fulfillments 
            SET tracking_number = $1,
                tracking_url = $2,
                tracking_company = $3,
                updated_at = NOW()
            WHERE id = $4
            RETURNING *
            "#,
        )
        .bind(tracking_number)
        .bind(tracking_url)
        .bind(tracking_company)
        .bind(id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to update tracking: {}", e)))?;

        Ok(fulfillment)
    }

    async fn mark_shipped(&self, id: Uuid, tracking_number: Option<&str>) -> Result<Fulfillment> {
        let fulfillment = sqlx::query_as::<_, Fulfillment>(
            r#"
            UPDATE fulfillments 
            SET status = 'shipped',
                tracking_number = COALESCE($1, tracking_number),
                shipped_at = NOW(),
                updated_at = NOW()
            WHERE id = $2
            RETURNING *
            "#,
        )
        .bind(tracking_number)
        .bind(id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to mark as shipped: {}", e)))?;

        Ok(fulfillment)
    }

    async fn mark_delivered(&self, id: Uuid) -> Result<Fulfillment> {
        let fulfillment = sqlx::query_as::<_, Fulfillment>(
            r#"
            UPDATE fulfillments 
            SET status = 'delivered',
                delivered_at = NOW(),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to mark as delivered: {}", e)))?;

        Ok(fulfillment)
    }

    async fn list_by_status(
        &self,
        status: FulfillmentStatus,
        limit: i64,
    ) -> Result<Vec<Fulfillment>> {
        let fulfillments = sqlx::query_as::<_, Fulfillment>(
            "SELECT * FROM fulfillments WHERE status = $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(status)
        .bind(limit)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to list fulfillments: {}", e)))?;

        Ok(fulfillments)
    }

    async fn get_pending(&self, limit: i64) -> Result<Vec<Fulfillment>> {
        let fulfillments = sqlx::query_as::<_, Fulfillment>(
            r#"
            SELECT * FROM fulfillments 
            WHERE status IN ('pending', 'processing') 
            ORDER BY created_at ASC 
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch pending fulfillments: {}", e)))?;

        Ok(fulfillments)
    }
}
