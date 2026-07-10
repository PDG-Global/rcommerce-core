//! Coupon and Discount model
//!
//! Supports various discount types:
//! - Percentage off (e.g., 20% off)
//! - Fixed amount off (e.g., $10 off)
//! - Free shipping
//! - Buy X Get Y (BOGO)

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

/// Discount type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "discount_type", rename_all = "snake_case")]
pub enum DiscountType {
    /// Percentage off (e.g., 20% off)
    Percentage,
    /// Fixed amount off (e.g., $10 off)
    FixedAmount,
    /// Free shipping
    FreeShipping,
    /// Buy X Get Y
    BuyXGetY,
}

/// Coupon entity
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Coupon {
    pub id: Uuid,
    /// Unique coupon code (e.g., "SUMMER20")
    pub code: String,
    /// Human-readable description
    pub description: Option<String>,
    /// Type of discount
    pub discount_type: DiscountType,
    /// Discount value (percentage or fixed amount)
    pub discount_value: Decimal,
    /// Minimum purchase amount required
    pub minimum_purchase: Option<Decimal>,
    /// Maximum discount amount (for percentage discounts)
    pub maximum_discount: Option<Decimal>,
    /// Whether the coupon is active
    pub is_active: bool,
    /// Start date for coupon validity
    pub starts_at: Option<DateTime<Utc>>,
    /// End date for coupon validity
    pub expires_at: Option<DateTime<Utc>>,
    /// Maximum number of times this coupon can be used globally
    pub usage_limit: Option<i32>,
    /// Maximum uses per customer
    pub usage_limit_per_customer: Option<i32>,
    /// Current usage count
    pub usage_count: i32,
    /// Whether coupon applies to specific products only
    pub applies_to_specific_products: bool,
    /// Whether coupon applies to specific collections only
    pub applies_to_specific_collections: bool,
    /// Whether coupon can be combined with other discounts
    pub can_combine: bool,
    /// Created by user ID
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Coupon application scope - what products/collections the coupon applies to
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CouponApplication {
    pub id: Uuid,
    pub coupon_id: Uuid,
    /// Product ID (if applies to specific product)
    pub product_id: Option<Uuid>,
    /// Collection ID (if applies to specific collection)
    pub collection_id: Option<Uuid>,
    /// Exclude this product/collection (for exclusion rules)
    pub is_exclusion: bool,
    pub created_at: DateTime<Utc>,
}

/// Coupon usage tracking
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CouponUsage {
    pub id: Uuid,
    pub coupon_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub order_id: Uuid,
    pub discount_amount: Decimal,
    pub used_at: DateTime<Utc>,
}

/// Input for creating a coupon
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateCouponInput {
    #[validate(length(min = 1, max = 50))]
    pub code: String,
    #[validate(length(max = 500))]
    pub description: Option<String>,
    pub discount_type: DiscountType,
    // Note: Decimal validation is done in service layer since validator crate doesn't support Decimal
    pub discount_value: Decimal,
    pub minimum_purchase: Option<Decimal>,
    pub maximum_discount: Option<Decimal>,
    pub starts_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub usage_limit: Option<i32>,
    pub usage_limit_per_customer: Option<i32>,
    pub applies_to_specific_products: bool,
    pub applies_to_specific_collections: bool,
    pub can_combine: bool,
}

/// Input for updating a coupon
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateCouponInput {
    #[validate(length(max = 500))]
    pub description: Option<String>,
    pub is_active: Option<bool>,
    pub discount_value: Option<Decimal>,
    pub minimum_purchase: Option<Decimal>,
    pub maximum_discount: Option<Decimal>,
    pub starts_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub usage_limit: Option<i32>,
}

/// Discount calculation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscountCalculation {
    /// Original amount before discount
    pub original_amount: Decimal,
    /// Discount amount
    pub discount_amount: Decimal,
    /// Final amount after discount
    pub final_amount: Decimal,
    /// Applied coupon code
    pub coupon_code: Option<String>,
    /// Description of applied discount
    pub description: String,
}

/// Validation result for coupon application
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CouponValidationResult {
    /// Coupon is valid and can be applied
    Valid,
    /// Coupon code not found
    NotFound,
    /// Coupon is inactive
    Inactive,
    /// Coupon has expired
    Expired,
    /// Coupon has not started yet
    NotStarted,
    /// Usage limit reached
    UsageLimitReached,
    /// Customer usage limit reached
    CustomerUsageLimitReached,
    /// Minimum purchase not met
    MinimumPurchaseNotMet { required: Decimal, current: Decimal },
    /// Coupon does not apply to cart items
    DoesNotApplyToItems,
    /// Cannot combine with other discounts
    CannotCombine,
}

impl Coupon {
    /// Check if coupon is currently valid
    pub fn is_valid(&self, now: DateTime<Utc>) -> bool {
        if !self.is_active {
            return false;
        }

        if let Some(starts_at) = self.starts_at {
            if now < starts_at {
                return false;
            }
        }

        if let Some(expires_at) = self.expires_at {
            if now > expires_at {
                return false;
            }
        }

        if let Some(limit) = self.usage_limit {
            if self.usage_count >= limit {
                return false;
            }
        }

        true
    }

    /// Calculate discount amount for a given subtotal
    pub fn calculate_discount(&self, subtotal: Decimal) -> Decimal {
        match self.discount_type {
            DiscountType::Percentage => {
                let discount = subtotal * (self.discount_value / Decimal::from(100));
                if let Some(max) = self.maximum_discount {
                    discount.min(max)
                } else {
                    discount
                }
            }
            DiscountType::FixedAmount => self.discount_value.min(subtotal),
            DiscountType::FreeShipping => {
                // Free shipping discount is handled separately
                Decimal::ZERO
            }
            DiscountType::BuyXGetY => {
                // BOGO logic is more complex and handled at item level
                Decimal::ZERO
            }
        }
    }

    /// Validate if coupon can be applied to a cart
    pub fn validate_for_cart(
        &self,
        cart_subtotal: Decimal,
        customer_usage_count: i32,
        now: DateTime<Utc>,
    ) -> CouponValidationResult {
        if !self.is_active {
            return CouponValidationResult::Inactive;
        }

        if let Some(starts_at) = self.starts_at {
            if now < starts_at {
                return CouponValidationResult::NotStarted;
            }
        }

        if let Some(expires_at) = self.expires_at {
            if now > expires_at {
                return CouponValidationResult::Expired;
            }
        }

        if let Some(limit) = self.usage_limit {
            if self.usage_count >= limit {
                return CouponValidationResult::UsageLimitReached;
            }
        }

        if let Some(limit) = self.usage_limit_per_customer {
            if customer_usage_count >= limit {
                return CouponValidationResult::CustomerUsageLimitReached;
            }
        }

        if let Some(minimum) = self.minimum_purchase {
            if cart_subtotal < minimum {
                return CouponValidationResult::MinimumPurchaseNotMet {
                    required: minimum,
                    current: cart_subtotal,
                };
            }
        }

        CouponValidationResult::Valid
    }
}

impl CouponValidationResult {
    /// Check if validation passed
    pub fn is_valid(&self) -> bool {
        matches!(self, CouponValidationResult::Valid)
    }

    /// Get error message
    pub fn error_message(&self) -> Option<String> {
        match self {
            CouponValidationResult::Valid => None,
            CouponValidationResult::NotFound => Some("Coupon code not found".to_string()),
            CouponValidationResult::Inactive => Some("Coupon is not active".to_string()),
            CouponValidationResult::Expired => Some("Coupon has expired".to_string()),
            CouponValidationResult::NotStarted => Some("Coupon is not yet valid".to_string()),
            CouponValidationResult::UsageLimitReached => {
                Some("Coupon usage limit reached".to_string())
            }
            CouponValidationResult::CustomerUsageLimitReached => {
                Some("You have already used this coupon".to_string())
            }
            CouponValidationResult::MinimumPurchaseNotMet { required, .. } => {
                Some(format!("Minimum purchase of {} required", required))
            }
            CouponValidationResult::DoesNotApplyToItems => {
                Some("Coupon does not apply to items in your cart".to_string())
            }
            CouponValidationResult::CannotCombine => {
                Some("This coupon cannot be combined with other discounts".to_string())
            }
        }
    }
}
