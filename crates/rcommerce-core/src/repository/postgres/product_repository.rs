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

/// Escape user input for use inside an ILIKE pattern matched with ESCAPE '\'.
/// Without this, '%' and '_' in the query would act as wildcards and a
/// backslash could break out of the escaping scheme.
fn escape_ilike_pattern(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
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
                is_active, is_featured, seo_title, seo_description, gtin
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
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
        .bind(request.gtin)
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
        let has_weight = request.weight.is_some();
        let has_weight_unit = request.weight_unit.is_some();
        let has_inventory_quantity = request.inventory_quantity.is_some();
        let has_gtin = request.gtin.is_some();

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
        if has_gtin {
            param_count += 1;
            sets.push(format!("gtin = ${}", param_count));
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
        if has_weight {
            param_count += 1;
            sets.push(format!("weight = ${}", param_count));
        }
        if has_weight_unit {
            param_count += 1;
            sets.push(format!("weight_unit = ${}", param_count));
        }
        if has_inventory_quantity {
            param_count += 1;
            sets.push(format!("inventory_quantity = ${}", param_count));
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
        if let Some(gtin) = request.gtin {
            query_builder = query_builder.bind(gtin);
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
        if let Some(weight) = request.weight {
            query_builder = query_builder.bind(weight);
        }
        if let Some(weight_unit) = request.weight_unit {
            query_builder = query_builder.bind(weight_unit);
        }
        if let Some(inventory_quantity) = request.inventory_quantity {
            query_builder = query_builder.bind(inventory_quantity);
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

        if options.is_empty() {
            return Ok(Vec::new());
        }

        // All values in one query instead of one query per option
        let option_ids: Vec<Uuid> = options.iter().map(|o| o.id).collect();
        let values = sqlx::query_as::<_, ProductOptionValue>(
            "SELECT * FROM product_option_values WHERE option_id = ANY($1) ORDER BY value",
        )
        .bind(&option_ids)
        .fetch_all(self.db.pool())
        .await?;

        let mut by_option: std::collections::HashMap<Uuid, Vec<ProductOptionValue>> =
            std::collections::HashMap::new();
        for value in values {
            by_option.entry(value.option_id).or_default().push(value);
        }
        Ok(options
            .into_iter()
            .map(|option| {
                let values = by_option.remove(&option.id).unwrap_or_default();
                (option, values)
            })
            .collect())
    }

    /// First image (lowest position) of several products, in one query
    pub async fn find_first_images(
        &self,
        product_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, String>> {
        if product_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let rows = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT DISTINCT ON (product_id) product_id, src FROM product_images
             WHERE product_id = ANY($1) ORDER BY product_id, position",
        )
        .bind(product_ids)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().collect())
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
        let has_query = filter
            .query
            .as_deref()
            .map(|q| !q.trim().is_empty())
            .unwrap_or(false);

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

        if has_query {
            param_count += 1;
            // Escape ILIKE wildcards in the user input so '%' and '_' are
            // treated literally; ESCAPE '\' handles the backslash itself.
            query.push_str(&format!(
                " AND (title ILIKE '%' || ${0} || '%' ESCAPE '\\' OR description ILIKE '%' || ${0} || '%' ESCAPE '\\')",
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
        if let Some(query_text) = filter
            .query
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
        {
            query_builder = query_builder.bind(escape_ilike_pattern(query_text));
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
        let mut param_count = 0;

        // Must mirror find_with_filter, otherwise meta.total counts
        // inactive/draft products that the page query excludes.
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

        if filter.category_id.is_some() {
            param_count += 1;
            query.push_str(&format!(
                " AND id IN (SELECT product_id FROM product_category_relations WHERE category_id = ${})",
                param_count
            ));
        }

        let has_query = filter
            .query
            .as_deref()
            .map(|q| !q.trim().is_empty())
            .unwrap_or(false);
        if has_query {
            param_count += 1;
            query.push_str(&format!(
                " AND (title ILIKE '%' || ${0} || '%' ESCAPE '\\' OR description ILIKE '%' || ${0} || '%' ESCAPE '\\')",
                param_count
            ));
        }

        let mut q = sqlx::query(&query);
        if let Some(category_id) = filter.category_id {
            q = q.bind(category_id);
        }
        if let Some(query_text) = filter
            .query
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
        {
            q = q.bind(escape_ilike_pattern(query_text));
        }

        let row = q.fetch_one(self.db.pool()).await?;

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

#[cfg(test)]
mod tests {
    use super::*;

    /// DB-backed tests: require PostgreSQL via TEST_DATABASE_URL; skipped without it
    async fn test_pool() -> Option<sqlx::PgPool> {
        let url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres@localhost:5432/rcommerce_scratch".to_string());
        let pool = match sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
        {
            Ok(pool) => pool,
            Err(e) => {
                eprintln!("Skipping DB test: cannot connect to {url}: {e}");
                return None;
            }
        };
        if let Err(e) = crate::ensure_test_migrations(&pool).await {
            eprintln!("Skipping DB test: migration failed: {e}");
            return None;
        }
        Some(pool)
    }

    async fn insert_product(pool: &sqlx::PgPool, title: &str) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO products (id, title, slug, price) VALUES ($1, $2, $3, 10.00)")
            .bind(id)
            .bind(title)
            .bind(format!("batched-{id}"))
            .execute(pool)
            .await
            .unwrap();
        id
    }

    #[tokio::test]
    async fn batched_lookups_return_the_same_data() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let repo = PostgresProductRepository::new(crate::repository::postgres::PostgresDb::new(
            pool.clone(),
        ));
        let with_images = insert_product(&pool, "With images").await;
        let without_images = insert_product(&pool, "Without images").await;
        for (position, src) in [(2, "second.jpg"), (0, "first.jpg")] {
            sqlx::query(
                "INSERT INTO product_images (product_id, position, src) VALUES ($1, $2, $3)",
            )
            .bind(with_images)
            .bind(position)
            .bind(src)
            .execute(&pool)
            .await
            .unwrap();
        }

        let first = repo
            .find_first_images(&[with_images, without_images])
            .await
            .unwrap();
        assert_eq!(
            first.get(&with_images).map(String::as_str),
            Some("first.jpg")
        );
        assert!(!first.contains_key(&without_images));
        assert!(repo.find_first_images(&[]).await.unwrap().is_empty());

        // One option ("Size") with a value per variant, grouped under the option
        let option_id: Uuid = sqlx::query_scalar(
            "INSERT INTO product_options (product_id, name) VALUES ($1, 'Size') RETURNING id",
        )
        .bind(with_images)
        .fetch_one(&pool)
        .await
        .unwrap();
        for value in ["M", "L", "S"] {
            let variant_id: Uuid = sqlx::query_scalar(
                "INSERT INTO product_variants (product_id, title, price) VALUES ($1, $2, 10.00) RETURNING id",
            )
            .bind(with_images)
            .bind(value)
            .fetch_one(&pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO product_option_values (option_id, variant_id, value) VALUES ($1, $2, $3)")
                .bind(option_id)
                .bind(variant_id)
                .bind(value)
                .execute(&pool)
                .await
                .unwrap();
        }
        let options = repo.find_options(with_images).await.unwrap();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].0.name, "Size");
        let values: Vec<&str> = options[0].1.iter().map(|v| v.value.as_str()).collect();
        assert_eq!(values, ["L", "M", "S"]);
        assert!(repo.find_options(without_images).await.unwrap().is_empty());

        sqlx::query("DELETE FROM products WHERE id = ANY($1)")
            .bind([with_images, without_images].as_slice())
            .execute(&pool)
            .await
            .unwrap();
    }
}
