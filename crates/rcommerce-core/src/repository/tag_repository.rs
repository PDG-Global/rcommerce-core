//! Product tag repository for database operations

use async_trait::async_trait;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    models::{Product, ProductTag},
    Error, Result,
};

/// Repository trait for product tag operations
#[async_trait]
pub trait TagRepository: Send + Sync {
    /// Get a tag by ID
    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProductTag>>;

    /// Get a tag by name
    async fn get_by_name(&self, name: &str) -> Result<Option<ProductTag>>;

    /// Create a new tag
    async fn create(&self, name: &str) -> Result<ProductTag>;

    /// Update a tag
    async fn update(&self, id: Uuid, name: &str) -> Result<ProductTag>;

    /// Delete a tag
    async fn delete(&self, id: Uuid) -> Result<bool>;

    /// List all tags
    async fn list(&self, limit: i64, offset: i64) -> Result<Vec<ProductTag>>;

    /// Search tags by name
    async fn search(&self, query: &str, limit: i64) -> Result<Vec<ProductTag>>;

    /// Assign tag to product
    async fn assign_to_product(&self, product_id: Uuid, tag_id: Uuid) -> Result<()>;

    /// Remove tag from product
    async fn remove_from_product(&self, product_id: Uuid, tag_id: Uuid) -> Result<bool>;

    /// Get tags for a product
    async fn get_product_tags(&self, product_id: Uuid) -> Result<Vec<ProductTag>>;

    /// Get products with a specific tag
    async fn get_tagged_products(
        &self,
        tag_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Product>>;

    /// Get popular tags (most used)
    async fn get_popular(&self, limit: i64) -> Result<Vec<(ProductTag, i64)>>;

    /// Get or create tag by name
    async fn get_or_create(&self, name: &str) -> Result<ProductTag>;

    /// Bulk assign tags to product
    async fn bulk_assign_to_product(
        &self,
        product_id: Uuid,
        tag_names: &[String],
    ) -> Result<Vec<ProductTag>>;
}

/// PostgreSQL implementation of TagRepository
pub struct PostgresTagRepository {
    db: sqlx::PgPool,
}

impl PostgresTagRepository {
    /// Create a new PostgreSQL tag repository
    pub fn new(db: sqlx::PgPool) -> Self {
        Self { db }
    }
}

#[async_trait]
impl TagRepository for PostgresTagRepository {
    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProductTag>> {
        let tag = sqlx::query_as::<_, ProductTag>("SELECT * FROM product_tags WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to fetch tag: {}", e)))?;

        Ok(tag)
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<ProductTag>> {
        let tag = sqlx::query_as::<_, ProductTag>("SELECT * FROM product_tags WHERE name = $1")
            .bind(name)
            .fetch_optional(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to fetch tag by name: {}", e)))?;

        Ok(tag)
    }

    async fn create(&self, name: &str) -> Result<ProductTag> {
        let id = Uuid::new_v4();
        let tag = sqlx::query_as::<_, ProductTag>(
            r#"
            INSERT INTO product_tags (id, name)
            VALUES ($1, $2)
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(name)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to create tag: {}", e)))?;

        Ok(tag)
    }

    async fn update(&self, id: Uuid, name: &str) -> Result<ProductTag> {
        let tag = sqlx::query_as::<_, ProductTag>(
            r#"
            UPDATE product_tags 
            SET name = $1, updated_at = NOW()
            WHERE id = $2
            RETURNING *
            "#,
        )
        .bind(name)
        .bind(id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to update tag: {}", e)))?;

        Ok(tag)
    }

    async fn delete(&self, id: Uuid) -> Result<bool> {
        // First remove all product associations
        sqlx::query("DELETE FROM product_tag_relations WHERE tag_id = $1")
            .bind(id)
            .execute(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to remove tag relations: {}", e)))?;

        // Delete the tag
        let result = sqlx::query("DELETE FROM product_tags WHERE id = $1")
            .bind(id)
            .execute(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to delete tag: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    async fn list(&self, limit: i64, offset: i64) -> Result<Vec<ProductTag>> {
        let tags = sqlx::query_as::<_, ProductTag>(
            "SELECT * FROM product_tags ORDER BY name LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to list tags: {}", e)))?;

        Ok(tags)
    }

    async fn search(&self, query: &str, limit: i64) -> Result<Vec<ProductTag>> {
        let search_pattern = format!("%{}%", query);
        let tags = sqlx::query_as::<_, ProductTag>(
            "SELECT * FROM product_tags WHERE name ILIKE $1 ORDER BY name LIMIT $2",
        )
        .bind(search_pattern)
        .bind(limit)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to search tags: {}", e)))?;

        Ok(tags)
    }

    async fn assign_to_product(&self, product_id: Uuid, tag_id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO product_tag_relations (product_id, tag_id)
            VALUES ($1, $2)
            ON CONFLICT (product_id, tag_id) DO NOTHING
            "#,
        )
        .bind(product_id)
        .bind(tag_id)
        .execute(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to assign tag to product: {}", e)))?;

        Ok(())
    }

    async fn remove_from_product(&self, product_id: Uuid, tag_id: Uuid) -> Result<bool> {
        let result =
            sqlx::query("DELETE FROM product_tag_relations WHERE product_id = $1 AND tag_id = $2")
                .bind(product_id)
                .bind(tag_id)
                .execute(&self.db)
                .await
                .map_err(|e| Error::Other(format!("Failed to remove tag from product: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    async fn get_product_tags(&self, product_id: Uuid) -> Result<Vec<ProductTag>> {
        let tags = sqlx::query_as::<_, ProductTag>(
            r#"
            SELECT t.* FROM product_tags t
            INNER JOIN product_tag_relations ptr ON t.id = ptr.tag_id
            WHERE ptr.product_id = $1
            ORDER BY t.name
            "#,
        )
        .bind(product_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch product tags: {}", e)))?;

        Ok(tags)
    }

    async fn get_tagged_products(
        &self,
        tag_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Product>> {
        let products = sqlx::query_as::<_, Product>(
            r#"
            SELECT p.* FROM products p
            INNER JOIN product_tag_relations ptr ON p.id = ptr.product_id
            WHERE ptr.tag_id = $1
            ORDER BY p.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(tag_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch tagged products: {}", e)))?;

        Ok(products)
    }

    async fn get_popular(&self, limit: i64) -> Result<Vec<(ProductTag, i64)>> {
        let rows = sqlx::query(
            r#"
            SELECT t.id, t.name, t.created_at, t.updated_at, COUNT(ptr.product_id) as usage_count
            FROM product_tags t
            LEFT JOIN product_tag_relations ptr ON t.id = ptr.tag_id
            GROUP BY t.id, t.name, t.created_at, t.updated_at
            ORDER BY usage_count DESC, t.name
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch popular tags: {}", e)))?;

        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let tag = ProductTag {
                id: row
                    .try_get("id")
                    .map_err(|e| Error::Other(format!("Failed to get id: {}", e)))?,
                name: row
                    .try_get("name")
                    .map_err(|e| Error::Other(format!("Failed to get name: {}", e)))?,
                created_at: row
                    .try_get("created_at")
                    .map_err(|e| Error::Other(format!("Failed to get created_at: {}", e)))?,
                updated_at: row
                    .try_get("updated_at")
                    .map_err(|e| Error::Other(format!("Failed to get updated_at: {}", e)))?,
            };
            let count: i64 = row
                .try_get("usage_count")
                .map_err(|e| Error::Other(format!("Failed to get count: {}", e)))?;
            result.push((tag, count));
        }

        Ok(result)
    }

    async fn get_or_create(&self, name: &str) -> Result<ProductTag> {
        // Try to find existing tag
        if let Some(tag) = self.get_by_name(name).await? {
            return Ok(tag);
        }

        // Create new tag
        self.create(name).await
    }

    async fn bulk_assign_to_product(
        &self,
        product_id: Uuid,
        tag_names: &[String],
    ) -> Result<Vec<ProductTag>> {
        let mut tags = Vec::with_capacity(tag_names.len());

        for name in tag_names {
            // Get or create tag
            let tag = self.get_or_create(name).await?;

            // Assign to product
            self.assign_to_product(product_id, tag.id).await?;

            tags.push(tag);
        }

        Ok(tags)
    }
}
