//! WooCommerce importer implementation

use crate::import::{
    error::{ImportError, ImportResult},
    types::{ImportConfig, ImportProgress, ImportStats},
    PlatformImporter,
};
#[allow(unused_imports)]
use crate::models::{
    CreateBundleComponentRequest, CreateProductRequest, Currency, FulfillmentStatus,
    InventoryPolicy, OrderStatus, PaymentStatus, ProductImage, ProductOption, ProductOptionValue,
    ProductType, ProductVariant, UpdateProductRequest, WeightUnit,
};
use crate::repository::{Database, ProductRepository};
use crate::services::BundleService;

use async_trait::async_trait;
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

/// WooCommerce API client and importer
pub struct WooCommerceImporter {
    client: Client,
}

impl WooCommerceImporter {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    #[allow(dead_code)]
    async fn import_categories(
        &self,
        base_url: &str,
        consumer_key: &str,
        consumer_secret: &str,
        pool: &PgPool,
        stats: &mut ImportStats,
    ) -> ImportResult<HashMap<u64, Uuid>> {
        let mut map = HashMap::new();
        let url = self.api_url(base_url, "products/categories");
        let categories: Vec<WooCommerceCategory> = self
            .fetch_paginated(&url, consumer_key, consumer_secret, 0)
            .await?;

        for wc_cat in &categories {
            let id = Uuid::new_v4();
            let result = sqlx::query(
                r#"
                INSERT INTO product_categories (id, name, slug, description, is_active, created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
                ON CONFLICT (slug) DO UPDATE SET
                    name = EXCLUDED.name,
                    description = EXCLUDED.description,
                    is_active = EXCLUDED.is_active,
                    updated_at = NOW()
                RETURNING id
                "#,
            )
            .bind(id)
            .bind(&wc_cat.name)
            .bind(&wc_cat.slug)
            .bind(None::<String>)
            .bind(true)
            .fetch_one(pool)
            .await
            .map_err(|e| ImportError::Database(crate::Error::Database(e)))?;

            let category_id: Uuid = result
                .try_get("id")
                .map_err(|e| ImportError::Database(crate::Error::Database(e)))?;
            map.insert(wc_cat.id, category_id);
            tracing::info!("Imported category: {} ({})", wc_cat.name, wc_cat.slug);
        }

        stats.total += categories.len();
        Ok(map)
    }

    async fn link_product_categories(
        &self,
        pool: &PgPool,
        product_id: Uuid,
        wc_categories: &[WooCommerceCategory],
        category_map: &HashMap<u64, Uuid>,
    ) -> ImportResult<()> {
        for (idx, wc_cat) in wc_categories.iter().enumerate() {
            let Some(category_id) = category_map.get(&wc_cat.id) else {
                continue;
            };
            sqlx::query(
                r#"
                INSERT INTO product_category_relations (
                    product_id, category_id, is_primary, created_at
                )
                VALUES ($1, $2, $3, NOW())
                ON CONFLICT (product_id, category_id) DO UPDATE SET
                    is_primary = EXCLUDED.is_primary
                "#,
            )
            .bind(product_id)
            .bind(category_id)
            .bind(idx == 0)
            .execute(pool)
            .await
            .map_err(|e| ImportError::Database(crate::Error::Database(e)))?;
        }
        Ok(())
    }

    async fn import_bundle_components(
        &self,
        pool: &PgPool,
        wc_products: &[WooCommerceProduct],
        wc_id_to_uuid: &HashMap<u64, Uuid>,
        stats: &mut ImportStats,
    ) {
        let db = Database::new(pool.clone());
        let bundle_service = BundleService::new(db);

        for product in wc_products {
            if product.product_type != "bundle" || product.bundled_items.is_empty() {
                continue;
            }

            let Some(bundle_product_id) = wc_id_to_uuid.get(&product.id) else {
                tracing::warn!(
                    "Bundle product {} not found in import map; skipping component linking",
                    product.id
                );
                continue;
            };

            for (sort_order, bundled_item) in product.bundled_items.iter().enumerate() {
                let Some(component_product_id) = wc_id_to_uuid.get(&bundled_item.product_id) else {
                    tracing::warn!(
                        "Bundle {} references component product {} which was not imported; skipping",
                        product.id,
                        bundled_item.product_id
                    );
                    continue;
                };

                let request = CreateBundleComponentRequest {
                    component_product_id: *component_product_id,
                    quantity: bundled_item.quantity.max(1),
                    is_optional: bundled_item.optional,
                    sort_order: sort_order as i32,
                };

                match bundle_service
                    .add_component(*bundle_product_id, request)
                    .await
                {
                    Ok(_) => {
                        tracing::info!(
                            "Linked bundle {} to component {} (qty {})",
                            product.id,
                            bundled_item.product_id,
                            bundled_item.quantity
                        );
                    }
                    Err(e) => {
                        let error_msg = format!(
                            "Failed to link bundle {} component {}: {}",
                            product.id, bundled_item.product_id, e
                        );
                        tracing::error!("{}", error_msg);
                        stats.errors += 1;
                        stats.error_details.push(error_msg);
                    }
                }
            }
        }
    }

    /// Get WooCommerce API URL
    fn api_url(&self, base_url: &str, endpoint: &str) -> String {
        format!(
            "{}/wp-json/wc/v3/{}",
            base_url.trim_end_matches('/'),
            endpoint
        )
    }

    /// Fetch WordPress password hash for a user
    ///
    /// Note: WooCommerce REST API doesn't expose passwords for security reasons.
    /// This method attempts to fetch from a custom endpoint or returns None.
    ///
    /// For full password migration, you have two options:
    /// 1. Direct MySQL access: Query wp_users table for user_pass field
    /// 2. Custom WordPress plugin: Create a secure endpoint that returns password hashes
    ///
    /// Without password import, users will need to reset their passwords on first login.
    async fn fetch_wordpress_password_hash(
        &self,
        _base_url: &str,
        _user_id: u64,
        _consumer_key: &str,
        _consumer_secret: &str,
    ) -> Option<String> {
        // TODO: Implement direct MySQL connection or custom WordPress plugin
        // For now, return None which will create accounts without passwords
        // Users will need to use "Forgot Password" to set a new password
        None
    }

    /// Create database pool from config
    async fn create_pool(&self, database_url: &str) -> ImportResult<PgPool> {
        use sqlx::postgres::PgPoolOptions;

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .map_err(|e| ImportError::Database(crate::Error::Database(e)))?;

        Ok(pool)
    }

    /// Fetch paginated data from WooCommerce
    async fn fetch_paginated<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        consumer_key: &str,
        consumer_secret: &str,
        limit: usize,
    ) -> ImportResult<Vec<T>> {
        let mut results = Vec::new();
        let mut page = 1;
        // WooCommerce requires per_page between 1-100, default to 100 if no limit
        let per_page = if limit == 0 { 100 } else { 100.min(limit) };

        loop {
            let request_url = format!("{}?page={}&per_page={}", url, page, per_page);

            let response = self
                .client
                .get(&request_url)
                .basic_auth(consumer_key, Some(consumer_secret))
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();

                // Provide helpful error messages for common issues
                let help_msg = if status == 404 {
                    format!(
                        "\n\nHelp: The WooCommerce REST API endpoint was not found (404).\n\
                        Common causes:\n\
                        1. Incorrect API URL - verify your store URL (e.g., https://example.com)\n\
                        2. WooCommerce REST API not enabled - enable in WP Admin > WooCommerce > Settings > Advanced > REST API\n\
                        3. Permalinks not configured - set to 'Post name' in WP Admin > Settings > Permalinks\n\
                        4. Pretty permalinks not supported - try adding ?rest_route=/wc/v3/ to your URL\n\
                        Requested URL: {}",
                        request_url
                    )
                } else if status == 401 {
                    "\n\nHelp: Authentication failed. Verify your consumer key and secret are correct.".to_string()
                } else {
                    String::new()
                };

                return Err(ImportError::Api(format!(
                    "WooCommerce API error: {} - {}{}",
                    status, body, help_msg
                )));
            }

            // Parse JSON with better error handling
            let body_text = response.text().await?;
            let items: Vec<T> = match serde_json::from_str(&body_text) {
                Ok(items) => items,
                Err(e) => {
                    // Try to parse as error response
                    if let Ok(error_json) = serde_json::from_str::<serde_json::Value>(&body_text) {
                        if let Some(code) = error_json.get("code").and_then(|c| c.as_str()) {
                            return Err(ImportError::Api(format!(
                                "WooCommerce API returned error: {} - {}",
                                code,
                                error_json
                                    .get("message")
                                    .and_then(|m| m.as_str())
                                    .unwrap_or("Unknown error")
                            )));
                        }
                    }

                    return Err(ImportError::Api(format!(
                        "Failed to parse WooCommerce response: {}. Response preview: {:.200}",
                        e, body_text
                    )));
                }
            };
            let item_count = items.len();

            results.extend(items);

            if limit > 0 && results.len() >= limit {
                results.truncate(limit);
                break;
            }

            if item_count < per_page {
                break;
            }

            page += 1;
        }

        Ok(results)
    }

    /// Convert WooCommerce price string to Decimal
    fn parse_price(&self, price: &str) -> Decimal {
        price.parse::<Decimal>().unwrap_or(Decimal::ZERO)
    }

    /// Determine effective stock quantity from WooCommerce fields.
    /// Falls back to a default when manage_stock is false and the product is in stock.
    fn effective_stock_quantity(&self, product: &WooCommerceProduct) -> i32 {
        if let Some(qty) = product.stock_quantity {
            return qty;
        }
        if product.manage_stock {
            return 0;
        }
        match product.stock_status.as_deref() {
            Some("instock") | Some("onbackorder") => 100,
            _ => 0,
        }
    }

    /// Map WooCommerce status to OrderStatus
    fn map_order_status(&self, status: &str) -> OrderStatus {
        match status {
            "pending" => OrderStatus::Pending,
            "processing" => OrderStatus::Processing,
            "on-hold" => OrderStatus::OnHold,
            "completed" => OrderStatus::Completed,
            "cancelled" => OrderStatus::Cancelled,
            "refunded" => OrderStatus::Refunded,
            "failed" => OrderStatus::Cancelled,
            _ => OrderStatus::Pending,
        }
    }

    /// Map WooCommerce currency string to Currency
    fn parse_currency(&self, currency: &str) -> Currency {
        match currency {
            "USD" => Currency::USD,
            "EUR" => Currency::EUR,
            "GBP" => Currency::GBP,
            "JPY" => Currency::JPY,
            "AUD" => Currency::AUD,
            "CAD" => Currency::CAD,
            "CNY" => Currency::CNY,
            "HKD" => Currency::HKD,
            "SGD" => Currency::SGD,
            _ => Currency::USD, // Default to USD
        }
    }

    /// Strip page-builder shortcodes (WPBakery `[vc_*]`, WooCommerce blocks,
    /// etc.) from imported text. Paired shortcodes keep their inner content;
    /// standalone tags (which carry only builder parameters) are dropped.
    /// Also decodes a few common HTML entities and collapses blank runs.
    fn strip_shortcodes(input: &str) -> String {
        static SHORTCODE_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let re = SHORTCODE_RE.get_or_init(|| {
            regex::Regex::new(r"\[\s*/?\s*[A-Za-z][A-Za-z0-9_-]*(\s+[^\]]*)?\]").unwrap()
        });
        let stripped = re.replace_all(input, "");
        let decoded = stripped
            .replace("&nbsp;", " ")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&#8211;", "–")
            .replace("&#8212;", "—")
            .replace("&#8217;", "'")
            .replace("&#8220;", "\"")
            .replace("&#8221;", "\"");
        // Collapse runs of blank lines and trailing spaces per line
        let mut out = String::with_capacity(decoded.len());
        let mut blank_run = 0;
        for line in decoded.lines() {
            let trimmed = line.trim_end();
            if trimmed.trim().is_empty() {
                blank_run += 1;
                if blank_run <= 1 {
                    out.push('\n');
                }
            } else {
                blank_run = 0;
                out.push_str(trimmed);
                out.push('\n');
            }
        }
        out.trim().to_string()
    }

    /// Sanitize a filename derived from a URL while preserving the extension
    fn sanitize_filename(&self, url: &str) -> String {
        use std::path::Path;

        let basename = url::Url::parse(url)
            .ok()
            .and_then(|u| {
                u.path_segments()
                    .and_then(|mut segments| segments.next_back().map(|s| s.to_string()))
            })
            .unwrap_or_else(|| url.split('/').next_back().unwrap_or("image").to_string());

        let decoded = basename.replace("%20", " ");
        let path = Path::new(&decoded);
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let ext = path.extension().unwrap_or_default().to_string_lossy();

        let safe_stem: String = stem
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .trim()
            .to_string();

        let safe_ext: String = ext
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_lowercase();

        if safe_ext.is_empty() {
            safe_stem
        } else {
            format!("{}.{}", safe_stem, safe_ext)
        }
    }

    /// Download an image to local storage, returning the local path on success
    async fn download_image(
        &self,
        image_url: &str,
        dest_dir: &std::path::Path,
        filename: &str,
    ) -> Option<std::path::PathBuf> {
        use tokio::io::AsyncWriteExt;

        if let Err(e) = tokio::fs::create_dir_all(dest_dir).await {
            tracing::warn!("Failed to create image directory {:?}: {}", dest_dir, e);
            return None;
        }

        let response = match self.client.get(image_url).send().await {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                tracing::warn!(
                    "Image download failed with status {}: {}",
                    r.status(),
                    image_url
                );
                return None;
            }
            Err(e) => {
                tracing::warn!("Failed to download image {}: {}", image_url, e);
                return None;
            }
        };

        let bytes = match response.bytes().await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("Failed to read image bytes from {}: {}", image_url, e);
                return None;
            }
        };

        let dest_path = dest_dir.join(filename);
        match tokio::fs::File::create(&dest_path).await {
            Ok(mut file) => {
                if let Err(e) = file.write_all(&bytes).await {
                    tracing::warn!("Failed to write image {}: {}", dest_path.display(), e);
                    return None;
                }
                Some(dest_path)
            }
            Err(e) => {
                tracing::warn!("Failed to create image file {}: {}", dest_path.display(), e);
                None
            }
        }
    }
}

impl Default for WooCommerceImporter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformImporter for WooCommerceImporter {
    fn platform(&self) -> &'static str {
        "woocommerce"
    }

    async fn import_products(
        &self,
        config: &ImportConfig,
        progress: &(dyn Fn(ImportProgress) + Send + Sync),
    ) -> ImportResult<ImportStats> {
        let dry_run = config.options.dry_run;
        let (base_url, consumer_key, consumer_secret) = match &config.source {
            crate::import::types::SourceConfig::Platform {
                api_url,
                api_key,
                headers,
                ..
            } => {
                let secret = headers.get("consumer_secret").cloned().unwrap_or_default();
                (api_url.clone(), api_key.clone(), secret)
            }
            _ => {
                return Err(ImportError::Configuration(
                    "Invalid source configuration for WooCommerce".to_string(),
                ))
            }
        };

        progress(ImportProgress {
            stage: "products".to_string(),
            current: 0,
            total: 1,
            message: if dry_run {
                "Fetching products from WooCommerce (DRY RUN)...".to_string()
            } else {
                "Fetching products from WooCommerce...".to_string()
            },
        });

        let url = self.api_url(&base_url, "products");
        tracing::info!("WooCommerce API URL: {}", url);

        let wc_products: Vec<WooCommerceProduct> = self
            .fetch_paginated(&url, &consumer_key, &consumer_secret, config.options.limit)
            .await?;

        let mut stats = ImportStats {
            total: wc_products.len(),
            ..Default::default()
        };

        // Create database pool and repository if not dry run
        let pool = if !dry_run {
            match self.create_pool(&config.database_url).await {
                Ok(pool) => {
                    tracing::info!("Database pool created successfully");
                    Some(pool)
                }
                Err(e) => {
                    tracing::error!("Failed to create database pool: {}", e);
                    return Err(e);
                }
            }
        } else {
            tracing::info!("Dry run mode - no database connection needed");
            None
        };

        // Pre-fetch and upsert WooCommerce product categories
        let category_map: HashMap<u64, Uuid> = if !dry_run {
            if let Some(ref pool) = pool {
                let (base_url, consumer_key, consumer_secret) = match &config.source {
                    crate::import::types::SourceConfig::Platform {
                        api_url,
                        api_key,
                        headers,
                        ..
                    } => {
                        let secret = headers.get("consumer_secret").cloned().unwrap_or_default();
                        (api_url.clone(), api_key.clone(), secret)
                    }
                    _ => {
                        return Err(ImportError::Configuration(
                            "Invalid source configuration for WooCommerce".to_string(),
                        ))
                    }
                };
                match self
                    .import_categories(&base_url, &consumer_key, &consumer_secret, pool, &mut stats)
                    .await
                {
                    Ok(map) => map,
                    Err(e) => {
                        tracing::warn!("Failed to import categories: {}", e);
                        HashMap::new()
                    }
                }
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

        let mut wc_id_to_uuid: HashMap<u64, Uuid> = HashMap::new();

        for (i, product) in wc_products.iter().enumerate() {
            progress(ImportProgress {
                stage: "products".to_string(),
                current: i + 1,
                total: wc_products.len(),
                message: if dry_run {
                    format!("Validating: {}", product.name)
                } else {
                    format!("Importing: {}", product.name)
                },
            });

            // Validate product
            if product.name.is_empty() {
                stats.errors += 1;
                stats
                    .error_details
                    .push(format!("Product {} has no name", product.id));
                continue;
            }

            if dry_run {
                stats.created += 1;
            } else {
                // Create product in database using repository pattern
                if let Some(ref pool) = pool {
                    // Generate slug for lookup
                    let slug = if product.slug.is_empty() {
                        product.name.to_lowercase().replace(" ", "-")
                    } else {
                        product.slug.clone()
                    };

                    // Create repository
                    let db = Database::new(pool.clone());
                    let repo = ProductRepository::new(db);

                    // Check for existing product by slug
                    let existing = repo
                        .find_by_slug(&slug)
                        .await
                        .map_err(ImportError::Database)?;

                    // Parse price - use sale_price if available, otherwise regular_price or price
                    let price = if !product.sale_price.is_empty() && product.sale_price != "0" {
                        self.parse_price(&product.sale_price)
                    } else if !product.regular_price.is_empty() {
                        self.parse_price(&product.regular_price)
                    } else {
                        self.parse_price(&product.price)
                    };

                    let compare_at_price =
                        if !product.regular_price.is_empty() && product.sale_price != "0" {
                            let regular = self.parse_price(&product.regular_price);
                            if regular > price {
                                Some(regular)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                    // Determine product type
                    let product_type = match product.product_type.as_str() {
                        "simple" => ProductType::Simple,
                        "variable" => ProductType::Variable,
                        "subscription" => ProductType::Subscription,
                        "bundle" => ProductType::Bundle,
                        _ => ProductType::Simple,
                    };

                    if let Some(existing_product) = existing {
                        // Product exists - check if we should update
                        if config.options.update_existing {
                            // Parse default currency from config
                            let default_currency =
                                self.parse_currency(&config.options.default_currency);

                            // Update existing product using repository
                            let update_request = UpdateProductRequest {
                                title: Some(product.name.clone()),
                                slug: None, // Keep existing slug
                                description: Some(
                                    product.description.as_deref().map(Self::strip_shortcodes),
                                ),
                                sku: Some(product.sku.clone()),
                                price: Some(price),
                                compare_at_price: Some(compare_at_price),
                                cost_price: None,
                                currency: Some(default_currency),
                                inventory_quantity: Some(self.effective_stock_quantity(product)),
                                inventory_policy: None,
                                inventory_management: Some(product.manage_stock),
                                continues_selling_when_out_of_stock: None,
                                weight: Some(
                                    product
                                        .weight
                                        .as_ref()
                                        .and_then(|w| w.parse::<Decimal>().ok()),
                                ),
                                weight_unit: Some(Some(WeightUnit::Kg)),
                                requires_shipping: None,
                                is_active: Some(product.status == "publish"),
                                is_featured: None,
                                seo_title: Some(Some(product.name.clone())),
                                seo_description: Some(
                                    product
                                        .short_description
                                        .as_deref()
                                        .map(Self::strip_shortcodes),
                                ),
                                gtin: Some(product.extract_gtin()),
                                product_type: Some(product_type),
                                subscription_interval: None,
                                subscription_interval_count: None,
                                subscription_trial_days: None,
                                subscription_setup_fee: None,
                                subscription_min_cycles: None,
                                subscription_max_cycles: None,
                                file_url: None,
                                file_size: None,
                                file_hash: None,
                                download_limit: None,
                                license_key_enabled: None,
                                download_expiry_days: None,
                                bundle_pricing_strategy: None,
                                bundle_discount_percentage: None,
                            };

                            match repo
                                .update_with_request(existing_product.id, update_request)
                                .await
                            {
                                Ok(_) => {
                                    stats.updated += 1;
                                    wc_id_to_uuid.insert(product.id, existing_product.id);
                                    tracing::info!("Updated product: {}", product.name);

                                    // Link product to categories
                                    if !category_map.is_empty() && !product.categories.is_empty() {
                                        if let Err(e) = self
                                            .link_product_categories(
                                                pool,
                                                existing_product.id,
                                                &product.categories,
                                                &category_map,
                                            )
                                            .await
                                        {
                                            tracing::warn!(
                                                "Failed to link categories for product '{}': {}",
                                                product.name,
                                                e
                                            );
                                        }
                                    }

                                    // Ensure inventory levels are in sync for non-variable products
                                    if product_type != ProductType::Variable {
                                        self.create_inventory_level(
                                            pool,
                                            existing_product.id,
                                            None,
                                            self.effective_stock_quantity(product),
                                            &mut stats,
                                        )
                                        .await;
                                    }
                                }
                                Err(e) => {
                                    stats.errors += 1;
                                    let error_msg = format!(
                                        "Failed to update product '{}': {}",
                                        product.name, e
                                    );
                                    tracing::error!("{}", error_msg);
                                    stats.error_details.push(error_msg);
                                    if !config.options.continue_on_error {
                                        break;
                                    }
                                }
                            }
                        } else {
                            // Skip existing product
                            tracing::info!("Product with slug '{}' already exists, skipping", slug);
                            stats.skipped += 1;
                            continue;
                        }
                    } else {
                        // Create new product
                        // Parse default currency from config
                        let default_currency =
                            self.parse_currency(&config.options.default_currency);

                        let create_request = CreateProductRequest {
                            title: product.name.clone(),
                            slug: if product.slug.is_empty() {
                                product.name.to_lowercase().replace(" ", "-")
                            } else {
                                product.slug.clone()
                            },
                            description: product.description.as_deref().map(Self::strip_shortcodes),
                            sku: product.sku.clone(),
                            product_type,
                            price,
                            compare_at_price,
                            cost_price: None,
                            currency: default_currency, // Use configured default currency
                            inventory_quantity: self.effective_stock_quantity(product),
                            inventory_policy: InventoryPolicy::Deny,
                            inventory_management: product.manage_stock,
                            continues_selling_when_out_of_stock: false,
                            weight: product
                                .weight
                                .as_ref()
                                .and_then(|w| w.parse::<Decimal>().ok()),
                            weight_unit: Some(WeightUnit::Kg),
                            requires_shipping: product_type != ProductType::Digital,
                            is_active: product.status == "publish",
                            is_featured: false,
                            seo_title: Some(product.name.clone()),
                            seo_description: product
                                .short_description
                                .as_deref()
                                .map(Self::strip_shortcodes),
                            gtin: product.extract_gtin(),
                            subscription_interval: None,
                            subscription_interval_count: None,
                            subscription_trial_days: None,
                            subscription_setup_fee: None,
                            subscription_min_cycles: None,
                            subscription_max_cycles: None,
                            file_url: None,
                            file_size: None,
                            file_hash: None,
                            download_limit: None,
                            license_key_enabled: None,
                            download_expiry_days: None,
                            bundle_pricing_strategy: None,
                            bundle_discount_percentage: None,
                            attributes: None,
                            bundle_components: None,
                        };

                        match repo.create_with_request(create_request).await {
                            Ok(created_product) => {
                                stats.created += 1;
                                wc_id_to_uuid.insert(product.id, created_product.id);
                                tracing::info!(
                                    "Created product: {} (type: {:?}, wc_type: {})",
                                    product.name,
                                    product_type,
                                    product.product_type
                                );

                                // Ensure product_type is persisted correctly. Some repository
                                // paths may not propagate the enum value, so force an update.
                                if let Err(e) = sqlx::query(
                                    "UPDATE products SET product_type = $1 WHERE id = $2",
                                )
                                .bind(product_type)
                                .bind(created_product.id)
                                .execute(pool)
                                .await
                                {
                                    tracing::warn!(
                                        "Failed to update product_type for {}: {}",
                                        product.name,
                                        e
                                    );
                                }

                                // Persist product images
                                if !product.images.is_empty() {
                                    self.create_product_images(
                                        pool,
                                        created_product.id,
                                        None,
                                        &product.images,
                                        &mut stats,
                                    )
                                    .await;
                                }

                                // Link product to categories
                                if !category_map.is_empty() && !product.categories.is_empty() {
                                    if let Err(e) = self
                                        .link_product_categories(
                                            pool,
                                            created_product.id,
                                            &product.categories,
                                            &category_map,
                                        )
                                        .await
                                    {
                                        tracing::warn!(
                                            "Failed to link categories for product '{}': {}",
                                            product.name,
                                            e
                                        );
                                    }
                                }

                                if product_type == ProductType::Variable {
                                    if let Err(e) = self
                                        .import_product_variations(
                                            &base_url,
                                            &consumer_key,
                                            &consumer_secret,
                                            product,
                                            created_product.id,
                                            pool,
                                            default_currency,
                                            &mut stats,
                                        )
                                        .await
                                    {
                                        stats.errors += 1;
                                        let error_msg = format!(
                                            "Failed to import variations for product '{}': {}",
                                            product.name, e
                                        );
                                        tracing::error!("{}", error_msg);
                                        stats.error_details.push(error_msg);
                                        if !config.options.continue_on_error {
                                            break;
                                        }
                                    }
                                } else {
                                    // Create inventory level for all non-variable product
                                    // types (simple, bundle, subscription, digital) so
                                    // checkout can reserve stock against them.
                                    self.create_inventory_level(
                                        pool,
                                        created_product.id,
                                        None,
                                        self.effective_stock_quantity(product),
                                        &mut stats,
                                    )
                                    .await;
                                }
                            }
                            Err(e) => {
                                stats.errors += 1;
                                let error_msg =
                                    format!("Failed to create product '{}': {}", product.name, e);
                                tracing::error!("{}", error_msg);
                                stats.error_details.push(error_msg);
                                if !config.options.continue_on_error {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Second pass: create bundle component links for any bundle products.
        // Components must be imported after all products so that referenced IDs exist.
        if !dry_run {
            if let Some(ref pool) = pool {
                self.import_bundle_components(pool, &wc_products, &wc_id_to_uuid, &mut stats)
                    .await;
            }
        }

        Ok(stats)
    }

    async fn import_customers(
        &self,
        config: &ImportConfig,
        progress: &(dyn Fn(ImportProgress) + Send + Sync),
    ) -> ImportResult<ImportStats> {
        let dry_run = config.options.dry_run;
        let (base_url, consumer_key, consumer_secret) = match &config.source {
            crate::import::types::SourceConfig::Platform {
                api_url,
                api_key,
                headers,
                ..
            } => {
                let secret = headers.get("consumer_secret").cloned().unwrap_or_default();
                (api_url.clone(), api_key.clone(), secret)
            }
            _ => {
                return Err(ImportError::Configuration(
                    "Invalid source configuration for WooCommerce".to_string(),
                ))
            }
        };

        progress(ImportProgress {
            stage: "customers".to_string(),
            current: 0,
            total: 1,
            message: if dry_run {
                "Fetching customers from WooCommerce (DRY RUN)...".to_string()
            } else {
                "Fetching customers from WooCommerce...".to_string()
            },
        });

        let url = self.api_url(&base_url, "customers");
        let wc_customers: Vec<WooCommerceCustomer> = self
            .fetch_paginated(&url, &consumer_key, &consumer_secret, config.options.limit)
            .await?;

        let mut stats = ImportStats {
            total: wc_customers.len(),
            ..Default::default()
        };

        // Create database pool and repository if not dry run
        let pool = if !dry_run {
            Some(self.create_pool(&config.database_url).await?)
        } else {
            None
        };

        for (i, customer) in wc_customers.iter().enumerate() {
            progress(ImportProgress {
                stage: "customers".to_string(),
                current: i + 1,
                total: wc_customers.len(),
                message: if dry_run {
                    format!("Validating customer {}/{}", i + 1, wc_customers.len())
                } else {
                    format!("Importing customer {}/{}", i + 1, wc_customers.len())
                },
            });

            // Validate customer
            if customer.email.is_empty() {
                stats.errors += 1;
                stats
                    .error_details
                    .push(format!("Customer {} has no email", customer.id));
                continue;
            }

            if dry_run {
                stats.created += 1;
            } else {
                // Create customer in database
                if let Some(ref pool) = pool {
                    // Check for existing customer by email (using raw SQL to avoid role column issue)
                    let existing: Option<(Uuid,)> =
                        sqlx::query_as("SELECT id FROM customers WHERE email = $1")
                            .bind(&customer.email)
                            .fetch_optional(pool)
                            .await
                            .map_err(|e| ImportError::Database(crate::Error::Database(e)))?;

                    // Handle existing customer
                    if let Some((existing_id,)) = existing {
                        if config.options.skip_existing {
                            stats.skipped += 1;
                            continue;
                        }
                        if config.options.update_existing {
                            // Update existing customer
                            let first_name = if customer.first_name.is_empty() {
                                customer.username.clone()
                            } else {
                                customer.first_name.clone()
                            };
                            let last_name = customer.last_name.clone();
                            let phone = customer.billing.as_ref().and_then(|b| b.phone.clone());
                            let default_currency =
                                self.parse_currency(&config.options.default_currency);

                            match sqlx::query(
                                r#"
                                UPDATE customers 
                                SET first_name = $1, last_name = $2, phone = $3, currency = $4, updated_at = NOW()
                                WHERE id = $5
                                "#
                            )
                            .bind(&first_name)
                            .bind(&last_name)
                            .bind(&phone)
                            .bind(default_currency)
                            .bind(existing_id)
                            .execute(pool)
                            .await {
                                Ok(_) => {
                                    stats.updated += 1;
                                    tracing::info!("Updated customer: {}", customer.email);
                                }
                                Err(e) => {
                                    stats.errors += 1;
                                    stats.error_details.push(format!(
                                        "Failed to update customer '{}': {}",
                                        customer.email, e
                                    ));
                                }
                            }
                            continue;
                        }
                        stats.skipped += 1;
                        continue;
                    }

                    // Create customer using raw SQL (without role column)
                    let customer_id = Uuid::new_v4();
                    let first_name = if customer.first_name.is_empty() {
                        customer.username.clone()
                    } else {
                        customer.first_name.clone()
                    };
                    let last_name = customer.last_name.clone();
                    let phone = customer.billing.as_ref().and_then(|b| b.phone.clone());

                    // Parse default currency from config
                    let default_currency = self.parse_currency(&config.options.default_currency);

                    // Import password hash if available from WordPress
                    // Note: WooCommerce REST API doesn't expose passwords, so we set a placeholder
                    // that forces password reset on first login
                    let password_hash = self
                        .fetch_wordpress_password_hash(
                            &base_url,
                            customer.id,
                            &consumer_key,
                            &consumer_secret,
                        )
                        .await;

                    match sqlx::query(
                        r#"
                        INSERT INTO customers (
                            id, email, first_name, last_name, phone, accepts_marketing, 
                            tax_exempt, currency, is_verified, marketing_opt_in,
                            email_notifications, sms_notifications, push_notifications,
                            password_hash, is_imported, created_at, updated_at
                        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, NOW(), NOW())
                        "#
                    )
                    .bind(customer_id)
                    .bind(&customer.email)
                    .bind(&first_name)
                    .bind(&last_name)
                    .bind(&phone)
                    .bind(false)  // accepts_marketing
                    .bind(false)  // tax_exempt
                    .bind(default_currency)  // currency - use configured default
                    .bind(false)  // is_verified
                    .bind(false)  // marketing_opt_in
                    .bind(true)   // email_notifications
                    .bind(false)  // sms_notifications
                    .bind(false)  // push_notifications
                    .bind(password_hash)  // password_hash from WordPress (if available)
                    .bind(true)   // is_imported
                    .execute(pool)
                    .await {
                        Ok(_) => {
                            stats.created += 1;
                            tracing::info!("Created customer: {}", customer.email);

                            // Create addresses if available
                            // Billing address
                            if let Some(ref billing) = customer.billing {
                                if let Err(e) = self.create_address(pool, customer_id, billing, true, false).await {
                                    stats.error_details.push(format!(
                                        "Failed to create billing address for customer '{}': {}",
                                        customer.email, e
                                    ));
                                }
                            }

                            // Shipping address
                            if let Some(ref shipping) = customer.shipping {
                                let is_default_shipping = customer.billing.is_none() ||
                                    (shipping.address_1 != customer.billing.as_ref().unwrap().address_1);
                                if let Err(e) = self.create_address(pool, customer_id, shipping, false, is_default_shipping).await {
                                    stats.error_details.push(format!(
                                        "Failed to create shipping address for customer '{}': {}",
                                        customer.email, e
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            stats.errors += 1;
                            let error_msg = format!("Failed to create customer '{}': {}", customer.email, e);
                            tracing::error!("{}", error_msg);
                            stats.error_details.push(error_msg);
                            if !config.options.continue_on_error {
                                break;
                            }
                        }
                    }
                }
            }
        }

        Ok(stats)
    }

    async fn import_orders(
        &self,
        config: &ImportConfig,
        progress: &(dyn Fn(ImportProgress) + Send + Sync),
    ) -> ImportResult<ImportStats> {
        let dry_run = config.options.dry_run;
        let (base_url, consumer_key, consumer_secret) = match &config.source {
            crate::import::types::SourceConfig::Platform {
                api_url,
                api_key,
                headers,
                ..
            } => {
                let secret = headers.get("consumer_secret").cloned().unwrap_or_default();
                (api_url.clone(), api_key.clone(), secret)
            }
            _ => {
                return Err(ImportError::Configuration(
                    "Invalid source configuration for WooCommerce".to_string(),
                ))
            }
        };

        progress(ImportProgress {
            stage: "orders".to_string(),
            current: 0,
            total: 1,
            message: if dry_run {
                "Fetching orders from WooCommerce (DRY RUN)...".to_string()
            } else {
                "Fetching orders from WooCommerce...".to_string()
            },
        });

        let url = self.api_url(&base_url, "orders");
        let wc_orders: Vec<WooCommerceOrder> = self
            .fetch_paginated(&url, &consumer_key, &consumer_secret, config.options.limit)
            .await?;

        let mut stats = ImportStats {
            total: wc_orders.len(),
            ..Default::default()
        };

        // Create database pool if not dry run
        let pool = if !dry_run {
            Some(self.create_pool(&config.database_url).await?)
        } else {
            None
        };

        for (i, order) in wc_orders.iter().enumerate() {
            progress(ImportProgress {
                stage: "orders".to_string(),
                current: i + 1,
                total: wc_orders.len(),
                message: if dry_run {
                    format!("Validating order {}/{}", i + 1, wc_orders.len())
                } else {
                    format!("Importing order {}/{}", i + 1, wc_orders.len())
                },
            });

            // Validate order
            if order.id == 0 {
                stats.errors += 1;
                stats
                    .error_details
                    .push(format!("Order at index {} has invalid ID", i));
                continue;
            }

            if dry_run {
                stats.created += 1;
            } else {
                // Create order in database
                if let Some(ref pool) = pool {
                    // Get customer email from billing or use placeholder
                    let customer_email = order
                        .billing
                        .as_ref()
                        .and_then(|b| b.email.clone())
                        .unwrap_or_else(|| "unknown@example.com".to_string());

                    // Look up customer by email (needed for both create and update)
                    let customer_id = if !customer_email.is_empty()
                        && customer_email != "unknown@example.com"
                    {
                        sqlx::query_scalar::<_, Uuid>("SELECT id FROM customers WHERE email = $1")
                            .bind(&customer_email)
                            .fetch_optional(pool)
                            .await
                            .map_err(|e| ImportError::Database(crate::Error::Database(e)))?
                    } else {
                        None
                    };

                    // Parse totals
                    let total = self.parse_price(&order.total);
                    let tax_total = order
                        .total_tax
                        .as_ref()
                        .map(|t| self.parse_price(t))
                        .unwrap_or(Decimal::ZERO);
                    let shipping_total = order
                        .shipping_total
                        .as_ref()
                        .map(|s| self.parse_price(s))
                        .unwrap_or(Decimal::ZERO);
                    let discount_total = order
                        .discount_total
                        .as_ref()
                        .map(|d| self.parse_price(d))
                        .unwrap_or(Decimal::ZERO);
                    let subtotal = total - tax_total - shipping_total + discount_total;

                    // Map status
                    let status = self.map_order_status(&order.status);
                    let payment_status = match order.status.as_str() {
                        "completed" | "processing" => PaymentStatus::Paid,
                        "on-hold" => PaymentStatus::Pending,
                        "pending" => PaymentStatus::Pending,
                        "cancelled" => PaymentStatus::Cancelled,
                        "refunded" => PaymentStatus::Refunded,
                        "failed" => PaymentStatus::Failed,
                        _ => PaymentStatus::Pending,
                    };
                    let fulfillment_status = match order.status.as_str() {
                        "completed" => FulfillmentStatus::Delivered,
                        "processing" => FulfillmentStatus::Processing,
                        "on-hold" => FulfillmentStatus::Pending,
                        "pending" => FulfillmentStatus::Pending,
                        "cancelled" => FulfillmentStatus::Cancelled,
                        _ => FulfillmentStatus::Pending,
                    };

                    let currency = self.parse_currency(&order.currency);

                    // Generate order number
                    let order_number = order
                        .order_number
                        .clone()
                        .unwrap_or_else(|| format!("WC-{}", order.id));

                    // Check if order already exists
                    let existing_order: Option<Uuid> =
                        sqlx::query_scalar("SELECT id FROM orders WHERE order_number = $1")
                            .bind(&order_number)
                            .fetch_optional(pool)
                            .await
                            .map_err(|e| ImportError::Database(crate::Error::Database(e)))?;

                    if let Some(existing_order_id) = existing_order {
                        if config.options.skip_existing {
                            stats.skipped += 1;
                            continue;
                        }
                        if config.options.update_existing {
                            // Delete existing order items first
                            let _ = sqlx::query("DELETE FROM order_items WHERE order_id = $1")
                                .bind(existing_order_id)
                                .execute(pool)
                                .await;

                            // Update existing order
                            match sqlx::query(
                                r#"
                                UPDATE orders 
                                SET customer_id = $1, customer_email = $2, currency = $3, status = $4,
                                    fulfillment_status = $5, payment_status = $6, subtotal = $7,
                                    tax_total = $8, shipping_total = $9, discount_total = $10, total = $11,
                                    updated_at = NOW()
                                WHERE id = $12
                                "#
                            )
                            .bind(customer_id)
                            .bind(&customer_email)
                            .bind(currency)
                            .bind(status)
                            .bind(fulfillment_status)
                            .bind(payment_status)
                            .bind(subtotal)
                            .bind(tax_total)
                            .bind(shipping_total)
                            .bind(discount_total)
                            .bind(total)
                            .bind(existing_order_id)
                            .execute(pool)
                            .await {
                                Ok(_) => {
                                    // Re-create order items
                                    if let Some(ref line_items) = order.line_items {
                                        for item in line_items {
                                            let item_price = self.parse_price(&item.subtotal) / Decimal::from(item.quantity.max(1));
                                            let item_subtotal = self.parse_price(&item.subtotal);
                                            let item_total = self.parse_price(&item.total);
                                            let item_tax = item_total - item_subtotal;

                                            // Look up product by SKU first, then by name
                                            let product_id = if let Some(ref sku) = item.sku {
                                                self.lookup_product_by_sku(pool, sku).await.ok().flatten()
                                            } else {
                                                None
                                            };

                                            let product_id = match product_id {
                                                Some(id) => Some(id),
                                                None => self.lookup_product_by_name(pool, &item.name).await.ok().flatten(),
                                            };

                                            let product_id = match product_id {
                                                Some(id) => id,
                                                None => continue,
                                            };

                                            let _ = sqlx::query(
                                                r#"
                                                INSERT INTO order_items (
                                                    id, order_id, product_id, variant_id,
                                                    quantity, price, subtotal, tax_amount, total,
                                                    sku, title, variant_title, requires_shipping,
                                                    created_at, updated_at
                                                )
                                                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, NOW(), NOW())
                                                "#
                                            )
                                            .bind(Uuid::new_v4())
                                            .bind(existing_order_id)
                                            .bind(product_id)
                                            .bind(None::<Uuid>)
                                            .bind(item.quantity)
                                            .bind(item_price)
                                            .bind(item_subtotal)
                                            .bind(item_tax)
                                            .bind(item_total)
                                            .bind(item.sku.clone())
                                            .bind(&item.name)
                                            .bind(None::<String>)
                                            .bind(true)
                                            .execute(pool)
                                            .await;
                                        }
                                    }

                                    stats.updated += 1;
                                    tracing::info!("Updated order: {}", order_number);
                                }
                                Err(e) => {
                                    stats.errors += 1;
                                    stats.error_details.push(format!(
                                        "Failed to update order {}: {}",
                                        order.id, e
                                    ));
                                }
                            }
                            continue;
                        }
                        stats.skipped += 1;
                        continue;
                    }

                    // Create addresses and get their IDs
                    let mut billing_address_id: Option<Uuid> = None;
                    let mut shipping_address_id: Option<Uuid> = None;

                    // Create billing address (with customer_id if found)
                    if let Some(ref billing) = order.billing {
                        match self
                            .create_address_for_order(pool, customer_id, billing)
                            .await
                        {
                            Ok(id) => billing_address_id = Some(id),
                            Err(e) => {
                                stats.error_details.push(format!(
                                    "Failed to create billing address for order {}: {}",
                                    order.id, e
                                ));
                            }
                        }
                    }

                    // Create shipping address (with customer_id if found)
                    if let Some(ref shipping) = order.shipping {
                        match self
                            .create_address_for_order(pool, customer_id, shipping)
                            .await
                        {
                            Ok(id) => shipping_address_id = Some(id),
                            Err(e) => {
                                stats.error_details.push(format!(
                                    "Failed to create shipping address for order {}: {}",
                                    order.id, e
                                ));
                            }
                        }
                    }

                    // Insert order (preserving the original Woo created_at)
                    let order_id = Uuid::new_v4();
                    let imported_created_at = parse_wc_datetime(order.date_created.as_deref());
                    let imported_updated_at = order
                        .date_modified
                        .as_deref()
                        .map(|v| parse_wc_datetime(Some(v)))
                        .unwrap_or(imported_created_at);
                    let result = sqlx::query(
                        r#"
                        INSERT INTO orders (
                            id, order_number, customer_id, customer_email,
                            currency, status, fulfillment_status, payment_status,
                            subtotal, tax_total, shipping_total, discount_total, total,
                            billing_address_id, shipping_address_id,
                            notes, tags, draft, created_at, updated_at
                        )
                        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
                        "#
                    )
                    .bind(order_id)
                    .bind(&order_number)
                    .bind(customer_id) // Use looked-up customer_id
                    .bind(&customer_email)
                    .bind(currency.to_string())  // Use string for varchar(3) column
                    .bind(status)  // Use OrderStatus enum directly
                    .bind(fulfillment_status)  // Use FulfillmentStatus enum directly
                    .bind(payment_status)  // Use PaymentStatus enum directly
                    .bind(subtotal)
                    .bind(tax_total)
                    .bind(shipping_total)
                    .bind(discount_total)
                    .bind(total)
                    .bind(billing_address_id)
                    .bind(shipping_address_id)
                    .bind(None::<String>) // notes
                    .bind(&[] as &[String]) // tags
                    .bind(false) // draft
                    .bind(imported_created_at)
                    .bind(imported_updated_at)
                    .execute(pool)
                    .await;

                    match result {
                        Ok(_) => {
                            // Create order items
                            if let Some(ref line_items) = order.line_items {
                                for item in line_items {
                                    let item_price = self.parse_price(&item.subtotal)
                                        / Decimal::from(item.quantity.max(1));
                                    let item_subtotal = self.parse_price(&item.subtotal);
                                    let item_total = self.parse_price(&item.total);
                                    let item_tax = item_total - item_subtotal;

                                    // Look up product by SKU first, then by name
                                    let product_id = if let Some(ref sku) = item.sku {
                                        self.lookup_product_by_sku(pool, sku).await.ok().flatten()
                                    } else {
                                        None
                                    };

                                    // Fallback to lookup by name if SKU not found.
                                    // WooCommerce order line names often carry the
                                    // variation suffix ("... Wheels - 74A") which is
                                    // not part of the product title — strip trailing
                                    // " - X" segments (up to two) and retry.
                                    let mut product_id = product_id;
                                    if product_id.is_none() {
                                        let mut candidate = item.name.as_str();
                                        for _ in 0..3 {
                                            match self.lookup_product_by_name(pool, candidate).await
                                            {
                                                Ok(Some(id)) => {
                                                    product_id = Some(id);
                                                    break;
                                                }
                                                _ => {}
                                            }
                                            match candidate.rfind(" - ") {
                                                Some(idx) => candidate = &candidate[..idx],
                                                None => break,
                                            }
                                        }
                                    }

                                    // Skip order item if product not found
                                    let product_id = match product_id {
                                        Some(id) => id,
                                        None => {
                                            stats.error_details.push(format!(
                                                "Order item '{}' not found in product catalog for order {}",
                                                item.name, order.id
                                            ));
                                            continue;
                                        }
                                    };

                                    let item_result = sqlx::query(
                                        r#"
                                        INSERT INTO order_items (
                                            id, order_id, product_id, variant_id,
                                            quantity, price, subtotal, tax_amount, total,
                                            sku, title, variant_title, requires_shipping,
                                            created_at, updated_at
                                        )
                                        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, NOW(), NOW())
                                        "#
                                    )
                                    .bind(Uuid::new_v4())
                                    .bind(order_id)
                                    .bind(product_id)
                                    .bind(None::<Uuid>) // variant_id
                                    .bind(item.quantity)
                                    .bind(item_price)
                                    .bind(item_subtotal)
                                    .bind(item_tax)
                                    .bind(item_total)
                                    .bind(item.sku.clone())
                                    .bind(&item.name)
                                    .bind(None::<String>) // variant_title
                                    .bind(true) // requires_shipping
                                    .execute(pool)
                                    .await;

                                    if let Err(e) = item_result {
                                        stats.error_details.push(format!(
                                            "Failed to create order item for order {}: {}",
                                            order.id, e
                                        ));
                                    }
                                }
                            }

                            stats.created += 1;
                        }
                        Err(e) => {
                            stats.errors += 1;
                            stats
                                .error_details
                                .push(format!("Failed to create order {}: {}", order.id, e));
                            if !config.options.continue_on_error {
                                break;
                            }
                        }
                    }
                }
            }
        }

        Ok(stats)
    }

    async fn import_reviews(
        &self,
        config: &ImportConfig,
        progress: &(dyn Fn(ImportProgress) + Send + Sync),
    ) -> ImportResult<ImportStats> {
        let dry_run = config.options.dry_run;
        let (base_url, consumer_key, consumer_secret) = match &config.source {
            crate::import::types::SourceConfig::Platform {
                api_url,
                api_key,
                headers,
                ..
            } => {
                let secret = headers.get("consumer_secret").cloned().unwrap_or_default();
                (api_url.clone(), api_key.clone(), secret)
            }
            _ => {
                return Err(ImportError::Configuration(
                    "Invalid source configuration for WooCommerce".to_string(),
                ))
            }
        };

        progress(ImportProgress {
            stage: "reviews".to_string(),
            current: 0,
            total: 1,
            message: if dry_run {
                "Fetching product reviews from WooCommerce (DRY RUN)...".to_string()
            } else {
                "Fetching product reviews from WooCommerce...".to_string()
            },
        });

        let url = self.api_url(&base_url, "products/reviews");
        let wc_reviews: Vec<WooCommerceReview> = self
            .fetch_paginated(&url, &consumer_key, &consumer_secret, config.options.limit)
            .await?;

        let mut stats = ImportStats {
            total: wc_reviews.len(),
            ..Default::default()
        };

        if dry_run {
            stats.created = wc_reviews.len();
            return Ok(stats);
        }

        let pool = self.create_pool(&config.database_url).await?;

        // Build a WooCommerce product id -> rCommerce product id mapping.
        // Imported products keep their WooCommerce slug, and SKUs are carried
        // over as well, so match by SKU first, then slug, then title.
        let wc_products: Vec<WooCommerceProduct> = self
            .fetch_paginated(
                &self.api_url(&base_url, "products"),
                &consumer_key,
                &consumer_secret,
                0,
            )
            .await?;

        let local_products: Vec<(Uuid, Option<String>, String, String)> =
            sqlx::query_as("SELECT id, sku, slug, title FROM products")
                .fetch_all(&pool)
                .await
                .map_err(|e| ImportError::Database(crate::Error::Database(e)))?;

        let mut by_sku: HashMap<String, Uuid> = HashMap::new();
        let mut by_slug: HashMap<String, Uuid> = HashMap::new();
        let mut by_title: HashMap<String, Uuid> = HashMap::new();
        for (id, sku, slug, title) in &local_products {
            if let Some(sku) = sku {
                by_sku.insert(sku.clone(), *id);
            }
            by_slug.insert(slug.clone(), *id);
            by_title.insert(title.clone(), *id);
        }

        let mut wc_id_to_uuid: HashMap<u64, Uuid> = HashMap::new();
        for wc_product in &wc_products {
            let mapped = wc_product
                .sku
                .as_ref()
                .and_then(|sku| by_sku.get(sku).copied())
                .or_else(|| by_slug.get(&wc_product.slug).copied())
                .or_else(|| by_title.get(&wc_product.name).copied());
            if let Some(id) = mapped {
                wc_id_to_uuid.insert(wc_product.id, id);
            }
        }

        for (i, review) in wc_reviews.iter().enumerate() {
            progress(ImportProgress {
                stage: "reviews".to_string(),
                current: i + 1,
                total: wc_reviews.len(),
                message: format!("Importing review {}/{}", i + 1, wc_reviews.len()),
            });

            // Keep parity with the live site: only approved reviews are shown
            if review.status != "approved" {
                stats.skipped += 1;
                continue;
            }

            let Some(product_id) = wc_id_to_uuid.get(&review.product_id) else {
                stats.skipped += 1;
                tracing::warn!(
                    "Review {} references WooCommerce product {} which was not imported; skipping",
                    review.id,
                    review.product_id
                );
                continue;
            };

            // WooCommerce review bodies are HTML; store plain text
            let plain_content = html_to_plain_text(&review.review);

            // Skip when this exact review was already imported (idempotent re-runs)
            let already_imported: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM product_reviews WHERE product_id = $1 AND author_name = $2 AND content = $3)",
            )
            .bind(product_id)
            .bind(&review.reviewer)
            .bind(&plain_content)
            .fetch_one(&pool)
            .await
            .map_err(|e| ImportError::Database(crate::Error::Database(e)))?;
            if already_imported {
                stats.skipped += 1;
                continue;
            }

            // Link to a local customer account when the reviewer email matches
            let customer_id: Option<Uuid> = match review.reviewer_email.as_deref() {
                Some(email) if !email.is_empty() => sqlx::query_scalar::<_, Uuid>(
                    "SELECT id FROM customers WHERE email = $1 LIMIT 1",
                )
                .bind(email)
                .fetch_optional(&pool)
                .await
                .map_err(|e| ImportError::Database(crate::Error::Database(e)))?,
                _ => None,
            };

            let rating = review.rating.unwrap_or(5).clamp(1, 5);
            let created_at = parse_wc_datetime(review.date_created.as_deref());

            let result = sqlx::query(
                r#"
                INSERT INTO product_reviews (
                    id, product_id, customer_id, author_name, rating, title, content,
                    status, created_at, updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, 'approved', $8, NOW())
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(product_id)
            .bind(customer_id)
            .bind(&review.reviewer)
            .bind(rating)
            .bind(None::<String>) // WooCommerce reviews have no separate title
            .bind(&plain_content)
            .bind(created_at)
            .execute(&pool)
            .await;

            match result {
                Ok(_) => {
                    stats.created += 1;
                    tracing::info!(
                        "Imported review for product {} (rating {})",
                        review.product_id,
                        rating
                    );
                }
                Err(e) => {
                    stats.errors += 1;
                    let error_msg = format!("Failed to import review {}: {}", review.id, e);
                    tracing::error!("{}", error_msg);
                    stats.error_details.push(error_msg);
                    if !config.options.continue_on_error {
                        break;
                    }
                }
            }
        }

        Ok(stats)
    }

    async fn import_all(
        &self,
        config: &ImportConfig,
        progress: &(dyn Fn(ImportProgress) + Send + Sync),
    ) -> ImportResult<ImportStats> {
        let mut total_stats = self.import_products(config, progress).await?;
        total_stats.add(&self.import_customers(config, progress).await?);
        total_stats.add(&self.import_orders(config, progress).await?);
        total_stats.add(&self.import_reviews(config, progress).await?);
        Ok(total_stats)
    }
}

/// Convert WooCommerce review HTML to plain text: strip all tags, decode the
/// common entities WooCommerce emits, and collapse whitespace.
fn html_to_plain_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                text.push(' ');
            }
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    // Decode entities. `&amp;` must come last so we don't double-decode.
    let decoded = text
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#034;", "\"")
        .replace("&#34;", "\"")
        .replace("&#39;", "'")
        .replace("&#039;", "'")
        .replace("&rsquo;", "'")
        .replace("&lsquo;", "'")
        .replace("&rdquo;", "\"")
        .replace("&ldquo;", "\"")
        .replace("&nbsp;", " ")
        .replace("&#160;", " ")
        .replace("&hellip;", "…")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
        .replace("&amp;", "&");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parse a WooCommerce date string ("2024-05-01T10:00:00" or RFC 3339)
fn parse_wc_datetime(value: Option<&str>) -> chrono::DateTime<chrono::Utc> {
    if let Some(value) = value {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(value) {
            return dt.with_timezone(&chrono::Utc);
        }
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S") {
            return chrono::DateTime::from_naive_utc_and_offset(naive, chrono::Utc);
        }
    }
    chrono::Utc::now()
}

impl WooCommerceImporter {
    /// Helper method to create an address for a customer
    async fn create_address(
        &self,
        pool: &PgPool,
        customer_id: Uuid,
        wc_addr: &WooCommerceAddress,
        is_default_billing: bool,
        is_default_shipping: bool,
    ) -> ImportResult<()> {
        // Only create if we have minimum required fields
        let address1 = match wc_addr.address_1 {
            Some(ref addr) if !addr.is_empty() => addr.clone(),
            _ => return Ok(()), // Skip if no address
        };

        let city = match wc_addr.city {
            Some(ref c) if !c.is_empty() => c.clone(),
            _ => return Ok(()), // Skip if no city
        };

        let country = match wc_addr.country {
            Some(ref c) if !c.is_empty() => c.clone(),
            _ => "US".to_string(), // Default to US
        };

        let zip = match wc_addr.postcode {
            Some(ref z) if !z.is_empty() => z.clone(),
            _ => "00000".to_string(), // Default zip
        };

        let first_name = wc_addr.first_name.clone().unwrap_or_default();
        let last_name = wc_addr.last_name.clone().unwrap_or_default();

        // Use a default name if both are empty
        let (first_name, last_name) = if first_name.is_empty() && last_name.is_empty() {
            ("Unknown".to_string(), "Customer".to_string())
        } else {
            (first_name, last_name)
        };

        let result = sqlx::query(
            r#"
            INSERT INTO addresses (
                id, customer_id, first_name, last_name, company,
                phone, address1, address2, city, province, country, zip,
                is_default_shipping, is_default_billing, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, NOW(), NOW())
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(customer_id)
        .bind(first_name)
        .bind(last_name)
        .bind(wc_addr.company.clone())
        .bind(wc_addr.phone.clone())
        .bind(address1)
        .bind(wc_addr.address_2.clone())
        .bind(city)
        .bind(wc_addr.state.clone())
        .bind(country)
        .bind(zip)
        .bind(is_default_shipping)
        .bind(is_default_billing)
        .execute(pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(ImportError::Database(crate::Error::Database(e))),
        }
    }

    /// Helper method to create an address for an order (optionally linked to a customer)
    async fn create_address_for_order(
        &self,
        pool: &PgPool,
        customer_id: Option<Uuid>,
        wc_addr: &WooCommerceAddress,
    ) -> ImportResult<Uuid> {
        let address_id = Uuid::new_v4();

        // Only create if we have minimum required fields
        let address1 = match wc_addr.address_1 {
            Some(ref addr) if !addr.is_empty() => addr.clone(),
            _ => "Unknown".to_string(),
        };

        let city = match wc_addr.city {
            Some(ref c) if !c.is_empty() => c.clone(),
            _ => "Unknown".to_string(),
        };

        let country = match wc_addr.country {
            Some(ref c) if !c.is_empty() => c.clone(),
            _ => "US".to_string(),
        };

        let zip = match wc_addr.postcode {
            Some(ref z) if !z.is_empty() => z.clone(),
            _ => "00000".to_string(),
        };

        let first_name = wc_addr.first_name.clone().unwrap_or_default();
        let last_name = wc_addr.last_name.clone().unwrap_or_default();

        // Use a default name if both are empty
        let (first_name, last_name) = if first_name.is_empty() && last_name.is_empty() {
            ("Unknown".to_string(), "Customer".to_string())
        } else {
            (first_name, last_name)
        };

        let result = sqlx::query(
            r#"
            INSERT INTO addresses (
                id, customer_id, first_name, last_name, company,
                phone, address1, address2, city, province, country, zip,
                is_default_shipping, is_default_billing, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, NOW(), NOW())
            "#,
        )
        .bind(address_id)
        .bind(customer_id) // Link to customer if found
        .bind(first_name)
        .bind(last_name)
        .bind(wc_addr.company.clone())
        .bind(wc_addr.phone.clone())
        .bind(address1)
        .bind(wc_addr.address_2.clone())
        .bind(city)
        .bind(wc_addr.state.clone())
        .bind(country)
        .bind(zip)
        .bind(false)
        .bind(false)
        .execute(pool)
        .await;

        match result {
            Ok(_) => Ok(address_id),
            Err(e) => Err(ImportError::Database(crate::Error::Database(e))),
        }
    }

    /// Look up product ID by SKU
    async fn lookup_product_by_sku(&self, pool: &PgPool, sku: &str) -> ImportResult<Option<Uuid>> {
        if sku.is_empty() {
            return Ok(None);
        }

        let product_id =
            sqlx::query_scalar::<_, Uuid>("SELECT id FROM products WHERE sku = $1 LIMIT 1")
                .bind(sku)
                .fetch_optional(pool)
                .await
                .map_err(|e| ImportError::Database(crate::Error::Database(e)))?;

        Ok(product_id)
    }

    /// Look up product ID by name (fallback if SKU not found)
    async fn lookup_product_by_name(
        &self,
        pool: &PgPool,
        name: &str,
    ) -> ImportResult<Option<Uuid>> {
        if name.is_empty() {
            return Ok(None);
        }

        let product_id =
            sqlx::query_scalar::<_, Uuid>("SELECT id FROM products WHERE title = $1 LIMIT 1")
                .bind(name)
                .fetch_optional(pool)
                .await
                .map_err(|e| ImportError::Database(crate::Error::Database(e)))?;

        Ok(product_id)
    }

    /// Find the first active inventory location
    async fn get_first_active_inventory_location(
        &self,
        pool: &PgPool,
    ) -> ImportResult<Option<Uuid>> {
        let location_id = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM inventory_locations WHERE is_active = true ORDER BY created_at LIMIT 1",
        )
        .fetch_optional(pool)
        .await
        .map_err(|e| ImportError::Database(crate::Error::Database(e)))?;

        Ok(location_id)
    }

    /// Create inventory level for a product or variant
    async fn create_inventory_level(
        &self,
        pool: &PgPool,
        product_id: Uuid,
        variant_id: Option<Uuid>,
        quantity: i32,
        stats: &mut ImportStats,
    ) {
        let location_id = match self.get_first_active_inventory_location(pool).await {
            Ok(Some(id)) => id,
            Ok(None) => return,
            Err(e) => {
                tracing::warn!("Failed to look up inventory location: {}", e);
                return;
            }
        };

        // UPDATE first, INSERT only if no row exists: ON CONFLICT cannot match
        // NULL variant_id rows (the unique index treats NULLs as distinct), so
        // an upsert silently inserted a duplicate row on every re-import.
        let updated = sqlx::query(
            r#"
            UPDATE inventory_levels
            SET available_quantity = $3,
                updated_at = NOW()
            WHERE product_id = $1 AND variant_id IS NOT DISTINCT FROM $2 AND location_id = $4
            "#,
        )
        .bind(product_id)
        .bind(variant_id)
        .bind(quantity)
        .bind(location_id)
        .execute(pool)
        .await;

        let outcome = match updated {
            Ok(result) if result.rows_affected() > 0 => Ok(()),
            Ok(_) => {
                sqlx::query(
                    r#"
                    INSERT INTO inventory_levels (
                        id, product_id, variant_id, location_id, available_quantity,
                        reserved_quantity, incoming_quantity, reorder_point, reorder_quantity,
                        cost_per_unit, last_counted_at, created_at, updated_at
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, NOW(), NOW())
                    "#,
                )
                .bind(Uuid::new_v4())
                .bind(product_id)
                .bind(variant_id)
                .bind(location_id)
                .bind(quantity)
                .bind(0i32)
                .bind(0i32)
                .bind(0i32)
                .bind(0i32)
                .bind(None::<Decimal>)
                .bind(None::<chrono::DateTime<chrono::Utc>>)
                .execute(pool)
                .await
                .map(|_| ())
            }
            Err(e) => Err(e),
        };

        if let Err(e) = outcome {
            stats.errors += 1;
            let entity = if variant_id.is_some() {
                "variant"
            } else {
                "product"
            };
            let error_msg = format!(
                "Failed to create inventory level for {} {}: {}",
                entity, product_id, e
            );
            tracing::error!("{}", error_msg);
            stats.error_details.push(error_msg);
        }
    }

    /// Persist product images, downloading them to local storage when possible
    async fn create_product_images(
        &self,
        pool: &PgPool,
        product_id: Uuid,
        variant_id: Option<Uuid>,
        images: &[WooCommerceImage],
        stats: &mut ImportStats,
    ) {
        for (position, image) in images.iter().enumerate() {
            let filename = self.sanitize_filename(&image.src);
            let dest_dir = std::path::PathBuf::from(format!(
                "/var/lib/rcommerce/uploads/products/{}",
                product_id
            ));

            let image_base = std::env::var("IMAGE_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:8080/uploads".to_string());
            let local_url = if let Some(_dest_path) =
                self.download_image(&image.src, &dest_dir, &filename).await
            {
                format!(
                    "{}/products/{}/{}",
                    image_base.trim_end_matches('/'),
                    product_id,
                    filename
                )
            } else {
                image.src.clone()
            };

            if let Err(e) = sqlx::query(
                r#"
                INSERT INTO product_images (
                    id, product_id, variant_id, position, src, alt_text,
                    width, height, created_at, updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW(), NOW())
                "#,
            )
            .bind(Uuid::new_v4())
            .bind(product_id)
            .bind(variant_id)
            .bind(position as i32)
            .bind(local_url)
            .bind(image.alt.clone())
            .bind(None::<i32>)
            .bind(None::<i32>)
            .execute(pool)
            .await
            {
                stats.errors += 1;
                let entity = if variant_id.is_some() {
                    "variant"
                } else {
                    "product"
                };
                let error_msg = format!(
                    "Failed to create image for {} {}: {}",
                    entity, product_id, e
                );
                tracing::error!("{}", error_msg);
                stats.error_details.push(error_msg);
            }
        }
    }

    /// Import WooCommerce variations for a variable product
    #[allow(clippy::too_many_arguments)]
    async fn import_product_variations(
        &self,
        base_url: &str,
        consumer_key: &str,
        consumer_secret: &str,
        product: &WooCommerceProduct,
        parent_product_id: Uuid,
        pool: &PgPool,
        default_currency: Currency,
        stats: &mut ImportStats,
    ) -> ImportResult<()> {
        use std::collections::HashMap;

        // Create product options for variation attributes
        let mut option_ids: HashMap<String, Uuid> = HashMap::new();
        for (idx, attr) in product.attributes.iter().enumerate() {
            if !attr.variation {
                continue;
            }

            let option_id = Uuid::new_v4();
            sqlx::query(
                r#"
                INSERT INTO product_options (
                    id, product_id, name, position, created_at, updated_at
                )
                VALUES ($1, $2, $3, $4, NOW(), NOW())
                "#,
            )
            .bind(option_id)
            .bind(parent_product_id)
            .bind(&attr.name)
            .bind(attr.position.unwrap_or(idx as i32))
            .execute(pool)
            .await
            .map_err(|e| ImportError::Database(crate::Error::Database(e)))?;

            option_ids.insert(attr.name.clone(), option_id);
        }

        // Fetch variations from WooCommerce
        let url = self.api_url(base_url, &format!("products/{}/variations", product.id));
        let variations: Vec<WooCommerceVariation> = self
            .fetch_paginated(&url, consumer_key, consumer_secret, 0)
            .await?;

        for variation in variations {
            let price = if !variation.sale_price.is_empty() && variation.sale_price != "0" {
                self.parse_price(&variation.sale_price)
            } else if !variation.price.is_empty() {
                self.parse_price(&variation.price)
            } else {
                self.parse_price(&variation.regular_price)
            };

            let compare_at_price = if !variation.regular_price.is_empty()
                && !variation.sale_price.is_empty()
                && variation.sale_price != "0"
                && variation.regular_price != variation.sale_price
            {
                Some(self.parse_price(&variation.regular_price))
            } else {
                None
            };

            let title = if variation.attributes.is_empty() {
                variation
                    .sku
                    .clone()
                    .unwrap_or_else(|| format!("Variation {}", variation.id))
            } else {
                variation
                    .attributes
                    .iter()
                    .map(|a| format!("{}: {}", a.name, a.option))
                    .collect::<Vec<_>>()
                    .join(", ")
            };

            let inventory_quantity = variation.stock_quantity.unwrap_or(0);
            let variant_id = Uuid::new_v4();

            sqlx::query(
                r#"
                INSERT INTO product_variants (
                    id, product_id, title, sku, price, compare_at_price, cost_price,
                    currency, inventory_quantity, inventory_policy, weight, weight_unit,
                    requires_shipping, is_active, gtin, created_at, updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, NOW(), NOW())
                "#,
            )
            .bind(variant_id)
            .bind(parent_product_id)
            .bind(&title)
            .bind(&variation.sku)
            .bind(price)
            .bind(compare_at_price)
            .bind(None::<Decimal>)
            .bind(default_currency)
            .bind(inventory_quantity)
            .bind(InventoryPolicy::Deny)
            .bind(None::<Decimal>)
            .bind(None::<WeightUnit>)
            .bind(true)
            .bind(variation.status == "publish")
            .bind(gtin_from_meta(&variation.meta_data))
            .execute(pool)
            .await
            .map_err(|e| ImportError::Database(crate::Error::Database(e)))?;

            // Link option values to the variant
            for attr in &variation.attributes {
                if let Some(option_id) = option_ids.get(&attr.name) {
                    sqlx::query(
                        r#"
                        INSERT INTO product_option_values (
                            id, option_id, variant_id, value, created_at, updated_at
                        )
                        VALUES ($1, $2, $3, $4, NOW(), NOW())
                        "#,
                    )
                    .bind(Uuid::new_v4())
                    .bind(option_id)
                    .bind(variant_id)
                    .bind(&attr.option)
                    .execute(pool)
                    .await
                    .map_err(|e| ImportError::Database(crate::Error::Database(e)))?;
                }
            }

            // Persist variation image if present
            if let Some(ref image) = variation.image {
                self.create_product_images(
                    pool,
                    parent_product_id,
                    Some(variant_id),
                    std::slice::from_ref(image),
                    stats,
                )
                .await;
            }

            // Create inventory level for the variant
            self.create_inventory_level(
                pool,
                parent_product_id,
                Some(variant_id),
                inventory_quantity,
                stats,
            )
            .await;
        }

        Ok(())
    }
}

// WooCommerce API response types
// These are used for API deserialization - fields are read by serde
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct WooCommerceProduct {
    id: u64,
    name: String,
    slug: String,
    #[serde(rename = "type")]
    product_type: String,
    status: String,
    description: Option<String>,
    #[serde(rename = "short_description")]
    short_description: Option<String>,
    sku: Option<String>,
    price: String,
    #[serde(rename = "regular_price")]
    regular_price: String,
    #[serde(rename = "sale_price")]
    sale_price: String,
    weight: Option<String>,
    dimensions: Option<WooCommerceDimensions>,
    #[serde(rename = "stock_quantity")]
    stock_quantity: Option<i32>,
    #[serde(rename = "stock_status")]
    stock_status: Option<String>,
    #[serde(rename = "manage_stock")]
    manage_stock: bool,
    categories: Vec<WooCommerceCategory>,
    images: Vec<WooCommerceImage>,
    attributes: Vec<WooCommerceAttribute>,
    #[serde(default)]
    meta_data: Vec<WooCommerceMeta>,
    #[serde(default)]
    bundled_items: Vec<WooCommerceBundledItem>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct WooCommerceMeta {
    key: String,
    #[serde(default)]
    value: serde_json::Value,
}

impl WooCommerceMeta {
    fn text(&self) -> Option<String> {
        match &self.value {
            serde_json::Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        }
    }
}

/// UPC/EAN/GTIN from WooCommerce meta entries, digits only. Checks the common
/// plugin keys in priority order (first non-empty value wins) so a re-import
/// picks up codes automatically.
fn gtin_from_meta(meta: &[WooCommerceMeta]) -> Option<String> {
    const KEYS: [&str; 5] = ["_upc", "upc", "_gtin", "gtin", "_ean"];
    for key in KEYS {
        if let Some(raw) = meta.iter().find(|m| m.key == key).and_then(|m| m.text()) {
            let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
            if (8..=14).contains(&digits.len()) {
                return Some(digits);
            }
        }
    }
    None
}

impl WooCommerceProduct {
    fn extract_gtin(&self) -> Option<String> {
        gtin_from_meta(&self.meta_data)
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
struct WooCommerceDimensions {
    length: Option<String>,
    width: Option<String>,
    height: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct WooCommerceBundledItem {
    #[serde(rename = "product_id")]
    product_id: u64,
    #[serde(default = "default_bundle_quantity")]
    quantity: i32,
    #[serde(default)]
    optional: bool,
    #[serde(default)]
    #[serde(rename = "bundle_order")]
    sort_order: i32,
}

fn default_bundle_quantity() -> i32 {
    1
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct WooCommerceCategory {
    id: u64,
    name: String,
    slug: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct WooCommerceImage {
    id: u64,
    src: String,
    name: Option<String>,
    alt: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct WooCommerceAttribute {
    id: u64,
    name: String,
    position: Option<i32>,
    options: Vec<String>,
    #[serde(default)]
    variation: bool,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct WooCommerceVariation {
    id: u64,
    sku: Option<String>,
    price: String,
    #[serde(rename = "regular_price")]
    regular_price: String,
    #[serde(rename = "sale_price")]
    sale_price: String,
    #[serde(rename = "stock_quantity")]
    stock_quantity: Option<i32>,
    status: String,
    image: Option<WooCommerceImage>,
    #[serde(default)]
    attributes: Vec<WooCommerceVariationAttribute>,
    #[serde(default)]
    meta_data: Vec<WooCommerceMeta>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct WooCommerceVariationAttribute {
    id: u64,
    name: String,
    option: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct WooCommerceCustomer {
    id: u64,
    email: String,
    #[serde(rename = "first_name")]
    first_name: String,
    #[serde(rename = "last_name")]
    last_name: String,
    username: String,
    billing: Option<WooCommerceAddress>,
    shipping: Option<WooCommerceAddress>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct WooCommerceAddress {
    #[serde(rename = "first_name")]
    first_name: Option<String>,
    #[serde(rename = "last_name")]
    last_name: Option<String>,
    company: Option<String>,
    address_1: Option<String>,
    address_2: Option<String>,
    city: Option<String>,
    state: Option<String>,
    postcode: Option<String>,
    country: Option<String>,
    email: Option<String>,
    phone: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct WooCommerceOrder {
    id: u64,
    #[serde(rename = "order_number")]
    order_number: Option<String>,
    status: String,
    currency: String,
    total: String,
    #[serde(rename = "total_tax")]
    total_tax: Option<String>,
    #[serde(rename = "shipping_total")]
    shipping_total: Option<String>,
    #[serde(rename = "discount_total")]
    discount_total: Option<String>,
    /// Original Woo order date — preserved so imported orders keep their
    /// real placement in time (accounting period queries depend on it).
    #[serde(rename = "date_created")]
    date_created: Option<String>,
    #[serde(rename = "date_modified")]
    date_modified: Option<String>,
    billing: Option<WooCommerceAddress>,
    shipping: Option<WooCommerceAddress>,
    #[serde(rename = "line_items")]
    line_items: Option<Vec<WooCommerceLineItem>>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct WooCommerceLineItem {
    id: u64,
    name: String,
    product_id: u64,
    quantity: i32,
    subtotal: String,
    total: String,
    sku: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct WooCommerceReview {
    id: u64,
    #[serde(rename = "product_id")]
    product_id: u64,
    reviewer: String,
    #[serde(rename = "reviewer_email")]
    reviewer_email: Option<String>,
    rating: Option<i32>,
    review: String,
    status: String,
    #[serde(rename = "date_created")]
    date_created: Option<String>,
}

#[cfg(test)]
mod shortcode_tests {
    use super::*;

    #[test]
    fn strips_wpbakery_shortcodes_keeps_inner_text() {
        let input = r#"[vc_row type="full_width_background" column_margin="default"][vc_column][vc_column_text]The Hemotox™ formula delivers grip and rebound.[/vc_column_text][/vc_column][/vc_row]

[vc_separator color="green"]"#;
        let out = WooCommerceImporter::strip_shortcodes(input);
        assert!(out.contains("The Hemotox™ formula delivers grip and rebound."));
        assert!(!out.contains("vc_row"));
        assert!(!out.contains("vc_separator"));
        assert!(!out.contains("column_margin"));
    }

    #[test]
    fn decodes_entities_and_collapses_blank_lines() {
        let input = "First&nbsp;&amp; foremost&#8212;always.\n\n\n\nSecond line   ";
        let out = WooCommerceImporter::strip_shortcodes(input);
        assert_eq!(out, "First & foremost—always.\n\nSecond line");
    }

    #[test]
    fn plain_text_untouched() {
        let input = "Just a normal description without any markup at all.";
        let out = WooCommerceImporter::strip_shortcodes(input);
        assert_eq!(out, input);
    }

    #[test]
    fn bracketed_prose_is_stripped_too() {
        // Indistinguishable from a real shortcode; readability is preserved.
        let out = WooCommerceImporter::strip_shortcodes("See the [brackets] note.");
        assert_eq!(out, "See the  note.");
    }

    fn woo_product_with_meta(entries: Vec<(&str, serde_json::Value)>) -> WooCommerceProduct {
        WooCommerceProduct {
            id: 1,
            name: "Test".to_string(),
            slug: "test".to_string(),
            product_type: "simple".to_string(),
            status: "publish".to_string(),
            description: None,
            short_description: None,
            sku: None,
            price: "10".to_string(),
            regular_price: "10".to_string(),
            sale_price: String::new(),
            weight: None,
            dimensions: None,
            stock_quantity: Some(10),
            stock_status: None,
            manage_stock: true,
            categories: Vec::new(),
            images: Vec::new(),
            attributes: Vec::new(),
            meta_data: entries
                .into_iter()
                .map(|(key, value)| WooCommerceMeta {
                    key: key.to_string(),
                    value,
                })
                .collect(),
            bundled_items: Vec::new(),
        }
    }

    #[test]
    fn extract_gtin_reads_common_upc_meta_keys() {
        let p = woo_product_with_meta(vec![("_upc", serde_json::json!("123456789012"))]);
        assert_eq!(p.extract_gtin().as_deref(), Some("123456789012"));
    }

    #[test]
    fn extract_gtin_prefers_first_known_key_and_strips_nondigits() {
        let p = woo_product_with_meta(vec![
            ("upc", serde_json::json!("0-12345-67890-5")),
            ("_gtin", serde_json::json!("9999999999999")),
        ]);
        assert_eq!(p.extract_gtin().as_deref(), Some("012345678905"));

        let ean = woo_product_with_meta(vec![("_ean", serde_json::json!("5012345678900"))]);
        assert_eq!(ean.extract_gtin().as_deref(), Some("5012345678900"));
    }

    #[test]
    fn extract_gtin_ignores_missing_short_and_empty_values() {
        assert_eq!(woo_product_with_meta(vec![]).extract_gtin(), None);
        assert_eq!(
            woo_product_with_meta(vec![("_upc", serde_json::json!(""))]).extract_gtin(),
            None
        );
        // Too short after digit extraction — ignored.
        assert_eq!(
            woo_product_with_meta(vec![("_upc", serde_json::json!("12345"))]).extract_gtin(),
            None
        );
    }
}
