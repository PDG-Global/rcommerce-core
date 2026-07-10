use async_trait::async_trait;
use uuid::Uuid;
use sqlx::Row;

use crate::{
    Result, Pagination, SortParams, SortDirection,
    models::{
        Product, ProductVariant, ProductImage, ProductFilter,
        CreateProductRequest, UpdateProductRequest, BundleComponent,
        ProductOption, ProductOptionValue,
    },
    traits::Repository,
};
use super::Database;

pub struct ProductRepository {
    db: Database,
}

impl ProductRepository {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
    
    /// Find options for a product
    pub async fn find_options(&self, product_id: Uuid) -> Result<Vec<(ProductOption, Vec<ProductOptionValue>)>> {
        let options = sqlx::query_as::<_, ProductOption>(
            "SELECT * FROM product_options WHERE product_id = $1 ORDER BY position"
        )
        .bind(product_id)
        .fetch_all(self.db.pool())
        .await?;

        let mut result = Vec::new();
        for option in options {
            let values = sqlx::query_as::<_, ProductOptionValue>(
                "SELECT * FROM product_option_values WHERE option_id = $1 ORDER BY value"
            )
            .bind(option.id)
            .fetch_all(self.db.pool())
            .await?;
            result.push((option, values));
        }

        Ok(result)
    }
    
    /// Find products with filtering and pagination
    pub async fn find_with_filter(
        &self,
        filter: &ProductFilter,
        pagination: &Pagination,
        sort: Option<&SortParams>,
    ) -> Result<Vec<Product>> {
        let mut query = String::from(
            "SELECT * FROM products WHERE 1=1"
        );
        let mut param_count = 0;
        
        // Track which parameters we need to bind
        let has_category = filter.category_id.is_some();
        let has_price_min = filter.price_min.is_some();
        let has_price_max = filter.price_max.is_some();
        
        // Add filters
        if let Some(status) = filter.status {
            match status {
                super::super::models::product::ProductStatus::Active => {
                    query.push_str(" AND is_active = true");
                }
                super::super::models::product::ProductStatus::Draft => {
                    query.push_str(" AND is_active = false AND published_at IS NULL");
                }
                super::super::models::product::ProductStatus::Archived => {
                    // Need to add archived field to products table
                }
            }
        }
        
        if has_category {
            param_count += 1;
            query.push_str(&format!(" AND id IN (SELECT product_id FROM product_category_relations WHERE category_id = ${})", param_count));
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
            let allowed_columns = ["id", "title", "slug", "price", "created_at", "updated_at", "inventory_quantity"];
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
        
        let products = query_builder
            .fetch_all(self.db.pool())
            .await?;
        
        Ok(products)
    }
    
    /// Find variants for a product
    pub async fn find_variants(&self, product_id: Uuid) -> Result<Vec<ProductVariant>> {
        let variants = sqlx::query_as::<_, ProductVariant>(
            "SELECT * FROM product_variants WHERE product_id = $1 ORDER BY created_at"
        )
        .bind(product_id)
        .fetch_all(self.db.pool())
        .await?;
        
        Ok(variants)
    }
    
    /// Find images for a product
    pub async fn find_images(&self, product_id: Uuid) -> Result<Vec<ProductImage>> {
        let images = sqlx::query_as::<_, ProductImage>(
            "SELECT * FROM product_images WHERE product_id = $1 ORDER BY position"
        )
        .bind(product_id)
        .fetch_all(self.db.pool())
        .await?;
        
        Ok(images)
    }

    /// Find bundle components for a product
    pub async fn find_bundle_components(&self, product_id: Uuid) -> Result<Vec<BundleComponent>> {
        let components = sqlx::query_as::<_, BundleComponent>(
            r#"
            SELECT bc.*, 
                p.id as "component_product_id", p.title, p.slug, p.price
            FROM bundle_components bc
            JOIN products p ON bc.component_product_id = p.id
            WHERE bc.bundle_product_id = $1
            ORDER BY bc.sort_order, bc.created_at
            "#
        )
        .bind(product_id)
        .fetch_all(self.db.pool())
        .await?;
        
        Ok(components)
    }
}

#[async_trait]
impl Repository<Product, Uuid> for ProductRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Product>> {
        let product = sqlx::query_as::<_, Product>(
            "SELECT * FROM products WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await?;
        
        Ok(product)
    }
    
    async fn list(&self) -> Result<Vec<Product>> {
        let products = sqlx::query_as::<_, Product>(
            "SELECT * FROM products ORDER BY created_at DESC"
        )
        .fetch_all(self.db.pool())
        .await?;
        
        Ok(products)
    }
    
    async fn create(&self, _entity: Product) -> Result<Product> {
        // This is a simplified version - in production, use CreateProductRequest
        Err(crate::Error::not_implemented("Use create_with_request instead".to_string()))
    }
    
    async fn update(&self, _entity: Product) -> Result<Product> {
        Err(crate::Error::not_implemented("Use update_with_request instead".to_string()))
    }
    
    async fn delete(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query("DELETE FROM products WHERE id = $1")
            .bind(id)
            .execute(self.db.pool())
            .await?;
        
        Ok(result.rows_affected() > 0)
    }
}

/// Extension trait for product-specific operations
impl ProductRepository {
    pub async fn create_with_request(&self, request: CreateProductRequest) -> Result<Product> {
        let product = sqlx::query_as::<_, Product>(
            r#"
            INSERT INTO products (
                title, slug, description, sku, product_type, price, compare_at_price, cost_price,
                currency, inventory_quantity, inventory_policy, inventory_management,
                continues_selling_when_out_of_stock, weight, weight_unit, requires_shipping,
                is_active, is_featured, seo_title, seo_description,
                file_url, file_size, file_hash, download_limit, license_key_enabled, download_expiry_days,
                bundle_pricing_strategy, bundle_discount_percentage
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
                    $21, $22, $23, $24, $25, $26, $27, $28)
            RETURNING *
            "#
        )
        .bind(request.title)
        .bind(request.slug)
        .bind(request.description)
        .bind(request.sku)
        .bind(request.product_type)
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
        // Digital product fields
        .bind(request.file_url)
        .bind(request.file_size)
        .bind(request.file_hash)
        .bind(request.download_limit)
        .bind(request.license_key_enabled)
        .bind(request.download_expiry_days)
        // Bundle product fields
        .bind(request.bundle_pricing_strategy)
        .bind(request.bundle_discount_percentage)
        .fetch_one(self.db.pool())
        .await?;
        
        Ok(product)
    }
    
    pub async fn update_with_request(&self, id: Uuid, request: UpdateProductRequest) -> Result<Product> {
        // Build dynamic update query based on which fields are provided
        let mut sets = Vec::new();
        let mut param_count = 0;
        
        // Track which fields are present
        let has_title = request.title.is_some();
        let has_slug = request.slug.is_some();
        let has_description = request.description.is_some();
        let has_price = request.price.is_some();
        let has_currency = request.currency.is_some();
        let has_is_active = request.is_active.is_some();
        let has_product_type = request.product_type.is_some();
        let has_file_url = request.file_url.is_some();
        let has_file_size = request.file_size.is_some();
        let has_file_hash = request.file_hash.is_some();
        let has_download_limit = request.download_limit.is_some();
        let has_license_key_enabled = request.license_key_enabled.is_some();
        let has_download_expiry_days = request.download_expiry_days.is_some();
        let has_bundle_pricing_strategy = request.bundle_pricing_strategy.is_some();
        let has_bundle_discount_percentage = request.bundle_discount_percentage.is_some();
        
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
        if has_price {
            param_count += 1;
            sets.push(format!("price = ${}", param_count));
        }
        if has_currency {
            param_count += 1;
            sets.push(format!("currency = ${}", param_count));
        }
        if has_is_active {
            param_count += 1;
            sets.push(format!("is_active = ${}", param_count));
        }
        if has_product_type {
            param_count += 1;
            sets.push(format!("product_type = ${}", param_count));
        }
        if has_file_url {
            param_count += 1;
            sets.push(format!("file_url = ${}", param_count));
        }
        if has_file_size {
            param_count += 1;
            sets.push(format!("file_size = ${}", param_count));
        }
        if has_file_hash {
            param_count += 1;
            sets.push(format!("file_hash = ${}", param_count));
        }
        if has_download_limit {
            param_count += 1;
            sets.push(format!("download_limit = ${}", param_count));
        }
        if has_license_key_enabled {
            param_count += 1;
            sets.push(format!("license_key_enabled = ${}", param_count));
        }
        if has_download_expiry_days {
            param_count += 1;
            sets.push(format!("download_expiry_days = ${}", param_count));
        }
        if has_bundle_pricing_strategy {
            param_count += 1;
            sets.push(format!("bundle_pricing_strategy = ${}", param_count));
        }
        if has_bundle_discount_percentage {
            param_count += 1;
            sets.push(format!("bundle_discount_percentage = ${}", param_count));
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
        if let Some(price) = request.price {
            query_builder = query_builder.bind(price);
        }
        if let Some(currency) = request.currency {
            query_builder = query_builder.bind(currency);
        }
        if let Some(is_active) = request.is_active {
            query_builder = query_builder.bind(is_active);
        }
        if let Some(product_type) = request.product_type {
            query_builder = query_builder.bind(product_type);
        }
        if let Some(file_url) = request.file_url {
            query_builder = query_builder.bind(file_url);
        }
        if let Some(file_size) = request.file_size {
            query_builder = query_builder.bind(file_size);
        }
        if let Some(file_hash) = request.file_hash {
            query_builder = query_builder.bind(file_hash);
        }
        if let Some(download_limit) = request.download_limit {
            query_builder = query_builder.bind(download_limit);
        }
        if let Some(license_key_enabled) = request.license_key_enabled {
            query_builder = query_builder.bind(license_key_enabled);
        }
        if let Some(download_expiry_days) = request.download_expiry_days {
            query_builder = query_builder.bind(download_expiry_days);
        }
        if let Some(bundle_pricing_strategy) = request.bundle_pricing_strategy {
            query_builder = query_builder.bind(bundle_pricing_strategy);
        }
        if let Some(bundle_discount_percentage) = request.bundle_discount_percentage {
            query_builder = query_builder.bind(bundle_discount_percentage);
        }
        query_builder = query_builder.bind(id);
        
        let product = query_builder
            .fetch_one(self.db.pool())
            .await?;
        
        Ok(product)
    }
    
    pub async fn find_by_slug(&self, slug: &str) -> Result<Option<Product>> {
        let product = sqlx::query_as::<_, Product>(
            "SELECT * FROM products WHERE slug = $1"
        )
        .bind(slug)
        .fetch_optional(self.db.pool())
        .await?;
        
        Ok(product)
    }
    
    pub async fn count_by_filter(&self, filter: &ProductFilter) -> Result<i64> {
        let mut query = String::from("SELECT COUNT(*) FROM products WHERE 1=1");
        
        // Add same filters as find_with_filter
        if let Some(_status) = filter.status {
            // TODO: Add status filtering
        }
        
        if filter.category_id.is_some() {
            query.push_str(" AND id IN (SELECT product_id FROM product_category_relations WHERE category_id = $1)");
        }
        
        let row = if let Some(category_id) = filter.category_id {
            sqlx::query(&query)
                .bind(category_id)
                .fetch_one(self.db.pool())
                .await?
        } else {
            sqlx::query(&query)
                .fetch_one(self.db.pool())
                .await?
        };
        
        let count: i64 = row.get(0);
        Ok(count)
    }
}
