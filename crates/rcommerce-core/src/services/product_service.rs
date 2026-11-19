use std::sync::Arc;
use uuid::Uuid;

use crate::{
    models::{
        CreateProductRequest, CreateVariantRequest, Product, ProductFilter, ProductImage,
        ProductOption, ProductOptionValue, ProductVariant, UpdateProductRequest,
    },
    repository::traits::ProductRepositoryTrait,
    repository::ProductRepository,
    services::{PaginationParams, Service},
    Error, Result,
};

#[derive(Clone)]
pub struct ProductService {
    pub repository: Arc<ProductRepository>,
}

impl ProductService {
    pub fn new(repository: ProductRepository) -> Self {
        Self {
            repository: Arc::new(repository),
        }
    }

    /// Create a new product
    pub async fn create_product(&self, request: CreateProductRequest) -> Result<Product> {
        // Validate request
        if request.title.is_empty() {
            return Err(Error::validation("Product title cannot be empty"));
        }

        if request.price < rust_decimal::Decimal::new(0, 0) {
            return Err(Error::validation("Product price cannot be negative"));
        }

        // Check if slug already exists
        if let Some(_existing) = self.repository.find_by_slug(&request.slug).await? {
            return Err(Error::validation("Product slug already exists"));
        }

        // Create product
        let product = self.repository.create_with_request(request).await?;

        Ok(product)
    }

    /// Create a product option (e.g. "Size"). Validation + persistence live in
    /// the repository so the CLI and any admin-API route share the same checks.
    pub async fn create_option(
        &self,
        product_id: Uuid,
        name: &str,
        position: i32,
    ) -> Result<ProductOption> {
        self.repository
            .create_option(product_id, name, position)
            .await
    }

    /// Create a product variant (validated, transactional).
    pub async fn create_variant(&self, request: CreateVariantRequest) -> Result<ProductVariant> {
        self.repository.create_variant(request).await
    }

    /// Attach an option value to a variant, validating option+variant belong to
    /// the product and the combination stays unique.
    pub async fn create_option_value(
        &self,
        product_id: Uuid,
        option_id: Uuid,
        variant_id: Uuid,
        value: &str,
    ) -> Result<ProductOptionValue> {
        self.repository
            .create_option_value(product_id, option_id, variant_id, value)
            .await
    }

    /// Create a variant with its option values in one transaction (the path that
    /// makes a variable product's variant selectable and addable to cart).
    pub async fn create_variant_with_options(
        &self,
        request: CreateVariantRequest,
        option_values: &[(String, String)],
    ) -> Result<(ProductVariant, Vec<ProductOptionValue>)> {
        self.repository
            .create_variant_with_options(request, option_values)
            .await
    }

    /// Get product by ID with variants and images
    pub async fn get_product(&self, id: Uuid) -> Result<Option<ProductDetail>> {
        let product = match self.repository.find_by_id(id).await? {
            Some(p) => p,
            None => return Ok(None),
        };

        let (variants, images, options) = self.load_related(id).await?;

        Ok(Some(ProductDetail {
            product,
            variants,
            images,
            options,
        }))
    }

    /// List products with filtering and pagination
    pub async fn list_products(
        &self,
        filter: Option<ProductFilter>,
        pagination: PaginationParams,
    ) -> Result<ProductList> {
        let filter = filter.unwrap_or_default();
        let sort = crate::models::SortParams {
            field: "created_at".to_string(),
            direction: crate::models::SortDirection::Desc,
        };

        let pagination = crate::models::Pagination {
            page: pagination.page,
            per_page: pagination.per_page,
        };

        let products = self
            .repository
            .find_with_filter(&filter, &pagination, Some(&sort))
            .await?;
        let total = self.repository.count_by_filter(&filter).await?;

        Ok(ProductList {
            products,
            pagination: crate::services::PaginationInfo {
                page: pagination.page,
                per_page: pagination.per_page,
                total,
                total_pages: (total as f64 / pagination.per_page as f64).ceil() as i64,
            },
        })
    }

    /// Update product
    pub async fn update_product(&self, id: Uuid, request: UpdateProductRequest) -> Result<Product> {
        // Check if product exists
        self.repository
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::not_found("Product not found"))?;

        // Update
        let product = self.repository.update_with_request(id, request).await?;

        Ok(product)
    }

    /// Delete product
    pub async fn delete_product(&self, id: Uuid) -> Result<bool> {
        // Check if product exists
        self.repository
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::not_found("Product not found"))?;

        // Check if product has orders (soft delete in production)
        // For MVP, we'll do hard delete

        self.repository.delete(id).await
    }

    /// Get product by slug
    pub async fn get_product_by_slug(&self, slug: &str) -> Result<Option<ProductDetail>> {
        let product = match self.repository.find_by_slug(slug).await? {
            Some(p) => p,
            None => return Ok(None),
        };

        let (variants, images, options) = self.load_related(product.id).await?;

        Ok(Some(ProductDetail {
            product,
            variants,
            images,
            options,
        }))
    }
}

impl ProductService {
    /// Variants, images and options of a product, fetched concurrently
    async fn load_related(
        &self,
        product_id: Uuid,
    ) -> Result<(
        Vec<ProductVariant>,
        Vec<ProductImage>,
        Vec<(ProductOption, Vec<ProductOptionValue>)>,
    )> {
        tokio::try_join!(
            self.repository.find_variants(product_id),
            self.repository.find_images(product_id),
            self.repository.find_options(product_id),
        )
    }
}

#[async_trait::async_trait]
impl Service for ProductService {
    async fn health_check(&self) -> Result<()> {
        // Check database connectivity
        let _ = self.repository.list().await?;
        Ok(())
    }
}

/// Product detail with related data
#[derive(Debug, Clone)]
pub struct ProductDetail {
    pub product: Product,
    pub variants: Vec<ProductVariant>,
    pub images: Vec<ProductImage>,
    pub options: Vec<(ProductOption, Vec<ProductOptionValue>)>,
}

/// Product list with pagination info
#[derive(Debug, Clone)]
pub struct ProductList {
    pub products: Vec<Product>,
    pub pagination: crate::services::PaginationInfo,
}
