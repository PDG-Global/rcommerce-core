//! Repository traits for database abstraction
//!
//! These traits define the interface that all database implementations must provide.

use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    models::{
        CreateCustomerRequest, CreateProductRequest, Customer, Product, ProductFilter,
        ProductImage, ProductOption, ProductOptionValue, ProductVariant, UpdateCustomerRequest,
        UpdateProductRequest,
    },
    Pagination, Result, SortParams,
};

/// Product repository trait - database agnostic
#[async_trait]
pub trait ProductRepositoryTrait: Send + Sync + 'static {
    /// Find product by ID
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Product>>;

    /// List all products
    async fn list(&self) -> Result<Vec<Product>>;

    /// Find products with filtering and pagination
    async fn find_with_filter(
        &self,
        filter: &ProductFilter,
        pagination: &Pagination,
        sort: Option<&SortParams>,
    ) -> Result<Vec<Product>>;

    /// Find product by slug
    async fn find_by_slug(&self, slug: &str) -> Result<Option<Product>>;

    /// Count products by filter
    async fn count_by_filter(&self, filter: &ProductFilter) -> Result<i64>;

    /// Create new product
    async fn create(&self, request: CreateProductRequest) -> Result<Product>;

    /// Update product
    async fn update(&self, id: Uuid, request: UpdateProductRequest) -> Result<Product>;

    /// Delete product
    async fn delete(&self, id: Uuid) -> Result<bool>;

    /// Find variants for a product
    async fn find_variants(&self, product_id: Uuid) -> Result<Vec<ProductVariant>>;

    /// Find images for a product
    async fn find_images(&self, product_id: Uuid) -> Result<Vec<ProductImage>>;

    /// Find options and option values for a product
    async fn find_options(
        &self,
        product_id: Uuid,
    ) -> Result<Vec<(ProductOption, Vec<ProductOptionValue>)>>;
}

/// Customer repository trait - database agnostic
#[async_trait]
pub trait CustomerRepositoryTrait: Send + Sync + 'static {
    /// Find customer by ID
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Customer>>;

    /// Find customer by email
    async fn find_by_email(&self, email: &str) -> Result<Option<Customer>>;

    /// List all customers
    async fn list(&self) -> Result<Vec<Customer>>;

    /// Create new customer
    async fn create(&self, request: CreateCustomerRequest) -> Result<Customer>;

    /// Update customer
    async fn update(&self, id: Uuid, request: UpdateCustomerRequest) -> Result<Customer>;

    /// Delete customer
    async fn delete(&self, id: Uuid) -> Result<bool>;
}

/// Repository container - holds all repositories
#[derive(Clone)]
pub struct Repositories<P, C>
where
    P: ProductRepositoryTrait,
    C: CustomerRepositoryTrait,
{
    pub products: P,
    pub customers: C,
}

impl<P, C> Repositories<P, C>
where
    P: ProductRepositoryTrait,
    C: CustomerRepositoryTrait,
{
    pub fn new(products: P, customers: C) -> Self {
        Self {
            products,
            customers,
        }
    }
}
