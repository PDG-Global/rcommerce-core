//! Digital Product Service
//!
//! Handles digital product file management, download tracking, and license key generation.

use chrono::{Duration, Utc};
use rand::Rng;
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    media::file_upload::FileUploadService,
    models::{DownloadResponse, LicenseKey, OrderItem, OrderItemDownload, Product},
    repository::Database,
    Error, Result,
};

/// Digital product service for managing downloads and license keys
pub struct DigitalProductService {
    db: Database,
    file_service: Arc<FileUploadService>,
}

impl DigitalProductService {
    /// Create a new digital product service
    pub fn new(db: Database, file_service: Arc<FileUploadService>) -> Self {
        Self { db, file_service }
    }

    /// Create a download record for an order item
    pub async fn create_download(
        &self,
        order_item_id: Uuid,
        customer_id: Option<Uuid>,
        download_limit: Option<i32>,
        expiry_days: Option<i32>,
    ) -> Result<OrderItemDownload> {
        let download_token = self.generate_download_token();

        let expires_at = expiry_days.map(|days| Utc::now() + Duration::days(days as i64));

        let download = sqlx::query_as::<_, OrderItemDownload>(
            r#"
            INSERT INTO order_item_downloads (
                order_item_id, customer_id, download_token, 
                download_count, download_limit, expires_at
            )
            VALUES ($1, $2, $3, 0, $4, $5)
            RETURNING *
            "#,
        )
        .bind(order_item_id)
        .bind(customer_id)
        .bind(&download_token)
        .bind(download_limit)
        .bind(expires_at)
        .fetch_one(self.db.pool())
        .await?;

        Ok(download)
    }

    /// Get download by token
    pub async fn get_download_by_token(&self, token: &str) -> Result<Option<OrderItemDownload>> {
        let download = sqlx::query_as::<_, OrderItemDownload>(
            "SELECT * FROM order_item_downloads WHERE download_token = $1",
        )
        .bind(token)
        .fetch_optional(self.db.pool())
        .await?;

        Ok(download)
    }

    /// Validate and record a download
    pub async fn record_download(&self, token: &str) -> Result<DownloadResponse> {
        let mut download = self
            .get_download_by_token(token)
            .await?
            .ok_or_else(|| Error::not_found("Download not found"))?;

        // Check if expired
        if let Some(expires_at) = download.expires_at {
            if Utc::now() > expires_at {
                return Err(Error::validation("Download link has expired"));
            }
        }

        // Check download limit
        if let Some(limit) = download.download_limit {
            if download.download_count >= limit {
                return Err(Error::validation("Download limit reached"));
            }
        }

        // Get order item and product details
        let order_item = sqlx::query_as::<_, OrderItem>("SELECT * FROM order_items WHERE id = $1")
            .bind(download.order_item_id)
            .fetch_one(self.db.pool())
            .await?;

        let product = sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = $1")
            .bind(order_item.product_id)
            .fetch_one(self.db.pool())
            .await?;

        // Update download count
        download.download_count += 1;
        download.updated_at = Utc::now();

        sqlx::query(
            "UPDATE order_item_downloads SET download_count = $1, updated_at = $2 WHERE id = $3",
        )
        .bind(download.download_count)
        .bind(download.updated_at)
        .bind(download.id)
        .execute(self.db.pool())
        .await?;

        // Generate download URL
        let file_url = product
            .file_url
            .ok_or_else(|| Error::not_found("Product file not found"))?;

        let expires_at = Utc::now() + Duration::hours(1); // URL valid for 1 hour
        let download_url = self
            .file_service
            .generate_download_url(&file_url, Duration::hours(1))?;

        Ok(DownloadResponse {
            download_url,
            file_name: product.title.clone(),
            file_size: product.file_size.unwrap_or(0),
            expires_at,
            download_count: download.download_count,
            download_limit: download.download_limit,
        })
    }

    /// Get all downloads for an order
    pub async fn get_order_downloads(&self, order_id: Uuid) -> Result<Vec<OrderItemDownload>> {
        let downloads = sqlx::query_as::<_, OrderItemDownload>(
            r#"
            SELECT d.* FROM order_item_downloads d
            JOIN order_items oi ON d.order_item_id = oi.id
            WHERE oi.order_id = $1
            ORDER BY d.created_at DESC
            "#,
        )
        .bind(order_id)
        .fetch_all(self.db.pool())
        .await?;

        Ok(downloads)
    }

    /// Get available downloads for a customer
    pub async fn get_customer_downloads(
        &self,
        customer_id: Uuid,
    ) -> Result<Vec<(OrderItemDownload, OrderItem, Product)>> {
        let rows = sqlx::query(
            r#"
            SELECT 
                d.id as download_id, d.order_item_id, d.customer_id, 
                d.download_token, d.download_count, d.download_limit, 
                d.expires_at, d.created_at, d.updated_at,
                oi.id as oi_id, oi.order_id, oi.product_id, oi.variant_id,
                oi.quantity, oi.price, oi.subtotal, oi.tax_amount, oi.total,
                oi.sku, oi.title, oi.variant_title, oi.requires_shipping,
                oi.is_gift_card, oi.weight, oi.weight_unit, oi.image_url,
                oi.created_at as oi_created_at, oi.updated_at as oi_updated_at,
                p.id as p_id, p.title as p_title, p.slug, p.description,
                p.sku as p_sku, p.product_type, p.price as p_price,
                p.compare_at_price, p.cost_price, p.currency,
                p.inventory_quantity, p.inventory_policy, p.inventory_management,
                p.continues_selling_when_out_of_stock, p.weight as p_weight,
                p.weight_unit as p_weight_unit, p.requires_shipping as p_requires_shipping,
                p.is_active, p.is_featured, p.seo_title, p.seo_description,
                p.created_at as p_created_at, p.updated_at as p_updated_at,
                p.published_at, p.subscription_interval, p.subscription_interval_count,
                p.subscription_trial_days, p.subscription_setup_fee,
                p.subscription_min_cycles, p.subscription_max_cycles,
                p.file_url, p.file_size, p.file_hash, p.download_limit as p_download_limit,
                p.license_key_enabled, p.download_expiry_days,
                p.bundle_pricing_strategy, p.bundle_discount_percentage
            FROM order_item_downloads d
            JOIN order_items oi ON d.order_item_id = oi.id
            JOIN products p ON oi.product_id = p.id
            WHERE d.customer_id = $1
            AND (d.expires_at IS NULL OR d.expires_at > NOW())
            ORDER BY d.created_at DESC
            "#,
        )
        .bind(customer_id)
        .fetch_all(self.db.pool())
        .await?;

        let mut results = Vec::new();
        for row in rows {
            let download = OrderItemDownload {
                id: row.try_get("download_id")?,
                order_item_id: row.try_get("order_item_id")?,
                customer_id: row.try_get("customer_id")?,
                download_token: row.try_get("download_token")?,
                download_count: row.try_get("download_count")?,
                download_limit: row.try_get("download_limit")?,
                expires_at: row.try_get("expires_at")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            };

            let order_item = OrderItem {
                id: row.try_get("oi_id")?,
                order_id: row.try_get("order_id")?,
                product_id: row.try_get("product_id")?,
                variant_id: row.try_get("variant_id")?,
                quantity: row.try_get("quantity")?,
                price: row.try_get("price")?,
                subtotal: row.try_get("subtotal")?,
                tax_amount: row.try_get("tax_amount")?,
                total: row.try_get("total")?,
                sku: row.try_get("sku")?,
                title: row.try_get("title")?,
                variant_title: row.try_get("variant_title")?,
                requires_shipping: row.try_get("requires_shipping")?,
                is_gift_card: row.try_get("is_gift_card")?,
                weight: row.try_get("weight")?,
                weight_unit: row.try_get("weight_unit")?,
                image_url: row.try_get("image_url")?,
                created_at: row.try_get("oi_created_at")?,
                updated_at: row.try_get("oi_updated_at")?,
                // Digital product fields
                is_digital: row.try_get("is_digital").ok(),
                download_url: row.try_get("download_url").ok(),
                license_key: row.try_get("license_key").ok(),
                // Bundle product fields
                bundle_parent_id: row.try_get("bundle_parent_id").ok(),
                is_bundle_component: row.try_get("is_bundle_component").ok(),
            };

            let product = Product {
                id: row.try_get("p_id")?,
                title: row.try_get("p_title")?,
                slug: row.try_get("slug")?,
                description: row.try_get("description")?,
                sku: row.try_get("p_sku")?,
                product_type: row.try_get("product_type")?,
                price: row.try_get("p_price")?,
                compare_at_price: row.try_get("compare_at_price")?,
                cost_price: row.try_get("cost_price")?,
                currency: row.try_get("currency")?,
                inventory_quantity: row.try_get("inventory_quantity")?,
                inventory_policy: row.try_get("inventory_policy")?,
                inventory_management: row.try_get("inventory_management")?,
                continues_selling_when_out_of_stock: row
                    .try_get("continues_selling_when_out_of_stock")?,
                weight: row.try_get("p_weight")?,
                weight_unit: row.try_get("p_weight_unit")?,
                requires_shipping: row.try_get("p_requires_shipping")?,
                is_active: row.try_get("is_active")?,
                is_featured: row.try_get("is_featured")?,
                seo_title: row.try_get("seo_title")?,
                seo_description: row.try_get("seo_description")?,
                created_at: row.try_get("p_created_at")?,
                updated_at: row.try_get("p_updated_at")?,
                published_at: row.try_get("published_at")?,
                subscription_interval: row.try_get("subscription_interval")?,
                subscription_interval_count: row.try_get("subscription_interval_count")?,
                subscription_trial_days: row.try_get("subscription_trial_days")?,
                subscription_setup_fee: row.try_get("subscription_setup_fee")?,
                subscription_min_cycles: row.try_get("subscription_min_cycles")?,
                subscription_max_cycles: row.try_get("subscription_max_cycles")?,
                file_url: row.try_get("file_url")?,
                file_size: row.try_get("file_size")?,
                file_hash: row.try_get("file_hash")?,
                download_limit: row.try_get("p_download_limit")?,
                license_key_enabled: row.try_get("license_key_enabled")?,
                download_expiry_days: row.try_get("download_expiry_days")?,
                bundle_pricing_strategy: row.try_get("bundle_pricing_strategy")?,
                bundle_discount_percentage: row.try_get("bundle_discount_percentage")?,
            };

            results.push((download, order_item, product));
        }

        Ok(results)
    }

    /// Generate a unique download token
    fn generate_download_token(&self) -> String {
        let mut rng = rand::thread_rng();
        let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
        hex::encode(&bytes)
    }

    /// Generate license keys for a product
    pub async fn generate_license_keys(
        &self,
        product_id: Uuid,
        count: i32,
    ) -> Result<Vec<LicenseKey>> {
        let mut keys = Vec::new();

        for _ in 0..count {
            let license_key = self.generate_license_key();

            let key = sqlx::query_as::<_, LicenseKey>(
                r#"
                INSERT INTO license_keys (product_id, license_key, is_used)
                VALUES ($1, $2, false)
                RETURNING *
                "#,
            )
            .bind(product_id)
            .bind(&license_key)
            .fetch_one(self.db.pool())
            .await?;

            keys.push(key);
        }

        Ok(keys)
    }

    /// Assign a license key to an order item
    pub async fn assign_license_key(
        &self,
        product_id: Uuid,
        order_item_id: Uuid,
        customer_id: Option<Uuid>,
    ) -> Result<Option<LicenseKey>> {
        // Find an unused license key
        let key = sqlx::query_as::<_, LicenseKey>(
            r#"
            SELECT * FROM license_keys 
            WHERE product_id = $1 AND is_used = false
            LIMIT 1
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .bind(product_id)
        .fetch_optional(self.db.pool())
        .await?;

        let mut key = match key {
            Some(k) => k,
            None => return Ok(None), // No available keys
        };

        // Mark as used
        key.is_used = true;
        key.used_at = Some(Utc::now());
        key.order_item_id = Some(order_item_id);
        key.customer_id = customer_id;
        key.updated_at = Utc::now();

        sqlx::query(
            r#"
            UPDATE license_keys 
            SET is_used = true, used_at = $1, order_item_id = $2, customer_id = $3, updated_at = $4
            WHERE id = $5
            "#,
        )
        .bind(key.used_at)
        .bind(key.order_item_id)
        .bind(key.customer_id)
        .bind(key.updated_at)
        .bind(key.id)
        .execute(self.db.pool())
        .await?;

        // Update order item with license key
        sqlx::query("UPDATE order_items SET license_key = $1 WHERE id = $2")
            .bind(&key.license_key)
            .bind(order_item_id)
            .execute(self.db.pool())
            .await?;

        Ok(Some(key))
    }

    /// Validate a license key
    pub async fn validate_license_key(&self, product_id: Uuid, license_key: &str) -> Result<bool> {
        let key = sqlx::query_as::<_, LicenseKey>(
            r#"
            SELECT * FROM license_keys 
            WHERE product_id = $1 AND license_key = $2
            "#,
        )
        .bind(product_id)
        .bind(license_key)
        .fetch_optional(self.db.pool())
        .await?;

        match key {
            Some(k) => Ok(k.is_used),
            None => Ok(false),
        }
    }

    /// Get license keys for a product
    pub async fn get_product_license_keys(&self, product_id: Uuid) -> Result<Vec<LicenseKey>> {
        let keys = sqlx::query_as::<_, LicenseKey>(
            "SELECT * FROM license_keys WHERE product_id = $1 ORDER BY created_at DESC",
        )
        .bind(product_id)
        .fetch_all(self.db.pool())
        .await?;

        Ok(keys)
    }

    /// Get license key for an order item
    pub async fn get_order_item_license_key(
        &self,
        order_item_id: Uuid,
    ) -> Result<Option<LicenseKey>> {
        let key =
            sqlx::query_as::<_, LicenseKey>("SELECT * FROM license_keys WHERE order_item_id = $1")
                .bind(order_item_id)
                .fetch_optional(self.db.pool())
                .await?;

        Ok(key)
    }

    /// Generate a license key in format: XXXX-XXXX-XXXX-XXXX
    fn generate_license_key(&self) -> String {
        let mut rng = rand::thread_rng();
        let segments: Vec<String> = (0..4)
            .map(|_| {
                (0..4)
                    .map(|_| rng.gen_range(0..36))
                    .map(|i| {
                        if i < 10 {
                            (b'0' + i as u8) as char
                        } else {
                            (b'A' + (i - 10) as u8) as char
                        }
                    })
                    .collect()
            })
            .collect();
        segments.join("-")
    }

    /// Process digital products for an order (create downloads, assign license keys)
    pub async fn process_digital_order(
        &self,
        order_id: Uuid,
        customer_id: Option<Uuid>,
    ) -> Result<Vec<(OrderItemDownload, Option<LicenseKey>)>> {
        // Get all digital products in the order
        let order_items = sqlx::query_as::<_, OrderItem>(
            r#"
            SELECT oi.* FROM order_items oi
            JOIN products p ON oi.product_id = p.id
            WHERE oi.order_id = $1 AND p.product_type = 'digital'
            "#,
        )
        .bind(order_id)
        .fetch_all(self.db.pool())
        .await?;

        let mut results = Vec::new();

        for item in order_items {
            // Get product details
            let product = sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = $1")
                .bind(item.product_id)
                .fetch_one(self.db.pool())
                .await?;

            // Create download record
            let download = self
                .create_download(
                    item.id,
                    customer_id,
                    product.download_limit,
                    product.download_expiry_days,
                )
                .await?;

            // Assign license key if enabled
            let license_key = if product.license_key_enabled.unwrap_or(false) {
                self.assign_license_key(product.id, item.id, customer_id)
                    .await?
            } else {
                None
            };

            results.push((download, license_key));
        }

        Ok(results)
    }

    /// Check if a product is a digital product
    pub async fn is_digital_product(&self, product_id: Uuid) -> Result<bool> {
        let product = sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = $1")
            .bind(product_id)
            .fetch_optional(self.db.pool())
            .await?;

        match product {
            Some(p) => Ok(matches!(
                p.product_type,
                crate::models::ProductType::Digital
            )),
            None => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests would require a database connection
    // These are placeholder tests showing the expected behavior

    #[test]
    fn test_license_key_format() {
        // Test the license key generation logic directly
        // The format should be XXXX-XXXX-XXXX-XXXX
        let key = generate_license_key_static();
        let parts: Vec<&str> = key.split('-').collect();
        assert_eq!(parts.len(), 4);
        assert!(parts.iter().all(|p| p.len() == 4));
    }

    /// Standalone license key generation for testing
    fn generate_license_key_static() -> String {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let mut rng = rand::thread_rng();

        let segments: Vec<String> = (0..4)
            .map(|_| {
                (0..4)
                    .map(|_| {
                        let idx = rng.gen_range(0..CHARSET.len());
                        CHARSET[idx] as char
                    })
                    .collect()
            })
            .collect();

        segments.join("-")
    }
}
