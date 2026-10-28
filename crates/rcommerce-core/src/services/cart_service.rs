//! Cart Service
//!
//! Handles all cart-related business logic including bundle product expansion,
//! tax calculation, and checkout preparation.

use std::sync::Arc;

use chrono::{Duration, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::{
    models::{
        AddToCartInput, Address, ApplyCouponInput, Cart, CartIdentifier, CartItem, CartWithItems,
        UpdateCartItemInput,
    },
    repository::{CartRepository, CouponRepository, Database},
    services::{BundleService, CouponService},
    tax::{
        CustomerTaxInfo, TaxAddress, TaxContext, TaxService, TaxableItem, TransactionType, VatId,
    },
    Error, Result,
};

/// Cart service for managing shopping carts
pub struct CartService {
    cart_repo: Arc<dyn CartRepository>,
    coupon_repo: Arc<dyn CouponRepository>,
    coupon_service: Arc<CouponService>,
    tax_service: Option<Arc<dyn TaxService>>,
    bundle_service: Option<Arc<BundleService>>,
    db: Option<Database>,
}

impl CartService {
    /// Create a new cart service
    pub fn new(
        cart_repo: Arc<dyn CartRepository>,
        coupon_repo: Arc<dyn CouponRepository>,
        coupon_service: Arc<CouponService>,
    ) -> Self {
        Self {
            cart_repo,
            coupon_repo,
            coupon_service,
            tax_service: None,
            bundle_service: None,
            db: None,
        }
    }

    /// Create a new cart service with tax support
    pub fn with_tax_service(mut self, tax_service: Arc<dyn TaxService>) -> Self {
        self.tax_service = Some(tax_service);
        self
    }

    /// Create a new cart service with bundle support
    pub fn with_bundle_service(mut self, bundle_service: Arc<BundleService>) -> Self {
        self.bundle_service = Some(bundle_service);
        self
    }

    /// Create a new cart service with database access (for bundle expansion)
    pub fn with_database(mut self, db: Database) -> Self {
        self.db = Some(db);
        self
    }

    /// Get or create a cart for a customer or session
    pub async fn get_or_create_cart(
        &self,
        identifier: CartIdentifier,
        currency: &str,
    ) -> Result<Cart> {
        // Try to find existing cart
        let existing_cart = match &identifier {
            CartIdentifier::Customer(customer_id) => {
                self.cart_repo.find_active_by_customer(*customer_id).await?
            }
            CartIdentifier::Session(session_token) => {
                self.cart_repo.find_active_by_session(session_token).await?
            }
            CartIdentifier::CartId(cart_id) => self.cart_repo.find_by_id(*cart_id).await?,
        };

        if let Some(cart) = existing_cart {
            // Update expiration
            self.cart_repo
                .update_expiration(cart.id, Utc::now() + Duration::days(30))
                .await?;
            return Ok(cart);
        }

        // Create new cart
        let cart = self.create_cart(identifier, currency).await?;
        Ok(cart)
    }

    /// Create a new cart
    async fn create_cart(&self, identifier: CartIdentifier, currency: &str) -> Result<Cart> {
        let (customer_id, session_token) = match identifier {
            CartIdentifier::Customer(id) => (Some(id), None),
            CartIdentifier::Session(token) => (None, Some(token)),
            CartIdentifier::CartId(_) => (None, None),
        };

        let cart = Cart {
            id: Uuid::new_v4(),
            customer_id,
            session_token,
            currency: currency.parse().unwrap_or(crate::models::Currency::USD),
            subtotal: Decimal::ZERO,
            discount_total: Decimal::ZERO,
            tax_total: Decimal::ZERO,
            shipping_total: Decimal::ZERO,
            total: Decimal::ZERO,
            coupon_code: None,
            email: None,
            shipping_address_id: None,
            billing_address_id: None,
            shipping_method: None,
            notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            expires_at: Some(Utc::now() + Duration::days(30)),
            converted_to_order: false,
            order_id: None,
        };

        self.cart_repo.create(&cart).await?;
        Ok(cart)
    }

    /// Get cart with items
    pub async fn get_cart_with_items(&self, cart_id: Uuid) -> Result<CartWithItems> {
        let cart = self
            .cart_repo
            .find_by_id(cart_id)
            .await?
            .ok_or_else(|| Error::not_found("Cart not found"))?;

        let items = self.cart_repo.get_items(cart_id).await?;

        Ok(CartWithItems { cart, items })
    }

    /// Get product weights for cart items (product_id -> weight in kg)
    pub async fn get_product_weights(
        &self,
        product_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, Decimal>> {
        use std::collections::HashMap;

        let db = self
            .db
            .as_ref()
            .ok_or_else(|| Error::internal("Database not available"))?;
        let mut weights = HashMap::new();

        for chunk in product_ids.chunks(100) {
            let rows: Vec<(Uuid, Option<Decimal>, Option<String>)> = sqlx::query_as(
                "SELECT id, weight, weight_unit::text AS weight_unit FROM products WHERE id = ANY($1)"
            )
            .bind(chunk)
            .fetch_all(db.pool())
            .await?;

            for (id, weight, weight_unit) in rows {
                if let Some(w) = weight {
                    // Convert to kg if needed
                    let weight_kg = match weight_unit.as_deref() {
                        Some("lb") => w * dec!(0.453592),
                        Some("oz") => w * dec!(0.0283495),
                        _ => w, // Assume kg
                    };
                    weights.insert(id, weight_kg);
                }
            }
        }

        Ok(weights)
    }

    /// Total component weight (kg) for each bundle product in `parent_ids`:
    /// sum of component quantity x component product weight. Parents with no
    /// components (or only weightless ones) are absent from the map. Used to
    /// rate bundle parents that have no weight of their own (e.g. race packs).
    pub async fn get_bundle_component_weights(
        &self,
        parent_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, Decimal>> {
        use std::collections::HashMap;

        let db = self
            .db
            .as_ref()
            .ok_or_else(|| Error::internal("Database not available"))?;
        let mut derived = HashMap::new();

        for chunk in parent_ids.chunks(100) {
            let rows: Vec<(Uuid, Decimal)> = sqlx::query_as(
                r#"
                SELECT bc.bundle_product_id,
                       SUM(bc.quantity * CASE p.weight_unit::text
                           WHEN 'lb' THEN p.weight * 0.453592
                           WHEN 'oz' THEN p.weight * 0.0283495
                           ELSE p.weight
                       END)
                FROM bundle_components bc
                JOIN products p ON bc.component_product_id = p.id
                WHERE bc.bundle_product_id = ANY($1)
                  AND bc.is_optional = false
                  AND p.weight IS NOT NULL
                GROUP BY bc.bundle_product_id
                "#,
            )
            .bind(chunk)
            .fetch_all(db.pool())
            .await?;

            for (parent_id, weight_kg) in rows {
                derived.insert(parent_id, weight_kg);
            }
        }

        Ok(derived)
    }

    /// Get product categories for cart items (product_id -> category_slug)
    pub async fn get_product_categories(
        &self,
        product_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, String>> {
        use std::collections::HashMap;

        let db = self
            .db
            .as_ref()
            .ok_or_else(|| Error::internal("Database not available"))?;
        let mut categories = HashMap::new();

        for chunk in product_ids.chunks(100) {
            let rows: Vec<(Uuid, String)> = sqlx::query_as(
                "SELECT pcr.product_id, c.slug FROM product_category_relations pcr JOIN product_categories c ON pcr.category_id = c.id WHERE pcr.product_id = ANY($1)"
            )
            .bind(chunk)
            .fetch_all(db.pool())
            .await?;

            for (product_id, slug) in rows {
                categories.insert(product_id, slug);
            }
        }

        Ok(categories)
    }

    /// Get cart with calculated totals including tax
    pub async fn get_cart_with_totals(
        &self,
        cart_id: Uuid,
        shipping_address: Option<&Address>,
        vat_id: Option<&str>,
    ) -> Result<CartWithTotals> {
        let cart_with_items = self.get_cart_with_items(cart_id).await?;
        let cart = cart_with_items.cart;
        let items = cart_with_items.items;

        // Calculate tax if address provided and tax service available
        let (tax_total, tax_breakdown) =
            if let (Some(address), Some(_tax_service)) = (shipping_address, &self.tax_service) {
                match self
                    .calculate_cart_tax(&cart, &items, address, vat_id)
                    .await
                {
                    Ok((tax, breakdown)) => (tax, Some(breakdown)),
                    Err(e) => {
                        warn!("Failed to calculate cart tax: {}", e);
                        (Decimal::ZERO, None)
                    }
                }
            } else {
                (Decimal::ZERO, None)
            };

        // Calculate totals
        let total = cart.subtotal - cart.discount_total + tax_total + cart.shipping_total;

        Ok(CartWithTotals {
            cart,
            items,
            tax_total,
            calculated_total: total,
            tax_breakdown,
        })
    }

    /// Add item to cart with product details
    /// Note: Product validation should be done by the caller (API layer)
    pub async fn add_item(
        &self,
        cart_id: Uuid,
        input: AddToCartInput,
        product_details: ProductDetails,
    ) -> Result<CartItem> {
        // Verify cart exists and is not converted
        let cart = self
            .cart_repo
            .find_by_id(cart_id)
            .await?
            .ok_or_else(|| Error::not_found("Cart not found"))?;

        if cart.converted_to_order {
            return Err(Error::validation(
                "Cart has already been converted to an order",
            ));
        }

        // Insert the line, or add to an existing one in the same statement (the
        // repository upserts): a read-modify-write here lost quantity, or failed on
        // the unique index, when the same product was added twice at once
        let mut item = CartItem {
            id: Uuid::new_v4(),
            cart_id,
            product_id: input.product_id,
            variant_id: input.variant_id,
            quantity: input.quantity,
            unit_price: product_details.unit_price,
            original_price: product_details.original_price,
            subtotal: Decimal::ZERO,
            discount_amount: Decimal::ZERO,
            total: Decimal::ZERO,
            sku: product_details.sku,
            title: product_details.title,
            variant_title: product_details.variant_title,
            image_url: product_details.image_url,
            requires_shipping: product_details.requires_shipping,
            is_gift_card: product_details.is_gift_card,
            custom_attributes: input.custom_attributes,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        item.calculate_totals();
        self.cart_repo.add_item(&item).await?;

        // Recalculate cart totals
        self.recalculate_cart(cart_id).await?;

        // Return the line as stored: the insert may have merged into an existing one
        Ok(self
            .cart_repo
            .find_item(cart_id, input.product_id, input.variant_id)
            .await?
            .unwrap_or(item))
    }

    /// Add a bundle product to cart (expands into components)
    pub async fn add_bundle_item(
        &self,
        cart_id: Uuid,
        bundle_product_id: Uuid,
        quantity: i32,
        bundle_details: BundleCartDetails,
    ) -> Result<Vec<CartItem>> {
        // Verify cart exists and is not converted
        let cart = self
            .cart_repo
            .find_by_id(cart_id)
            .await?
            .ok_or_else(|| Error::not_found("Cart not found"))?;

        if cart.converted_to_order {
            return Err(Error::validation(
                "Cart has already been converted to an order",
            ));
        }

        let mut added_items = Vec::new();

        // Add the bundle parent item
        let mut parent_item = CartItem {
            id: Uuid::new_v4(),
            cart_id,
            product_id: bundle_product_id,
            variant_id: None,
            quantity,
            unit_price: bundle_details.bundle_price,
            original_price: bundle_details.original_price,
            subtotal: Decimal::ZERO,
            discount_amount: Decimal::ZERO,
            total: Decimal::ZERO,
            sku: bundle_details.sku,
            title: bundle_details.title,
            variant_title: Some("Bundle".to_string()),
            image_url: bundle_details.image_url,
            requires_shipping: bundle_details.requires_shipping,
            is_gift_card: false,
            custom_attributes: Some(serde_json::json!({
                "is_bundle_parent": true,
                "bundle_product_id": bundle_product_id.to_string(),
            })),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        parent_item.calculate_totals();
        self.cart_repo.add_item(&parent_item).await?;
        added_items.push(parent_item);

        // Add bundle component items
        for component in bundle_details.components {
            let mut item = CartItem {
                id: Uuid::new_v4(),
                cart_id,
                product_id: component.product_id,
                variant_id: None,
                quantity: component.quantity * quantity,
                unit_price: component.unit_price,
                original_price: component.unit_price,
                subtotal: Decimal::ZERO,
                discount_amount: Decimal::ZERO,
                total: Decimal::ZERO,
                sku: component.sku,
                title: component.title,
                variant_title: Some(format!("Bundle Component ({}x)", component.quantity)),
                image_url: component.image_url,
                requires_shipping: component.requires_shipping,
                is_gift_card: false,
                custom_attributes: Some(serde_json::json!({
                    "bundle_parent_id": bundle_product_id.to_string(),
                    "is_bundle_component": true
                })),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };

            item.calculate_totals();
            self.cart_repo.add_item(&item).await?;
            added_items.push(item);
        }

        // Recalculate cart totals
        self.recalculate_cart(cart_id).await?;

        Ok(added_items)
    }

    /// Update cart item quantity
    pub async fn update_item(
        &self,
        cart_id: Uuid,
        item_id: Uuid,
        input: UpdateCartItemInput,
    ) -> Result<CartItem> {
        let mut item = self
            .cart_repo
            .find_item_by_id(item_id)
            .await?
            .ok_or_else(|| Error::not_found("Cart item not found"))?;

        if item.cart_id != cart_id {
            return Err(Error::validation("Item does not belong to cart"));
        }

        if input.quantity == 0 {
            // Remove item
            self.cart_repo.remove_item(item_id).await?;
            self.recalculate_cart(cart_id).await?;
            Ok(item)
        } else {
            item.quantity = input.quantity;
            item.custom_attributes = input.custom_attributes;
            item.calculate_totals();
            self.cart_repo.update_item(&item).await?;
            self.recalculate_cart(cart_id).await?;
            Ok(item)
        }
    }

    /// Remove item from cart
    pub async fn remove_item(&self, cart_id: Uuid, item_id: Uuid) -> Result<()> {
        let item = self
            .cart_repo
            .find_item_by_id(item_id)
            .await?
            .ok_or_else(|| Error::not_found("Cart item not found"))?;

        if item.cart_id != cart_id {
            return Err(Error::validation("Item does not belong to cart"));
        }

        self.cart_repo.remove_item(item_id).await?;
        self.recalculate_cart(cart_id).await?;
        Ok(())
    }

    /// Apply coupon to cart
    pub async fn apply_coupon(&self, cart_id: Uuid, input: ApplyCouponInput) -> Result<Cart> {
        let cart = self
            .cart_repo
            .find_by_id(cart_id)
            .await?
            .ok_or_else(|| Error::not_found("Cart not found"))?;

        if cart.converted_to_order {
            return Err(Error::validation(
                "Cart has already been converted to an order",
            ));
        }

        // Validate coupon
        let validation = self
            .coupon_service
            .validate_coupon(&input.coupon_code, cart_id)
            .await?;

        if !validation.is_valid() {
            return Err(Error::validation(
                validation
                    .error_message()
                    .unwrap_or_else(|| "Invalid coupon".to_string()),
            ));
        }

        // Apply coupon
        let mut updated_cart = cart;
        updated_cart.coupon_code = Some(input.coupon_code);
        self.cart_repo.update(&updated_cart).await?;

        // Recalculate with discount
        self.recalculate_cart(cart_id).await?;

        // Get updated cart
        self.cart_repo
            .find_by_id(cart_id)
            .await?
            .ok_or_else(|| Error::not_found("Cart not found"))
    }

    /// Remove coupon from cart
    pub async fn remove_coupon(&self, cart_id: Uuid) -> Result<Cart> {
        let mut cart = self
            .cart_repo
            .find_by_id(cart_id)
            .await?
            .ok_or_else(|| Error::not_found("Cart not found"))?;

        cart.coupon_code = None;
        self.cart_repo.update(&cart).await?;

        // Recalculate without discount
        self.recalculate_cart(cart_id).await?;

        // Get updated cart
        self.cart_repo
            .find_by_id(cart_id)
            .await?
            .ok_or_else(|| Error::not_found("Cart not found"))
    }

    /// Set shipping address for cart
    pub async fn set_shipping_address(&self, cart_id: Uuid, address_id: Uuid) -> Result<Cart> {
        let mut cart = self
            .cart_repo
            .find_by_id(cart_id)
            .await?
            .ok_or_else(|| Error::not_found("Cart not found"))?;

        cart.shipping_address_id = Some(address_id);
        cart.updated_at = Utc::now();

        self.cart_repo.update(&cart).await?;

        // Recalculate to update tax
        self.recalculate_cart(cart_id).await?;

        self.cart_repo
            .find_by_id(cart_id)
            .await?
            .ok_or_else(|| Error::not_found("Cart not found"))
    }

    /// Set shipping method for cart
    pub async fn set_shipping_method(
        &self,
        cart_id: Uuid,
        method: String,
        cost: Decimal,
    ) -> Result<Cart> {
        let mut cart = self
            .cart_repo
            .find_by_id(cart_id)
            .await?
            .ok_or_else(|| Error::not_found("Cart not found"))?;

        cart.shipping_method = Some(method);
        cart.shipping_total = cost;
        cart.updated_at = Utc::now();

        self.cart_repo.update(&cart).await?;

        // Recalculate totals
        self.recalculate_cart(cart_id).await?;

        self.cart_repo
            .find_by_id(cart_id)
            .await?
            .ok_or_else(|| Error::not_found("Cart not found"))
    }

    /// Mark cart as converted to order
    pub async fn mark_converted(&self, cart_id: Uuid, order_id: Option<Uuid>) -> Result<()> {
        self.cart_repo.mark_converted(cart_id, order_id).await?;
        Ok(())
    }

    /// Recalculate cart totals
    async fn recalculate_cart(&self, cart_id: Uuid) -> Result<()> {
        let items = self.cart_repo.get_items(cart_id).await?;
        let mut cart = self
            .cart_repo
            .find_by_id(cart_id)
            .await?
            .ok_or_else(|| Error::not_found("Cart not found"))?;

        // Calculate subtotals (only count parent items, not bundle components)
        cart.subtotal = items
            .iter()
            .filter(|i| !self.is_bundle_component(i))
            .map(|i| i.subtotal)
            .sum();

        // Calculate discount if coupon applied
        cart.discount_total = if let Some(ref coupon_code) = cart.coupon_code {
            if let Ok(Some(coupon)) = self.coupon_repo.find_by_code(coupon_code).await {
                self.coupon_service
                    .calculate_discount(&coupon, cart.subtotal, &items)
                    .await?
            } else {
                Decimal::ZERO
            }
        } else {
            Decimal::ZERO
        };

        // Apply discount to items proportionally (only to parent items)
        let parent_items: Vec<_> = items
            .iter()
            .filter(|i| !self.is_bundle_component(i))
            .cloned()
            .collect();
        for item in parent_items {
            let item_ratio = if cart.subtotal > Decimal::ZERO {
                item.subtotal / cart.subtotal
            } else {
                Decimal::ZERO
            };
            let discount_amount = (cart.discount_total * item_ratio).round_dp(2);
            // Only the discount changes here: writing the whole line read above would
            // overwrite a quantity added concurrently (and costs one UPDATE per line
            // on every cart change)
            if item.discount_amount != discount_amount {
                self.cart_repo
                    .set_item_discount(item.id, discount_amount)
                    .await?;
            }
        }

        // Calculate tax if we have shipping address and tax service
        cart.tax_total = if let (Some(_address_id), Some(_tax_service)) =
            (cart.shipping_address_id, &self.tax_service)
        {
            // TODO: Fetch address and calculate tax
            // For now, tax will be calculated on-demand via get_cart_with_totals
            Decimal::ZERO
        } else {
            Decimal::ZERO
        };

        // Calculate final total
        cart.total = cart.subtotal - cart.discount_total + cart.tax_total + cart.shipping_total;
        cart.updated_at = Utc::now();

        self.cart_repo.update(&cart).await?;
        Ok(())
    }

    /// Calculate tax for cart items
    async fn calculate_cart_tax(
        &self,
        cart: &Cart,
        items: &[CartItem],
        shipping_address: &Address,
        vat_id: Option<&str>,
    ) -> Result<(Decimal, Vec<TaxBreakdownItem>)> {
        let tax_service = self
            .tax_service
            .as_ref()
            .ok_or_else(|| Error::not_implemented("Tax service not configured"))?;

        // Convert cart items to taxable items
        let taxable_items: Vec<TaxableItem> = items
            .iter()
            .filter(|i| !self.is_bundle_component(i))
            .map(|item| TaxableItem {
                id: item.id,
                product_id: item.product_id,
                quantity: item.quantity,
                unit_price: item.unit_price,
                total_price: item.total,
                tax_category_id: None, // TODO: Get from product
                is_digital: false,     // TODO: Get from product
                title: item.title.clone(),
                sku: item.sku.clone(),
            })
            .collect();

        // Build tax context
        let tax_context = TaxContext {
            customer: CustomerTaxInfo {
                customer_id: cart.customer_id,
                is_tax_exempt: false, // TODO: Check customer exemptions
                vat_id: vat_id.and_then(|v| VatId::parse(v).ok()),
                exemptions: vec![],
            },
            shipping_address: address_to_tax_address(shipping_address),
            billing_address: address_to_tax_address(shipping_address), // Use shipping as billing for now
            currency: cart.currency,
            transaction_type: if vat_id.is_some() {
                TransactionType::B2B
            } else {
                TransactionType::B2C
            },
        };

        // Calculate tax
        let calculation = tax_service
            .calculate_tax(&taxable_items, &tax_context)
            .await?;

        let total_tax: Decimal = calculation.line_items.iter().map(|li| li.tax_amount).sum();

        // Convert to breakdown items
        let breakdown: Vec<TaxBreakdownItem> = calculation
            .tax_breakdown
            .iter()
            .map(|tb| TaxBreakdownItem {
                tax_zone_name: tb.tax_zone_name.clone(),
                tax_rate_name: tb.tax_rate_name.clone(),
                rate: tb.rate,
                taxable_amount: tb.taxable_amount,
                tax_amount: tb.tax_amount,
            })
            .collect();

        debug!(
            "Cart tax calculated: total_tax={}, breakdown_items={}",
            total_tax,
            breakdown.len()
        );

        Ok((total_tax, breakdown))
    }

    /// Check if a cart item is a bundle component
    fn is_bundle_component(&self, item: &CartItem) -> bool {
        item.custom_attributes
            .as_ref()
            .and_then(|attrs| attrs.get("is_bundle_component"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Merge guest cart into customer cart
    pub async fn merge_carts(&self, session_token: &str, customer_id: Uuid) -> Result<Cart> {
        let guest_cart = self.cart_repo.find_active_by_session(session_token).await?;

        let customer_cart = self.cart_repo.find_active_by_customer(customer_id).await?;

        match (guest_cart, customer_cart) {
            (Some(guest), Some(customer)) => {
                // Merge items from guest cart into customer cart
                let guest_items = self.cart_repo.get_items(guest.id).await?;

                for item in guest_items {
                    // Add item to customer cart
                    let new_item = CartItem {
                        id: Uuid::new_v4(),
                        cart_id: customer.id,
                        product_id: item.product_id,
                        variant_id: item.variant_id,
                        quantity: item.quantity,
                        unit_price: item.unit_price,
                        original_price: item.original_price,
                        subtotal: item.subtotal,
                        discount_amount: item.discount_amount,
                        total: item.total,
                        sku: item.sku,
                        title: item.title,
                        variant_title: item.variant_title,
                        image_url: item.image_url,
                        requires_shipping: item.requires_shipping,
                        is_gift_card: item.is_gift_card,
                        custom_attributes: item.custom_attributes,
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                    };
                    self.cart_repo.add_item(&new_item).await?;
                }

                // Mark guest cart as converted
                self.cart_repo.mark_converted(guest.id, None).await?;

                // Apply coupon from guest cart if customer cart doesn't have one
                if guest.coupon_code.is_some() && customer.coupon_code.is_none() {
                    if let Some(coupon_code) = guest.coupon_code {
                        let _ = self
                            .apply_coupon(customer.id, ApplyCouponInput { coupon_code })
                            .await;
                    }
                }

                self.recalculate_cart(customer.id).await?;

                self.cart_repo
                    .find_by_id(customer.id)
                    .await?
                    .ok_or_else(|| Error::not_found("Cart not found"))
            }
            (Some(guest), None) => {
                // Transfer guest cart to customer
                self.cart_repo
                    .assign_customer(guest.id, customer_id)
                    .await?;

                self.cart_repo
                    .find_by_id(guest.id)
                    .await?
                    .ok_or_else(|| Error::not_found("Cart not found"))
            }
            (None, Some(customer)) => Ok(customer),
            (None, None) => {
                // Create new cart for customer
                self.get_or_create_cart(CartIdentifier::Customer(customer_id), "USD")
                    .await
            }
        }
    }

    /// Clear cart (remove all items)
    pub async fn clear_cart(&self, cart_id: Uuid) -> Result<()> {
        self.cart_repo.clear_items(cart_id).await?;
        self.recalculate_cart(cart_id).await?;
        Ok(())
    }

    /// Delete cart
    pub async fn delete_cart(&self, cart_id: Uuid) -> Result<()> {
        self.cart_repo.delete(cart_id).await?;
        Ok(())
    }
}

/// Product details needed to add item to cart
#[derive(Debug, Clone)]
pub struct ProductDetails {
    pub unit_price: Decimal,
    pub original_price: Decimal,
    pub sku: Option<String>,
    pub title: String,
    pub variant_title: Option<String>,
    pub image_url: Option<String>,
    pub requires_shipping: bool,
    pub is_gift_card: bool,
}

/// Bundle component details for cart
#[derive(Debug, Clone)]
pub struct BundleComponentDetails {
    pub product_id: Uuid,
    pub quantity: i32,
    pub unit_price: Decimal,
    pub sku: Option<String>,
    pub title: String,
    pub image_url: Option<String>,
    pub requires_shipping: bool,
}

/// Bundle details for adding to cart
#[derive(Debug, Clone)]
pub struct BundleCartDetails {
    pub bundle_price: Decimal,
    pub original_price: Decimal,
    pub sku: Option<String>,
    pub title: String,
    pub image_url: Option<String>,
    pub requires_shipping: bool,
    pub components: Vec<BundleComponentDetails>,
}

/// Cart with calculated totals
#[derive(Debug, Clone)]
pub struct CartWithTotals {
    pub cart: Cart,
    pub items: Vec<CartItem>,
    pub tax_total: Decimal,
    pub calculated_total: Decimal,
    pub tax_breakdown: Option<Vec<TaxBreakdownItem>>,
}

/// Tax breakdown item
#[derive(Debug, Clone)]
pub struct TaxBreakdownItem {
    pub tax_zone_name: String,
    pub tax_rate_name: String,
    pub rate: Decimal,
    pub taxable_amount: Decimal,
    pub tax_amount: Decimal,
}

/// Convert Address to TaxAddress
fn address_to_tax_address(address: &Address) -> TaxAddress {
    TaxAddress {
        country_code: address.country.clone(),
        region_code: address.state.clone(),
        postal_code: Some(address.zip.clone()),
        city: Some(address.city.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_conversion() {
        let address = Address {
            id: Uuid::new_v4(),
            customer_id: Uuid::new_v4(),
            first_name: "John".to_string(),
            last_name: "Doe".to_string(),
            company: None,
            phone: None,
            address1: "123 Main St".to_string(),
            address2: None,
            city: "Berlin".to_string(),
            state: Some("BE".to_string()),
            country: "DE".to_string(),
            zip: "10115".to_string(),
            is_default_shipping: true,
            is_default_billing: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let tax_addr = address_to_tax_address(&address);
        assert_eq!(tax_addr.country_code, "DE");
        assert_eq!(tax_addr.region_code, Some("BE".to_string()));
        assert_eq!(tax_addr.postal_code, Some("10115".to_string()));
    }

    /// DB-backed: requires PostgreSQL via TEST_DATABASE_URL; skipped without it
    #[tokio::test]
    async fn test_simultaneous_adds_of_one_product_all_count() {
        use crate::repository::{PgCartRepository, PgCouponRepository};

        let url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres@localhost:5432/rcommerce_scratch".to_string());
        let pool = match sqlx::postgres::PgPoolOptions::new()
            .max_connections(10)
            .connect(&url)
            .await
        {
            Ok(pool) => pool,
            Err(e) => {
                eprintln!("Skipping cart concurrency test: cannot connect to {url}: {e}");
                return;
            }
        };
        if let Err(e) = crate::Migrator::new(pool.clone()).migrate().await {
            eprintln!("Skipping cart concurrency test: migration failed: {e}");
            return;
        }

        let product_id = Uuid::new_v4();
        sqlx::query("INSERT INTO products (id, title, slug, price) VALUES ($1, 'Concurrent add', $2, 10.00)")
            .bind(product_id)
            .bind(format!("concurrent-add-{product_id}"))
            .execute(&pool)
            .await
            .unwrap();

        let cart_repo: Arc<dyn CartRepository> = Arc::new(PgCartRepository::new(pool.clone()));
        let coupon_repo: Arc<dyn CouponRepository> =
            Arc::new(PgCouponRepository::new(pool.clone()));
        let coupon_service = Arc::new(CouponService::new(coupon_repo.clone(), cart_repo.clone()));
        let service = Arc::new(CartService::new(cart_repo, coupon_repo, coupon_service));
        let cart = service
            .get_or_create_cart(
                CartIdentifier::Session(format!("sess_{}", Uuid::new_v4())),
                "USD",
            )
            .await
            .unwrap();

        let adds: Vec<_> = (0..6)
            .map(|_| {
                let service = service.clone();
                tokio::spawn(async move {
                    service
                        .add_item(
                            cart.id,
                            AddToCartInput {
                                product_id,
                                variant_id: None,
                                quantity: 1,
                                custom_attributes: None,
                            },
                            ProductDetails {
                                unit_price: dec!(10.00),
                                original_price: dec!(10.00),
                                sku: None,
                                title: "Concurrent add".to_string(),
                                variant_title: None,
                                image_url: None,
                                requires_shipping: true,
                                is_gift_card: false,
                            },
                        )
                        .await
                })
            })
            .collect();
        for add in adds {
            add.await.unwrap().expect("every add must succeed");
        }

        let lines = service.get_cart_with_items(cart.id).await.unwrap().items;
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].quantity, 6);
        assert_eq!(lines[0].subtotal, dec!(60.00));

        sqlx::query("DELETE FROM carts WHERE id = $1")
            .bind(cart.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM products WHERE id = $1")
            .bind(product_id)
            .execute(&pool)
            .await
            .unwrap();
    }
}
