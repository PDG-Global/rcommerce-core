//! Coupon Service
//!
//! Handles coupon validation, discount calculations, and usage tracking.

use std::sync::Arc;

use chrono::Utc;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::{
    models::{
        CartItem, Coupon, CouponValidationResult, CreateCouponInput, DiscountCalculation,
        UpdateCouponInput,
    },
    repository::{CartRepository, CouponRepository},
    Error, Result,
};

/// Coupon service for managing discounts
#[derive(Clone)]
pub struct CouponService {
    coupon_repo: Arc<dyn CouponRepository>,
    cart_repo: Arc<dyn CartRepository>,
}

impl CouponService {
    /// Create a new coupon service
    pub fn new(coupon_repo: Arc<dyn CouponRepository>, cart_repo: Arc<dyn CartRepository>) -> Self {
        Self {
            coupon_repo,
            cart_repo,
        }
    }

    /// Create a new coupon
    pub async fn create_coupon(
        &self,
        input: CreateCouponInput,
        created_by: Option<Uuid>,
    ) -> Result<Coupon> {
        // Validate code is unique
        if self.coupon_repo.find_by_code(&input.code).await?.is_some() {
            return Err(Error::validation("Coupon code already exists"));
        }

        // Validate dates
        if let (Some(starts), Some(ends)) = (input.starts_at, input.expires_at) {
            if ends <= starts {
                return Err(Error::validation(
                    "Expiration date must be after start date",
                ));
            }
        }

        let coupon = Coupon {
            id: Uuid::new_v4(),
            code: input.code,
            description: input.description,
            discount_type: input.discount_type,
            discount_value: input.discount_value,
            minimum_purchase: input.minimum_purchase,
            maximum_discount: input.maximum_discount,
            is_active: true,
            starts_at: input.starts_at,
            expires_at: input.expires_at,
            usage_limit: input.usage_limit,
            usage_limit_per_customer: input.usage_limit_per_customer,
            usage_count: 0,
            applies_to_specific_products: input.applies_to_specific_products,
            applies_to_specific_collections: input.applies_to_specific_collections,
            can_combine: input.can_combine,
            created_by,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        self.coupon_repo.create(&coupon).await?;
        Ok(coupon)
    }

    /// Get coupon by ID
    pub async fn get_coupon(&self, id: Uuid) -> Result<Coupon> {
        self.coupon_repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::not_found("Coupon not found"))
    }

    /// Get coupon by code
    pub async fn get_coupon_by_code(&self, code: &str) -> Result<Coupon> {
        self.coupon_repo
            .find_by_code(code)
            .await?
            .ok_or_else(|| Error::not_found("Coupon not found"))
    }

    /// Update coupon
    pub async fn update_coupon(&self, id: Uuid, input: UpdateCouponInput) -> Result<Coupon> {
        let mut coupon = self.get_coupon(id).await?;

        if let Some(description) = input.description {
            coupon.description = Some(description);
        }
        if let Some(is_active) = input.is_active {
            coupon.is_active = is_active;
        }
        if let Some(discount_value) = input.discount_value {
            coupon.discount_value = discount_value;
        }
        if let Some(minimum_purchase) = input.minimum_purchase {
            coupon.minimum_purchase = Some(minimum_purchase);
        }
        if let Some(maximum_discount) = input.maximum_discount {
            coupon.maximum_discount = Some(maximum_discount);
        }
        if let Some(starts_at) = input.starts_at {
            coupon.starts_at = Some(starts_at);
        }
        if let Some(expires_at) = input.expires_at {
            coupon.expires_at = Some(expires_at);
        }
        if let Some(usage_limit) = input.usage_limit {
            coupon.usage_limit = Some(usage_limit);
        }

        coupon.updated_at = Utc::now();
        self.coupon_repo.update(&coupon).await?;
        Ok(coupon)
    }

    /// Delete coupon
    pub async fn delete_coupon(&self, id: Uuid) -> Result<()> {
        // Check if coupon has been used
        let usage_count = self.coupon_repo.get_usage_count(id).await?;
        if usage_count > 0 {
            return Err(Error::validation("Cannot delete coupon that has been used"));
        }

        self.coupon_repo.delete(id).await?;
        Ok(())
    }

    /// List all coupons
    pub async fn list_coupons(&self, active_only: bool) -> Result<Vec<Coupon>> {
        if active_only {
            self.coupon_repo.find_active().await
        } else {
            self.coupon_repo.find_all().await
        }
    }

    /// Validate coupon for a cart
    pub async fn validate_coupon(
        &self,
        code: &str,
        cart_id: Uuid,
    ) -> Result<CouponValidationResult> {
        let coupon = match self.coupon_repo.find_by_code(code).await? {
            Some(c) => c,
            None => return Ok(CouponValidationResult::NotFound),
        };

        let cart = self
            .cart_repo
            .find_by_id(cart_id)
            .await?
            .ok_or_else(|| Error::not_found("Cart not found"))?;

        let customer_usage = if let Some(customer_id) = cart.customer_id {
            self.coupon_repo
                .get_customer_usage_count(coupon.id, customer_id)
                .await?
        } else {
            0
        };

        let result = coupon.validate_for_cart(cart.subtotal, customer_usage, Utc::now());

        // Additional validation: check if coupon applies to cart items
        if result.is_valid()
            && (coupon.applies_to_specific_products || coupon.applies_to_specific_collections)
        {
            let items = self.cart_repo.get_items(cart_id).await?;
            let applies = self.check_coupon_applies_to_items(&coupon, &items).await?;

            if !applies {
                return Ok(CouponValidationResult::DoesNotApplyToItems);
            }
        }

        Ok(result)
    }

    /// Check if coupon applies to cart items
    async fn check_coupon_applies_to_items(
        &self,
        coupon: &Coupon,
        items: &[CartItem],
    ) -> Result<bool> {
        let applications = self.coupon_repo.get_applications(coupon.id).await?;

        if applications.is_empty() {
            return Ok(true); // No restrictions
        }

        // Check if any item matches the coupon restrictions
        for item in items {
            for app in &applications {
                if app.is_exclusion {
                    // If it's an exclusion, item must NOT match
                    if app.product_id == Some(item.product_id) {
                        return Ok(false);
                    }
                    // TODO: Check collection exclusions
                } else {
                    // If it's an inclusion, item must match
                    if app.product_id == Some(item.product_id) {
                        return Ok(true);
                    }
                    // TODO: Check collection inclusions
                }
            }
        }

        // If coupon has only inclusions and no items matched, it doesn't apply
        let has_inclusions = applications.iter().any(|a| !a.is_exclusion);
        if has_inclusions {
            Ok(false)
        } else {
            Ok(true) // Only exclusions, and none matched
        }
    }

    /// Calculate discount for a cart
    pub async fn calculate_discount(
        &self,
        coupon: &Coupon,
        subtotal: Decimal,
        items: &[CartItem],
    ) -> Result<Decimal> {
        // If coupon applies to specific items only, calculate based on applicable items
        if coupon.applies_to_specific_products || coupon.applies_to_specific_collections {
            let applications = self.coupon_repo.get_applications(coupon.id).await?;

            let applicable_subtotal: Decimal = items
                .iter()
                .filter(|item| {
                    applications
                        .iter()
                        .any(|app| !app.is_exclusion && app.product_id == Some(item.product_id))
                })
                .map(|item| item.subtotal)
                .sum();

            Ok(coupon.calculate_discount(applicable_subtotal))
        } else {
            // Apply to entire cart
            Ok(coupon.calculate_discount(subtotal))
        }
    }

    /// Record coupon usage
    pub async fn record_usage(
        &self,
        coupon_id: Uuid,
        customer_id: Option<Uuid>,
        order_id: Uuid,
        discount_amount: Decimal,
    ) -> Result<()> {
        self.coupon_repo
            .record_usage(coupon_id, customer_id, order_id, discount_amount)
            .await?;
        self.coupon_repo.increment_usage_count(coupon_id).await?;
        Ok(())
    }

    /// Get discount calculation for display
    pub async fn get_discount_calculation(
        &self,
        code: &str,
        subtotal: Decimal,
    ) -> Result<DiscountCalculation> {
        let coupon = self.get_coupon_by_code(code).await?;
        let discount_amount = coupon.calculate_discount(subtotal);

        Ok(DiscountCalculation {
            original_amount: subtotal,
            discount_amount,
            final_amount: subtotal - discount_amount,
            coupon_code: Some(code.to_string()),
            description: format!("{:?} discount", coupon.discount_type),
        })
    }

    /// Add product to coupon application
    pub async fn add_product_to_coupon(
        &self,
        coupon_id: Uuid,
        product_id: Uuid,
        is_exclusion: bool,
    ) -> Result<()> {
        self.coupon_repo
            .add_application(coupon_id, Some(product_id), None, is_exclusion)
            .await?;
        Ok(())
    }

    /// Add collection to coupon application
    pub async fn add_collection_to_coupon(
        &self,
        coupon_id: Uuid,
        collection_id: Uuid,
        is_exclusion: bool,
    ) -> Result<()> {
        self.coupon_repo
            .add_application(coupon_id, None, Some(collection_id), is_exclusion)
            .await?;
        Ok(())
    }

    /// Remove application from coupon
    pub async fn remove_application(&self, application_id: Uuid) -> Result<()> {
        self.coupon_repo.remove_application(application_id).await?;
        Ok(())
    }

    /// Get coupon statistics
    pub async fn get_coupon_stats(&self, coupon_id: Uuid) -> Result<CouponStats> {
        let coupon = self.get_coupon(coupon_id).await?;
        let total_usage = self.coupon_repo.get_usage_count(coupon_id).await?;
        let total_discount: Decimal = self
            .coupon_repo
            .get_total_discount_amount(coupon_id)
            .await?;

        Ok(CouponStats {
            coupon_id,
            code: coupon.code,
            total_usage,
            total_discount_amount: total_discount,
            remaining_uses: coupon.usage_limit.map(|limit| limit - total_usage),
        })
    }
}

/// Coupon statistics
#[derive(Debug, Clone)]
pub struct CouponStats {
    pub coupon_id: Uuid,
    pub code: String,
    pub total_usage: i32,
    pub total_discount_amount: Decimal,
    pub remaining_uses: Option<i32>,
}
