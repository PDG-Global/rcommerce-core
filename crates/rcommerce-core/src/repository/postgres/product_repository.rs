use async_trait::async_trait;
use sqlx::Row;
use uuid::Uuid;

use super::PostgresDb;
use crate::repository::traits::ProductRepositoryTrait;
use crate::{
    models::{
        CreateProductRequest, Product, ProductFilter, ProductImage, ProductOption,
        ProductOptionValue, ProductVariant, UpdateProductRequest,
    },
    Pagination, Result, SortDirection, SortParams,
};

#[derive(Clone)]
pub struct PostgresProductRepository {
    db: PostgresDb,
}

impl PostgresProductRepository {
    pub fn new(db: PostgresDb) -> Self {
        Self { db }
    }

    // Inherent methods for direct access
    pub async fn find_by_slug(&self, slug: &str) -> Result<Option<Product>> {
        let product = sqlx::query_as::<_, Product>("SELECT * FROM products WHERE slug = $1")
            .bind(slug)
            .fetch_optional(self.db.pool())
            .await?;

        Ok(product)
    }

    pub async fn create_with_request(&self, request: CreateProductRequest) -> Result<Product> {
        let product = sqlx::query_as::<_, Product>(
            r#"
            INSERT INTO products (
                title, slug, description, sku, price, compare_at_price, cost_price,
                currency, inventory_quantity, inventory_policy, inventory_management,
                continues_selling_when_out_of_stock, weight, weight_unit, requires_shipping,
                is_active, is_featured, seo_title, seo_description
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
            RETURNING *
            "#
        )
        .bind(request.title)
        .bind(request.slug)
        .bind(request.description)
        .bind(request.sku)
        .bind(request.price)
        .bind(request.compare_at_price)
        .bind(request.cost_price)
        .bind(request.currency)
        .bind(request.inventory_quantity)
        .bind(request.inventory_policy)
        .bind(request.inventory_management)
        .bind(request.continues_selling_when_out_of_stock)
        .bind(request.weight)
        .bind(request.weight_unit)
        .bind(request.requires_shipping)
        .bind(request.is_active)
        .bind(request.is_featured)
        .bind(request.seo_title)
        .bind(request.seo_description)
        .fetch_one(self.db.pool())
        .await?;

        Ok(product)
    }

    pub async fn update_with_request(
        &self,
        id: Uuid,
        request: UpdateProductRequest,
    ) -> Result<Product> {
        let mut sets = Vec::new();
        let mut param_count = 0;

        // Track which fields are present
        let has_title = request.title.is_some();
        let has_slug = request.slug.is_some();
        let has_description = request.description.is_some();
        let has_sku = request.sku.is_some();
        let has_price = request.price.is_some();
        let has_compare_at_price = request.compare_at_price.is_some();
        let has_is_active = request.is_active.is_some();

        if has_title {
            param_count += 1;
            sets.push(format!("title = ${}", param_count));
        }
        if has_slug {
            param_count += 1;
            sets.push(format!("slug = ${}", param_count));
        }
        if has_description {
            param_count += 1;
            sets.push(format!("description = ${}", param_count));
        }
        if has_sku {
            param_count += 1;
            sets.push(format!("sku = ${}", param_count));
        }
        if has_price {
            param_count += 1;
            sets.push(format!("price = ${}", param_count));
        }
        if has_compare_at_price {
            param_count += 1;
            sets.push(format!("compare_at_price = ${}", param_count));
        }
        if has_is_active {
            param_count += 1;
            sets.push(format!("is_active = ${}", param_count));
        }

        if sets.is_empty() {
            return Err(crate::Error::Validation("No fields to update".to_string()));
        }

        let id_idx = param_count + 1;
        let query = format!(
            "UPDATE products SET {}, updated_at = NOW() WHERE id = ${} RETURNING *",
            sets.join(", "),
            id_idx
        );

        // Build query with explicit binds
        let mut query_builder = sqlx::query_as::<_, Product>(&query);

        if let Some(title) = request.title {
            query_builder = query_builder.bind(title);
        }
        if let Some(slug) = request.slug {
            query_builder = query_builder.bind(slug);
        }
        if let Some(description) = request.description {
            query_builder = query_builder.bind(description);
        }
        if let Some(sku) = request.sku {
            query_builder = query_builder.bind(sku);
        }
        if let Some(price) = request.price {
            query_builder = query_builder.bind(price);
        }
        if let Some(compare_at_price) = request.compare_at_price {
            query_builder = query_builder.bind(compare_at_price);
        }
        if let Some(is_active) = request.is_active {
            query_builder = query_builder.bind(is_active);
        }
        query_builder = query_builder.bind(id);

        let product = query_builder.fetch_one(self.db.pool()).await?;

        Ok(product)
    }

    pub async fn find_variants(&self, product_id: Uuid) -> Result<Vec<ProductVariant>> {
        let variants = sqlx::query_as::<_, ProductVariant>(
            "SELECT * FROM product_variants WHERE product_id = $1 ORDER BY created_at",
        )
        .bind(product_id)
        .fetch_all(self.db.pool())
        .await?;

        Ok(variants)
    }

    pub async fn find_images(&self, product_id: Uuid) -> Result<Vec<ProductImage>> {
        let images = sqlx::query_as::<_, ProductImage>(
            "SELECT * FROM product_images WHERE product_id = $1 ORDER BY position",
        )
        .bind(product_id)
        .fetch_all(self.db.pool())
        .await?;

        Ok(images)
    }

    pub async fn find_options(
        &self,
        product_id: Uuid,
    ) -> Result<Vec<(ProductOption, Vec<ProductOptionValue>)>> {
        let options = sqlx::query_as::<_, ProductOption>(
            "SELECT * FROM product_options WHERE product_id = $1 ORDER BY position",
        )
        .bind(product_id)
        .fetch_all(self.db.pool())
        .await?;

        let mut result = Vec::new();
        for option in options {
            let values = sqlx::query_as::<_, ProductOptionValue>(
                "SELECT * FROM product_option_values WHERE option_id = $1 ORDER BY value",
            )
            .bind(option.id)
            .fetch_all(self.db.pool())
            .await?;
            result.push((option, values));
        }

        Ok(result)
    }
}

#[async_trait]
impl ProductRepositoryTrait for PostgresProductRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Product>> {
        let product = sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = $1")
            .bind(id)
            .fetch_optional(self.db.pool())
            .await?;

        Ok(product)
    }

    async fn list(&self) -> Result<Vec<Product>> {
        let products =
            sqlx::query_as::<_, Product>("SELECT * FROM products ORDER BY created_at DESC")
                .fetch_all(self.db.pool())
                .await?;

        Ok(products)
    }

    async fn find_with_filter(
        &self,
        filter: &ProductFilter,
        pagination: &Pagination,
        sort: Option<&SortParams>,
    ) -> Result<Vec<Product>> {
        let mut query = String::from("SELECT * FROM products WHERE 1=1");
        let mut param_count = 0;

        // Track which parameters we need to bind
        let has_category = filter.category_id.is_some();
        let has_price_min = filter.price_min.is_some();
        let has_price_max = filter.price_max.is_some();

        if let Some(status) = filter.status {
            match status {
                crate::models::ProductStatus::Active => {
                    query.push_str(" AND is_active = true");
                }
                crate::models::ProductStatus::Draft => {
                    query.push_str(" AND is_active = false AND published_at IS NULL");
                }
                crate::models::ProductStatus::Archived => {}
            }
        }

        if has_category {
            param_count += 1;
            query.push_str(&format!(
                " AND id IN (SELECT product_id FROM product_category_relations WHERE category_id = ${})",
                param_count
            ));
        }

        if has_price_min {
            param_count += 1;
            query.push_str(&format!(" AND price >= ${}", param_count));
        }

        if has_price_max {
            param_count += 1;
            query.push_str(&format!(" AND price <= ${}", param_count));
        }

        // Add sorting - validate sort field against whitelist
        if let Some(sort) = sort {
            let allowed_columns = [
                "id",
                "title",
                "slug",
                "price",
                "created_at",
                "updated_at",
                "inventory_quantity",
            ];
            if !allowed_columns.contains(&sort.field.as_str()) {
                return Err(crate::Error::validation("Invalid sort field"));
            }
            let direction = match sort.direction {
                SortDirection::Asc => "ASC",
                SortDirection::Desc => "DESC",
            };
            // Use the validated field directly in the query string
            // This is safe because we've validated it against a whitelist
            query.push_str(&format!(" ORDER BY {} {}", sort.field, direction));
        } else {
            query.push_str(" ORDER BY created_at DESC");
        }

        // Add pagination
        let limit_idx = param_count + 1;
        let offset_idx = param_count + 2;
        query.push_str(&format!(" LIMIT ${} OFFSET ${}", limit_idx, offset_idx));

        // Build query with explicit binds
        let mut query_builder = sqlx::query_as::<_, Product>(&query);

        if let Some(category_id) = filter.category_id {
            query_builder = query_builder.bind(category_id);
        }
        if let Some(price_min) = filter.price_min {
            query_builder = query_builder.bind(price_min);
        }
        if let Some(price_max) = filter.price_max {
            query_builder = query_builder.bind(price_max);
        }
        query_builder = query_builder.bind(pagination.per_page);
        query_builder = query_builder.bind(pagination.offset());

        let products = query_builder.fetch_all(self.db.pool()).await?;

        Ok(products)
    }

    async fn find_by_slug(&self, slug: &str) -> Result<Option<Product>> {
        self.find_by_slug(slug).await
    }

    async fn count_by_filter(&self, filter: &ProductFilter) -> Result<i64> {
        let mut query = String::from("SELECT COUNT(*) FROM products WHERE 1=1");

        if let Some(_status) = filter.status {}

        if filter.category_id.is_some() {
            query.push_str(" AND id IN (SELECT product_id FROM product_category_relations WHERE category_id = $1)");
        }

        let row = if let Some(category_id) = filter.category_id {
            sqlx::query(&query)
                .bind(category_id)
                .fetch_one(self.db.pool())
                .await?
        } else {
            sqlx::query(&query).fetch_one(self.db.pool()).await?
        };

        let count: i64 = row.get(0);
        Ok(count)
    }

    async fn create(&self, request: CreateProductRequest) -> Result<Product> {
        self.create_with_request(request).await
    }

    async fn update(&self, id: Uuid, request: UpdateProductRequest) -> Result<Product> {
        self.update_with_request(id, request).await
    }

    async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM products WHERE id = $1")
            .bind(id)
            .execute(self.db.pool())
            .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn find_variants(&self, product_id: Uuid) -> Result<Vec<ProductVariant>> {
        self.find_variants(product_id).await
    }

    async fn find_images(&self, product_id: Uuid) -> Result<Vec<ProductImage>> {
        self.find_images(product_id).await
    }

    async fn find_options(
        &self,
        product_id: Uuid,
    ) -> Result<Vec<(ProductOption, Vec<ProductOptionValue>)>> {
        self.find_options(product_id).await
    }
}
