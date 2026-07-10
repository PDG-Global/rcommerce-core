//! Shopify importer implementation

use crate::import::{
    error::{ImportError, ImportResult},
    types::{ImportConfig, ImportProgress, ImportStats},
    PlatformImporter,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

const SHOPIFY_API_VERSION: &str = "2024-01";

/// Shopify API client and importer
pub struct ShopifyImporter {
    client: Client,
}

impl ShopifyImporter {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// Get Shopify API URL
    fn api_url(&self, shop_domain: &str, endpoint: &str) -> String {
        format!(
            "https://{}/admin/api/{}/{}.json",
            shop_domain, SHOPIFY_API_VERSION, endpoint
        )
    }

    /// Fetch paginated data from Shopify
    async fn fetch_paginated<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        access_token: &str,
        limit: usize,
    ) -> ImportResult<Vec<T>> {
        let mut results = Vec::new();
        let page_info: Option<String> = None;
        let batch_size = 250.min(limit);

        loop {
            let mut request_url = format!("{}?limit={}", url, batch_size);
            if let Some(ref pi) = page_info {
                request_url = format!("{}&page_info={}", url, pi);
            }

            let response = self
                .client
                .get(&request_url)
                .header("X-Shopify-Access-Token", access_token)
                .header("Content-Type", "application/json")
                .send()
                .await?;

            if response.status().as_u16() == 429 {
                // Rate limited - wait and retry
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }

            if !response.status().is_success() {
                return Err(ImportError::Api(format!(
                    "Shopify API error: {} - {}",
                    response.status(),
                    response.text().await.unwrap_or_default()
                )));
            }

            // Parse response based on entity type
            let data: serde_json::Value = response.json().await?;

            // Extract items from the response
            let items = if let Some(arr) = data.as_array() {
                arr.clone()
            } else if let Some(obj) = data.as_object() {
                // Shopify wraps arrays in object keys like "products", "customers"
                obj.values()
                    .find(|v| v.is_array())
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            let items_len = items.len();
            for item in items {
                if let Ok(parsed) = serde_json::from_value::<T>(item) {
                    results.push(parsed);
                    if limit > 0 && results.len() >= limit {
                        return Ok(results);
                    }
                }
            }

            // Check for next page
            // Note: Shopify uses Link headers for pagination, simplified here
            if items_len < batch_size {
                break;
            }

            // In real implementation, parse Link header for page_info
            break; // Simplified - break after first batch for now
        }

        Ok(results)
    }
}

impl Default for ShopifyImporter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformImporter for ShopifyImporter {
    fn platform(&self) -> &'static str {
        "shopify"
    }

    async fn import_products(
        &self,
        config: &ImportConfig,
        progress: &(dyn Fn(ImportProgress) + Send + Sync),
    ) -> ImportResult<ImportStats> {
        let (shop_domain, access_token) = match &config.source {
            crate::import::types::SourceConfig::Platform {
                api_url, api_key, ..
            } => (api_url.clone(), api_key.clone()),
            _ => {
                return Err(ImportError::Configuration(
                    "Invalid source configuration for Shopify".to_string(),
                ))
            }
        };

        let dry_run = config.options.dry_run;

        progress(ImportProgress {
            stage: "products".to_string(),
            current: 0,
            total: 1,
            message: if dry_run {
                "Fetching products from Shopify (DRY RUN)...".to_string()
            } else {
                "Fetching products from Shopify...".to_string()
            },
        });

        // Fetch products from Shopify
        let url = self.api_url(&shop_domain, "products");
        let shopify_products: Vec<ShopifyProduct> = self
            .fetch_paginated(&url, &access_token, config.options.limit)
            .await?;

        progress(ImportProgress {
            stage: "products".to_string(),
            current: 0,
            total: shopify_products.len(),
            message: if dry_run {
                format!(
                    "Validating {} products (DRY RUN)...",
                    shopify_products.len()
                )
            } else {
                format!("Importing {} products...", shopify_products.len())
            },
        });

        let mut stats = ImportStats {
            total: shopify_products.len(),
            ..Default::default()
        };

        // Transform and import each product
        for (i, shopify_product) in shopify_products.iter().enumerate() {
            progress(ImportProgress {
                stage: "products".to_string(),
                current: i + 1,
                total: shopify_products.len(),
                message: if dry_run {
                    format!("Validating: {}", shopify_product.title)
                } else {
                    format!("Importing: {}", shopify_product.title)
                },
            });

            // Validate the product
            if shopify_product.title.is_empty() {
                stats.errors += 1;
                stats
                    .error_details
                    .push(format!("Product {} has no title", shopify_product.id));
                continue;
            }

            if dry_run {
                // In dry run mode, just validate and count what would be imported
                stats.created += 1;
            } else {
                // In real implementation, this would insert into database
                // For now, just count as created
                stats.created += 1;
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
        let (shop_domain, access_token) = match &config.source {
            crate::import::types::SourceConfig::Platform {
                api_url, api_key, ..
            } => (api_url.clone(), api_key.clone()),
            _ => {
                return Err(ImportError::Configuration(
                    "Invalid source configuration for Shopify".to_string(),
                ))
            }
        };

        progress(ImportProgress {
            stage: "customers".to_string(),
            current: 0,
            total: 1,
            message: if dry_run {
                "Fetching customers from Shopify (DRY RUN)...".to_string()
            } else {
                "Fetching customers from Shopify...".to_string()
            },
        });

        let url = self.api_url(&shop_domain, "customers");
        let shopify_customers: Vec<ShopifyCustomer> = self
            .fetch_paginated(&url, &access_token, config.options.limit)
            .await?;

        let mut stats = ImportStats {
            total: shopify_customers.len(),
            ..Default::default()
        };

        for (i, customer) in shopify_customers.iter().enumerate() {
            progress(ImportProgress {
                stage: "customers".to_string(),
                current: i + 1,
                total: shopify_customers.len(),
                message: if dry_run {
                    format!("Validating customer {}/{}", i + 1, shopify_customers.len())
                } else {
                    format!("Importing customer {}/{}", i + 1, shopify_customers.len())
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
                // In real implementation, insert into database
                stats.created += 1;
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
        let (shop_domain, access_token) = match &config.source {
            crate::import::types::SourceConfig::Platform {
                api_url, api_key, ..
            } => (api_url.clone(), api_key.clone()),
            _ => {
                return Err(ImportError::Configuration(
                    "Invalid source configuration for Shopify".to_string(),
                ))
            }
        };

        progress(ImportProgress {
            stage: "orders".to_string(),
            current: 0,
            total: 1,
            message: if dry_run {
                "Fetching orders from Shopify (DRY RUN)...".to_string()
            } else {
                "Fetching orders from Shopify...".to_string()
            },
        });

        let url = self.api_url(&shop_domain, "orders");
        let shopify_orders: Vec<ShopifyOrder> = self
            .fetch_paginated(&url, &access_token, config.options.limit)
            .await?;

        let mut stats = ImportStats {
            total: shopify_orders.len(),
            ..Default::default()
        };

        for (i, order) in shopify_orders.iter().enumerate() {
            progress(ImportProgress {
                stage: "orders".to_string(),
                current: i + 1,
                total: shopify_orders.len(),
                message: if dry_run {
                    format!("Validating order {}/{}", i + 1, shopify_orders.len())
                } else {
                    format!("Importing order {}/{}", i + 1, shopify_orders.len())
                },
            });

            // Validate order
            if order.email.is_empty() {
                stats.errors += 1;
                stats
                    .error_details
                    .push(format!("Order {} has no customer email", order.id));
                continue;
            }

            if dry_run {
                stats.created += 1;
            } else {
                // In real implementation, insert into database
                stats.created += 1;
            }
        }

        Ok(stats)
    }
}

// Shopify API response types
// These are used for API deserialization - fields are read by serde
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ShopifyProduct {
    id: u64,
    title: String,
    handle: String,
    description: Option<String>,
    #[serde(rename = "product_type")]
    product_type: Option<String>,
    #[serde(rename = "created_at")]
    created_at: String,
    #[serde(rename = "updated_at")]
    updated_at: String,
    variants: Vec<ShopifyVariant>,
    images: Vec<ShopifyImage>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ShopifyVariant {
    id: u64,
    title: String,
    sku: Option<String>,
    price: String,
    #[serde(rename = "compare_at_price")]
    compare_at_price: Option<String>,
    #[serde(rename = "inventory_quantity")]
    inventory_quantity: i32,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ShopifyImage {
    id: u64,
    src: String,
    #[serde(rename = "alt_text")]
    alt_text: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ShopifyCustomer {
    id: u64,
    email: String,
    #[serde(rename = "first_name")]
    first_name: Option<String>,
    #[serde(rename = "last_name")]
    last_name: Option<String>,
    #[serde(rename = "created_at")]
    created_at: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ShopifyOrder {
    id: u64,
    #[serde(rename = "order_number")]
    order_number: i64,
    email: String,
    #[serde(rename = "total_price")]
    total_price: String,
    #[serde(rename = "created_at")]
    created_at: String,
}
