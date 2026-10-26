//! Product category repository for database operations

use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    models::{Product, ProductCategory},
    Error, Result,
};

/// Repository trait for product category operations
#[async_trait]
pub trait CategoryRepository: Send + Sync {
    /// Get a category by ID
    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProductCategory>>;

    /// Get a category by slug
    async fn get_by_slug(&self, slug: &str) -> Result<Option<ProductCategory>>;

    /// Create a new category
    async fn create(&self, category: &ProductCategory) -> Result<ProductCategory>;

    /// Update a category
    async fn update(&self, category: &ProductCategory) -> Result<ProductCategory>;

    /// Delete a category
    async fn delete(&self, id: Uuid) -> Result<bool>;

    /// List all categories
    async fn list(&self, limit: i64, offset: i64) -> Result<Vec<ProductCategory>>;

    /// Get root categories (no parent)
    async fn get_roots(&self) -> Result<Vec<ProductCategory>>;

    /// Get child categories
    async fn get_children(&self, parent_id: Uuid) -> Result<Vec<ProductCategory>>;

    /// Get category tree (recursive)
    async fn get_tree(&self, root_id: Option<Uuid>) -> Result<Vec<CategoryTreeNode>>;

    /// Assign product to category
    async fn assign_product(&self, product_id: Uuid, category_id: Uuid) -> Result<()>;

    /// Remove product from category
    async fn remove_product(&self, product_id: Uuid, category_id: Uuid) -> Result<bool>;

    /// Get products in category
    async fn get_products(
        &self,
        category_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Product>>;

    /// Get categories for a product
    async fn get_product_categories(&self, product_id: Uuid) -> Result<Vec<ProductCategory>>;

    /// Count products in category
    async fn count_products(&self, category_id: Uuid) -> Result<i64>;

    /// Search categories by name
    async fn search(&self, query: &str, limit: i64) -> Result<Vec<ProductCategory>>;
}

/// Tree node for category hierarchy
#[derive(Debug, Clone)]
pub struct CategoryTreeNode {
    pub category: ProductCategory,
    pub children: Vec<CategoryTreeNode>,
}

/// PostgreSQL implementation of CategoryRepository
pub struct PostgresCategoryRepository {
    db: sqlx::PgPool,
}

impl PostgresCategoryRepository {
    /// Create a new PostgreSQL category repository
    pub fn new(db: sqlx::PgPool) -> Self {
        Self { db }
    }
}

#[async_trait]
impl CategoryRepository for PostgresCategoryRepository {
    async fn get_by_id(&self, id: Uuid) -> Result<Option<ProductCategory>> {
        let category =
            sqlx::query_as::<_, ProductCategory>("SELECT * FROM product_categories WHERE id = $1")
                .bind(id)
                .fetch_optional(&self.db)
                .await
                .map_err(|e| Error::Other(format!("Failed to fetch category: {}", e)))?;

        Ok(category)
    }

    async fn get_by_slug(&self, slug: &str) -> Result<Option<ProductCategory>> {
        let category = sqlx::query_as::<_, ProductCategory>(
            "SELECT * FROM product_categories WHERE slug = $1",
        )
        .bind(slug)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch category by slug: {}", e)))?;

        Ok(category)
    }

    async fn create(&self, category: &ProductCategory) -> Result<ProductCategory> {
        let category = sqlx::query_as::<_, ProductCategory>(
            r#"
            INSERT INTO product_categories (id, name, slug, description, parent_id, sort_order)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(category.id)
        .bind(&category.name)
        .bind(&category.slug)
        .bind(&category.description)
        .bind(category.parent_id)
        .bind(category.sort_order)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to create category: {}", e)))?;

        Ok(category)
    }

    async fn update(&self, category: &ProductCategory) -> Result<ProductCategory> {
        let category = sqlx::query_as::<_, ProductCategory>(
            r#"
            UPDATE product_categories 
            SET name = $1,
                slug = $2,
                description = $3,
                parent_id = $4,
                sort_order = $5,
                updated_at = NOW()
            WHERE id = $6
            RETURNING *
            "#,
        )
        .bind(&category.name)
        .bind(&category.slug)
        .bind(&category.description)
        .bind(category.parent_id)
        .bind(category.sort_order)
        .bind(category.id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to update category: {}", e)))?;

        Ok(category)
    }

    async fn delete(&self, id: Uuid) -> Result<bool> {
        // First, update children to have no parent or move to grandparent
        sqlx::query("UPDATE product_categories SET parent_id = NULL WHERE parent_id = $1")
            .bind(id)
            .execute(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to update child categories: {}", e)))?;

        // Delete category-product relations
        sqlx::query("DELETE FROM product_category_relations WHERE category_id = $1")
            .bind(id)
            .execute(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to delete category relations: {}", e)))?;

        // Delete the category
        let result = sqlx::query("DELETE FROM product_categories WHERE id = $1")
            .bind(id)
            .execute(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to delete category: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    async fn list(&self, limit: i64, offset: i64) -> Result<Vec<ProductCategory>> {
        let categories = sqlx::query_as::<_, ProductCategory>(
            "SELECT * FROM product_categories WHERE is_active = true ORDER BY sort_order, name LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to list categories: {}", e)))?;

        Ok(categories)
    }

    async fn get_roots(&self) -> Result<Vec<ProductCategory>> {
        let categories = sqlx::query_as::<_, ProductCategory>(
            "SELECT * FROM product_categories WHERE parent_id IS NULL ORDER BY sort_order, name",
        )
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch root categories: {}", e)))?;

        Ok(categories)
    }

    async fn get_children(&self, parent_id: Uuid) -> Result<Vec<ProductCategory>> {
        let categories = sqlx::query_as::<_, ProductCategory>(
            "SELECT * FROM product_categories WHERE parent_id = $1 ORDER BY sort_order, name",
        )
        .bind(parent_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch child categories: {}", e)))?;

        Ok(categories)
    }

    async fn get_tree(&self, root_id: Option<Uuid>) -> Result<Vec<CategoryTreeNode>> {
        // Get all categories
        let all_categories = if let Some(root) = root_id {
            // Get the root and all descendants
            sqlx::query_as::<_, ProductCategory>(
                r#"
                WITH RECURSIVE category_tree AS (
                    SELECT * FROM product_categories WHERE id = $1
                    UNION ALL
                    SELECT c.* FROM product_categories c
                    INNER JOIN category_tree ct ON c.parent_id = ct.id
                )
                SELECT * FROM category_tree ORDER BY sort_order, name
                "#,
            )
            .bind(root)
            .fetch_all(&self.db)
            .await
            .map_err(|e| Error::Other(format!("Failed to fetch category tree: {}", e)))?
        } else {
            // Get all categories
            self.list(1000, 0).await?
        };

        // Build tree structure
        fn build_tree(
            categories: &[ProductCategory],
            parent_id: Option<Uuid>,
        ) -> Vec<CategoryTreeNode> {
            categories
                .iter()
                .filter(|c| c.parent_id == parent_id)
                .map(|cat| CategoryTreeNode {
                    category: cat.clone(),
                    children: build_tree(categories, Some(cat.id)),
                })
                .collect()
        }

        Ok(build_tree(&all_categories, root_id))
    }

    async fn assign_product(&self, product_id: Uuid, category_id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO product_category_relations (product_id, category_id)
            VALUES ($1, $2)
            ON CONFLICT (product_id, category_id) DO NOTHING
            "#,
        )
        .bind(product_id)
        .bind(category_id)
        .execute(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to assign product to category: {}", e)))?;

        Ok(())
    }

    async fn remove_product(&self, product_id: Uuid, category_id: Uuid) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM product_category_relations WHERE product_id = $1 AND category_id = $2",
        )
        .bind(product_id)
        .bind(category_id)
        .execute(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to remove product from category: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    async fn get_products(
        &self,
        category_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Product>> {
        let products = sqlx::query_as::<_, Product>(
            r#"
            SELECT p.* FROM products p
            INNER JOIN product_category_relations pcr ON p.id = pcr.product_id
            WHERE pcr.category_id = $1 AND p.is_active = true
            ORDER BY p.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(category_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch products in category: {}", e)))?;

        Ok(products)
    }

    async fn get_product_categories(&self, product_id: Uuid) -> Result<Vec<ProductCategory>> {
        let categories = sqlx::query_as::<_, ProductCategory>(
            r#"
            SELECT c.* FROM product_categories c
            INNER JOIN product_category_relations pcr ON c.id = pcr.category_id
            WHERE pcr.product_id = $1
            ORDER BY c.sort_order, c.name
            "#,
        )
        .bind(product_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to fetch product categories: {}", e)))?;

        Ok(categories)
    }

    async fn count_products(&self, category_id: Uuid) -> Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM product_category_relations WHERE category_id = $1",
        )
        .bind(category_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to count products: {}", e)))?;

        Ok(count)
    }

    async fn search(&self, query: &str, limit: i64) -> Result<Vec<ProductCategory>> {
        let search_pattern = format!("%{}%", query);
        let categories = sqlx::query_as::<_, ProductCategory>(
            r#"
            SELECT * FROM product_categories 
            WHERE name ILIKE $1 OR description ILIKE $1
            ORDER BY name
            LIMIT $2
            "#,
        )
        .bind(search_pattern)
        .bind(limit)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Error::Other(format!("Failed to search categories: {}", e)))?;

        Ok(categories)
    }
}
