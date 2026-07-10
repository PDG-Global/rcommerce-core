//! Data Import Module
//!
//! Provides functionality for importing data from various e-commerce platforms
//! and generic file formats (CSV, JSON, XML) into R Commerce.
//!
//! # Supported Platforms
//! - Shopify
//! - WooCommerce
//! - Magento
//! - Medusa.js
//!
//! # Supported File Formats
//! - CSV
//! - JSON
//! - XML

pub mod error;
pub mod formats;
pub mod platforms;
pub mod types;

pub use error::{ImportError, ImportResult};
pub use types::{ImportConfig, ImportProgress, ImportStats};

use async_trait::async_trait;
use std::path::Path;

/// Progress callback type
pub type ProgressCallback = Box<dyn Fn(ImportProgress) + Send + Sync>;

/// Trait for platform-specific importers
#[async_trait]
pub trait PlatformImporter: Send + Sync {
    /// Platform name (e.g., "shopify", "woocommerce")
    fn platform(&self) -> &'static str;

    /// Import products from the platform
    async fn import_products(
        &self,
        config: &ImportConfig,
        progress: &(dyn Fn(ImportProgress) + Send + Sync),
    ) -> ImportResult<ImportStats>;

    /// Import customers from the platform
    async fn import_customers(
        &self,
        config: &ImportConfig,
        progress: &(dyn Fn(ImportProgress) + Send + Sync),
    ) -> ImportResult<ImportStats>;

    /// Import orders from the platform
    async fn import_orders(
        &self,
        config: &ImportConfig,
        progress: &(dyn Fn(ImportProgress) + Send + Sync),
    ) -> ImportResult<ImportStats>;

    /// Import all data (products, customers, orders)
    async fn import_all(
        &self,
        config: &ImportConfig,
        progress: &(dyn Fn(ImportProgress) + Send + Sync),
    ) -> ImportResult<ImportStats> {
        let mut total_stats = ImportStats::default();

        progress(ImportProgress {
            stage: "products".to_string(),
            current: 0,
            total: 3,
            message: "Importing products...".to_string(),
        });
        let product_stats = self.import_products(config, progress).await?;
        total_stats.add(&product_stats);

        progress(ImportProgress {
            stage: "customers".to_string(),
            current: 1,
            total: 3,
            message: "Importing customers...".to_string(),
        });
        let customer_stats = self.import_customers(config, progress).await?;
        total_stats.add(&customer_stats);

        progress(ImportProgress {
            stage: "orders".to_string(),
            current: 2,
            total: 3,
            message: "Importing orders...".to_string(),
        });
        let order_stats = self.import_orders(config, progress).await?;
        total_stats.add(&order_stats);

        progress(ImportProgress {
            stage: "complete".to_string(),
            current: 3,
            total: 3,
            message: "Import complete".to_string(),
        });

        Ok(total_stats)
    }
}

/// Trait for file format importers
#[async_trait]
pub trait FileImporter: Send + Sync {
    /// File format (e.g., "csv", "json", "xml")
    fn format(&self) -> &'static str;

    /// Import from a file
    async fn import_file(
        &self,
        file_path: &Path,
        entity_type: EntityType,
        config: &ImportConfig,
        progress: &(dyn Fn(ImportProgress) + Send + Sync),
    ) -> ImportResult<ImportStats>;
}

/// Types of entities that can be imported
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityType {
    Products,
    Customers,
    Orders,
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityType::Products => write!(f, "products"),
            EntityType::Customers => write!(f, "customers"),
            EntityType::Orders => write!(f, "orders"),
        }
    }
}

/// Get a platform importer by name
pub fn get_platform_importer(platform: &str) -> Option<Box<dyn PlatformImporter>> {
    match platform.to_lowercase().as_str() {
        "shopify" => Some(Box::new(platforms::shopify::ShopifyImporter::new())),
        "woocommerce" => Some(Box::new(platforms::woocommerce::WooCommerceImporter::new())),
        "magento" => Some(Box::new(platforms::magento::MagentoImporter::new())),
        "medusa" => Some(Box::new(platforms::medusa::MedusaImporter::new())),
        _ => None,
    }
}

/// Get a file importer by format
pub fn get_file_importer(format: &str) -> Option<Box<dyn FileImporter>> {
    match format.to_lowercase().as_str() {
        "csv" => Some(Box::new(formats::csv::CsvImporter::new())),
        "json" => Some(Box::new(formats::json::JsonImporter::new())),
        "xml" => Some(Box::new(formats::xml::XmlImporter::new())),
        _ => None,
    }
}
