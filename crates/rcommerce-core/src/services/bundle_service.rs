//! Bundle Product Service
//!
//! Handles bundle product component management, pricing calculations, and cart expansion.

use rust_decimal::Decimal;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    models::{
        BundleComponent, BundleComponentWithProduct, BundlePricingStrategy,
        CreateBundleComponentRequest, OrderItem, Product, ProductType,
        UpdateBundleComponentRequest,
    },
    repository::Database,
    Error, Result,
};

/// Bundle service for managing bundle products
pub struct BundleService {
    db: Database,
}

impl BundleService {
    /// Create a new bundle service
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Add a component to a bundle
    pub async fn add_component(
        &self,
        bundle_product_id: Uuid,
        request: CreateBundleComponentRequest,
    ) -> Result<BundleComponent> {
        // Verify the bundle product exists and is a bundle type
        let bundle = sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = $1")
            .bind(bundle_product_id)
            .fetch_optional(self.db.pool())
            .await?;

        let bundle = bundle.ok_or_else(|| Error::not_found("Bundle product not found"))?;

        if !matches!(bundle.product_type, ProductType::Bundle) {
            return Err(Error::validation("Product is not a bundle type"));
        }

        // Verify the component product exists
        let component = sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = $1")
            .bind(request.component_product_id)
            .fetch_optional(self.db.pool())
            .await?;

        if component.is_none() {
            return Err(Error::not_found("Component product not found"));
        }

        // Prevent adding a bundle to itself
        if bundle_product_id == request.component_product_id {
            return Err(Error::validation("Cannot add a bundle to itself"));
        }

        // Check for circular references (don't allow adding a bundle that contains this bundle)
        if self
            .is_bundle_component(request.component_product_id, bundle_product_id)
            .await?
        {
            return Err(Error::validation("Circular bundle reference detected"));
        }

        // Add the component
        let component = sqlx::query_as::<_, BundleComponent>(
            r#"
            INSERT INTO bundle_components (
                bundle_product_id, component_product_id, quantity, is_optional, sort_order
            )
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (bundle_product_id, component_product_id) DO UPDATE SET
                quantity = EXCLUDED.quantity,
                is_optional = EXCLUDED.is_optional,
                sort_order = EXCLUDED.sort_order,
                updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(bundle_product_id)
        .bind(request.component_product_id)
        .bind(request.quantity)
        .bind(request.is_optional)
        .bind(request.sort_order)
        .fetch_one(self.db.pool())
        .await?;

        Ok(component)
    }

    /// Update a bundle component
    pub async fn update_component(
        &self,
        bundle_product_id: Uuid,
        component_id: Uuid,
        request: UpdateBundleComponentRequest,
    ) -> Result<BundleComponent> {
        // Build dynamic update query
        let mut sets = Vec::new();
        let mut param_count = 0;

        if request.quantity.is_some() {
            param_count += 1;
            sets.push(format!("quantity = ${}", param_count));
        }
        if request.is_optional.is_some() {
            param_count += 1;
            sets.push(format!("is_optional = ${}", param_count));
        }
        if request.sort_order.is_some() {
            param_count += 1;
            sets.push(format!("sort_order = ${}", param_count));
        }

        if sets.is_empty() {
            // Return current component if no updates
            return self
                .get_component(bundle_product_id, component_id)
                .await?
                .ok_or_else(|| Error::not_found("Component not found"));
        }

        sets.push("updated_at = NOW()".to_string());

        let query = format!(
            "UPDATE bundle_components SET {} 
             WHERE id = ${} AND bundle_product_id = ${} 
             RETURNING *",
            sets.join(", "),
            param_count + 1,
            param_count + 2
        );

        let mut query_builder = sqlx::query_as::<_, BundleComponent>(&query);

        if let Some(quantity) = request.quantity {
            query_builder = query_builder.bind(quantity);
        }
        if let Some(is_optional) = request.is_optional {
            query_builder = query_builder.bind(is_optional);
        }
        if let Some(sort_order) = request.sort_order {
            query_builder = query_builder.bind(sort_order);
        }

        query_builder = query_builder.bind(component_id);
        query_builder = query_builder.bind(bundle_product_id);

        let component = query_builder.fetch_one(self.db.pool()).await?;

        Ok(component)
    }

    /// Remove a component from a bundle
    pub async fn remove_component(
        &self,
        bundle_product_id: Uuid,
        component_id: Uuid,
    ) -> Result<bool> {
        let result =
            sqlx::query("DELETE FROM bundle_components WHERE id = $1 AND bundle_product_id = $2")
                .bind(component_id)
                .bind(bundle_product_id)
                .execute(self.db.pool())
                .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get a specific bundle component
    pub async fn get_component(
        &self,
        bundle_product_id: Uuid,
        component_id: Uuid,
    ) -> Result<Option<BundleComponent>> {
        let component = sqlx::query_as::<_, BundleComponent>(
            "SELECT * FROM bundle_components WHERE id = $1 AND bundle_product_id = $2",
        )
        .bind(component_id)
        .bind(bundle_product_id)
        .fetch_optional(self.db.pool())
        .await?;

        Ok(component)
    }

    /// Get all components for a bundle
    pub async fn get_bundle_components(
        &self,
        bundle_product_id: Uuid,
    ) -> Result<Vec<BundleComponentWithProduct>> {
        let rows = sqlx::query(
            r#"
            SELECT 
                bc.*,
                p.id as p_id, p.title, p.slug, p.description, p.sku as p_sku,
                p.product_type, p.price, p.compare_at_price, p.cost_price,
                p.currency, p.inventory_quantity, p.inventory_policy,
                p.inventory_management, p.continues_selling_when_out_of_stock,
                p.weight, p.weight_unit, p.requires_shipping, p.is_active,
                p.is_featured, p.seo_title, p.seo_description,
                p.created_at as p_created_at, p.updated_at as p_updated_at,
                p.published_at, p.subscription_interval, p.subscription_interval_count,
                p.subscription_trial_days, p.subscription_setup_fee,
                p.subscription_min_cycles, p.subscription_max_cycles,
                p.file_url, p.file_size, p.file_hash, p.download_limit,
                p.license_key_enabled, p.download_expiry_days,
                p.bundle_pricing_strategy, p.bundle_discount_percentage
            FROM bundle_components bc
            JOIN products p ON bc.component_product_id = p.id
            WHERE bc.bundle_product_id = $1
            ORDER BY bc.sort_order, bc.created_at
            "#,
        )
        .bind(bundle_product_id)
        .fetch_all(self.db.pool())
        .await?;

        let mut components = Vec::new();
        for row in rows {
            let component = BundleComponent {
                id: row.try_get("id")?,
                bundle_product_id: row.try_get("bundle_product_id")?,
                component_product_id: row.try_get("component_product_id")?,
                quantity: row.try_get("quantity")?,
                is_optional: row.try_get("is_optional")?,
                sort_order: row.try_get("sort_order")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                component_product: None,
            };

            let product = Product {
                id: row.try_get("p_id")?,
                title: row.try_get("title")?,
                slug: row.try_get("slug")?,
                description: row.try_get("description")?,
                sku: row.try_get("p_sku")?,
                product_type: row.try_get("product_type")?,
                price: row.try_get("price")?,
                compare_at_price: row.try_get("compare_at_price")?,
                cost_price: row.try_get("cost_price")?,
                currency: row.try_get("currency")?,
                inventory_quantity: row.try_get("inventory_quantity")?,
                inventory_policy: row.try_get("inventory_policy")?,
                inventory_management: row.try_get("inventory_management")?,
                continues_selling_when_out_of_stock: row
                    .try_get("continues_selling_when_out_of_stock")?,
                weight: row.try_get("weight")?,
                weight_unit: row.try_get("weight_unit")?,
                requires_shipping: row.try_get("requires_shipping")?,
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
                download_limit: row.try_get("download_limit")?,
                license_key_enabled: row.try_get("license_key_enabled")?,
                download_expiry_days: row.try_get("download_expiry_days")?,
                bundle_pricing_strategy: row.try_get("bundle_pricing_strategy")?,
                bundle_discount_percentage: row.try_get("bundle_discount_percentage")?,
            };

            components.push(BundleComponentWithProduct {
                component,
                product: Some(product),
            });
        }

        Ok(components)
    }

    /// Calculate bundle price based on pricing strategy
    pub async fn calculate_bundle_price(&self, bundle_product_id: Uuid) -> Result<Decimal> {
        let bundle = sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = $1")
            .bind(bundle_product_id)
            .fetch_one(self.db.pool())
            .await?;

        let components = self.get_bundle_components(bundle_product_id).await?;

        let strategy = bundle
            .bundle_pricing_strategy
            .unwrap_or(BundlePricingStrategy::Fixed);

        match strategy {
            BundlePricingStrategy::Fixed => {
                // Return the fixed price set on the bundle product
                Ok(bundle.price)
            }
            BundlePricingStrategy::Sum => {
                // Sum of all component prices * quantities
                let total: Decimal = components
                    .iter()
                    .filter(|c| !c.component.is_optional) // Only include required components
                    .map(|c| {
                        let price = c.product.as_ref().map(|p| p.price).unwrap_or(Decimal::ZERO);
                        price * Decimal::from(c.component.quantity)
                    })
                    .sum();
                Ok(total)
            }
            BundlePricingStrategy::PercentageDiscount => {
                // Sum of components minus discount percentage
                let total: Decimal = components
                    .iter()
                    .filter(|c| !c.component.is_optional)
                    .map(|c| {
                        let price = c.product.as_ref().map(|p| p.price).unwrap_or(Decimal::ZERO);
                        price * Decimal::from(c.component.quantity)
                    })
                    .sum();

                let discount_percentage =
                    bundle.bundle_discount_percentage.unwrap_or(Decimal::ZERO);
                let discount = total * (discount_percentage / Decimal::from(100));
                Ok(total - discount)
            }
        }
    }

    /// Expand a bundle into its components for cart/order
    pub async fn expand_bundle(
        &self,
        bundle_product_id: Uuid,
        quantity: i32,
    ) -> Result<Vec<(Product, i32)>> {
        let components = self.get_bundle_components(bundle_product_id).await?;

        let expanded: Vec<(Product, i32)> = components
            .into_iter()
            .filter(|c| !c.component.is_optional) // Skip optional components for now
            .map(|c| {
                let product = c.product.unwrap();
                let component_quantity = c.component.quantity * quantity;
                (product, component_quantity)
            })
            .collect();

        Ok(expanded)
    }

    /// Check if a product is a bundle
    pub async fn is_bundle(&self, product_id: Uuid) -> Result<bool> {
        let product = sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = $1")
            .bind(product_id)
            .fetch_optional(self.db.pool())
            .await?;

        match product {
            Some(p) => Ok(matches!(p.product_type, ProductType::Bundle)),
            None => Ok(false),
        }
    }

    /// Check if a product is a component of a bundle (for circular reference check)
    async fn is_bundle_component(
        &self,
        potential_component_id: Uuid,
        bundle_id: Uuid,
    ) -> Result<bool> {
        // Check if the potential component is a bundle that contains the bundle we're trying to add to
        let is_component = sqlx::query(
            "SELECT 1 FROM bundle_components WHERE bundle_product_id = $1 AND component_product_id = $2"
        )
        .bind(potential_component_id)
        .bind(bundle_id)
        .fetch_optional(self.db.pool())
        .await?;

        Ok(is_component.is_some())
    }

    /// Validate bundle inventory
    pub async fn validate_bundle_inventory(
        &self,
        bundle_product_id: Uuid,
        quantity: i32,
    ) -> Result<bool> {
        let components = self.get_bundle_components(bundle_product_id).await?;

        for component in components {
            if component.component.is_optional {
                continue;
            }

            let product = component.product.unwrap();
            let required = component.component.quantity * quantity;

            if product.inventory_management && product.inventory_quantity < required {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Update bundle product price based on strategy
    pub async fn update_bundle_price(&self, bundle_product_id: Uuid) -> Result<Product> {
        let bundle = sqlx::query_as::<_, Product>("SELECT * FROM products WHERE id = $1")
            .bind(bundle_product_id)
            .fetch_one(self.db.pool())
            .await?;

        // Only auto-update if using Sum or PercentageDiscount strategy
        let strategy = bundle
            .bundle_pricing_strategy
            .unwrap_or(BundlePricingStrategy::Fixed);

        if matches!(strategy, BundlePricingStrategy::Fixed) {
            return Ok(bundle);
        }

        let new_price = self.calculate_bundle_price(bundle_product_id).await?;

        let updated = sqlx::query_as::<_, Product>(
            "UPDATE products SET price = $1, updated_at = NOW() WHERE id = $2 RETURNING *",
        )
        .bind(new_price)
        .bind(bundle_product_id)
        .fetch_one(self.db.pool())
        .await?;

        Ok(updated)
    }

    /// Get bundles containing a specific product
    pub async fn get_bundles_for_product(&self, product_id: Uuid) -> Result<Vec<Product>> {
        let bundles = sqlx::query_as::<_, Product>(
            r#"
            SELECT p.* FROM products p
            JOIN bundle_components bc ON p.id = bc.bundle_product_id
            WHERE bc.component_product_id = $1
            ORDER BY p.title
            "#,
        )
        .bind(product_id)
        .fetch_all(self.db.pool())
        .await?;

        Ok(bundles)
    }

    /// Create order items for a bundle (expands bundle into components)
    pub async fn create_bundle_order_items(
        &self,
        order_id: Uuid,
        bundle_product_id: Uuid,
        parent_item_id: Uuid,
        quantity: i32,
    ) -> Result<Vec<OrderItem>> {
        let components = self.expand_bundle(bundle_product_id, quantity).await?;
        let mut order_items = Vec::new();

        for (product, component_quantity) in components {
            let order_item = sqlx::query_as::<_, OrderItem>(
                r#"
                INSERT INTO order_items (
                    order_id, product_id, variant_id, quantity, price,
                    subtotal, tax_amount, total, sku, title, variant_title,
                    requires_shipping, is_gift_card, weight, weight_unit, image_url,
                    is_digital, bundle_parent_id, is_bundle_component
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
                RETURNING *
                "#
            )
            .bind(order_id)
            .bind(product.id)
            .bind(None::<Uuid>) // variant_id
            .bind(component_quantity)
            .bind(product.price)
            .bind(product.price * Decimal::from(component_quantity))
            .bind(Decimal::ZERO) // tax_amount - calculated separately
            .bind(product.price * Decimal::from(component_quantity))
            .bind(&product.sku)
            .bind(&product.title)
            .bind(None::<String>) // variant_title
            .bind(product.requires_shipping)
            .bind(false) // is_gift_card
            .bind(product.weight)
            .bind(product.weight_unit)
            .bind(None::<String>) // image_url
            .bind(matches!(product.product_type, ProductType::Digital))
            .bind(parent_item_id)
            .bind(true) // is_bundle_component
            .fetch_one(self.db.pool())
            .await?;

            order_items.push(order_item);
        }

        Ok(order_items)
    }
}

#[cfg(test)]
mod tests {

    // Tests would require a database connection
}
