use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::order::{Order, OrderItem};
use crate::{Error, Result};

/// Order calculator for totals, tax, shipping, discounts
pub struct OrderCalculator {
    tax_rate: Decimal,
    shipping_rate: Decimal,
}

impl OrderCalculator {
    pub fn new(tax_rate: Decimal, shipping_rate: Decimal) -> Self {
        Self {
            tax_rate,
            shipping_rate,
        }
    }

    /// Calculate order totals from items
    pub fn calculate_totals(&self, items: &[OrderItem]) -> OrderTotals {
        let mut subtotal = Decimal::ZERO;
        let mut tax_amount = Decimal::ZERO;

        for item in items {
            subtotal += item.subtotal;
            tax_amount += item.tax_amount;
        }

        // Calculate shipping (simplified - based on item weights)
        let shipping_total = self.calculate_shipping(items);

        // Calculate discounts (placeholder)
        let discount_total = self.calculate_discounts(subtotal);

        // Calculate final total
        let total = subtotal + tax_amount + shipping_total - discount_total;

        OrderTotals {
            subtotal,
            tax_total: tax_amount,
            shipping_total,
            discount_total,
            total,
        }
    }

    /// Calculate subtotal for items
    pub fn calculate_subtotal(&self, items: &[OrderItem]) -> Decimal {
        items.iter().map(|item| item.subtotal).sum()
    }

    /// Calculate tax for an order
    pub fn calculate_tax_total(&self, items: &[OrderItem]) -> Decimal {
        items.iter().map(|item| item.tax_amount).sum()
    }

    /// Calculate tax for a specific item
    pub fn calculate_item_tax(&self, item: &OrderItem, tax_rate: Option<Decimal>) -> Decimal {
        let rate = tax_rate.unwrap_or(self.tax_rate);
        item.subtotal * rate
    }

    /// Calculate shipping cost
    pub fn calculate_shipping(&self, items: &[OrderItem]) -> Decimal {
        // Simplified shipping calculation
        // In production, this would consider:
        // - Item weights and dimensions
        // - Shipping destination
        // - Shipping method
        // - Carrier rates

        let total_weight: Decimal = items
            .iter()
            .filter_map(|item| item.weight.map(|w| w * Decimal::from(item.quantity)))
            .sum();

        if total_weight > dec!(0) {
            self.shipping_rate * total_weight
        } else {
            self.shipping_rate * Decimal::from(items.len())
        }
    }

    /// Calculate discounts
    pub fn calculate_discounts(&self, _subtotal: Decimal) -> Decimal {
        // Placeholder for discount logic
        // In production, this would:
        // - Apply discount codes
        // - Check customer group discounts
        // - Apply automatic promotions
        // - Calculate bulk discounts

        Decimal::ZERO
    }

    /// Calculate refund amount
    pub fn calculate_refund(&self, _order: &Order, items_to_refund: &[OrderItem]) -> Decimal {
        // Calculate refund amount
        let items_subtotal: Decimal = items_to_refund.iter().map(|item| item.subtotal).sum();

        let items_tax: Decimal = items_to_refund.iter().map(|item| item.tax_amount).sum();

        items_subtotal + items_tax
    }

    /// Calculate partial refund
    pub fn calculate_partial_refund(&self, order: &Order, amount: Decimal) -> Result<Decimal> {
        if amount > order.total {
            return Err(Error::validation("Refund amount cannot exceed order total"));
        }

        Ok(amount)
    }

    /// Calculate shipping tax
    pub fn calculate_shipping_tax(&self, shipping_total: Decimal) -> Decimal {
        shipping_total * self.tax_rate
    }

    /// Calculate gift wrapping (if applicable)
    pub fn calculate_gift_wrapping(&self, items: &[OrderItem]) -> Decimal {
        // Check if any items have gift wrapping
        let gift_wrapped_count = items
            .iter()
            .filter(|item| {
                item.metadata
                    .get("gift_wrapped")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            })
            .count();

        if gift_wrapped_count > 0 {
            dec!(5.00) * Decimal::from(gift_wrapped_count)
        } else {
            Decimal::ZERO
        }
    }

    /// Calculate insurance (if applicable)
    pub fn calculate_insurance(&self, order: &Order) -> Decimal {
        // Insurance is typically a percentage of order value
        // For high-value orders
        if order.total > dec!(1000) {
            order.total * dec!(0.01) // 1% insurance
        } else {
            Decimal::ZERO
        }
    }

    /// Apply tiered discounts
    pub fn apply_tiered_discount(
        &self,
        subtotal: Decimal,
        tiers: &[(Decimal, Decimal)],
    ) -> Decimal {
        // tiers: Vec of (threshold, discount_percentage)
        for (threshold, discount) in tiers.iter().rev() {
            if subtotal >= *threshold {
                return subtotal * discount;
            }
        }
        Decimal::ZERO
    }

    /// Calculate loyalty points
    pub fn calculate_loyalty_points(&self, order: &Order) -> i32 {
        // 1 point per dollar spent (rounded down)
        order.total.round().to_string().parse().unwrap_or(0)
    }
}

/// Order totals structure
#[derive(Debug, Clone)]
pub struct OrderTotals {
    pub subtotal: Decimal,
    pub tax_total: Decimal,
    pub shipping_total: Decimal,
    pub discount_total: Decimal,
    pub total: Decimal,
}

impl Default for OrderTotals {
    fn default() -> Self {
        Self {
            subtotal: Decimal::ZERO,
            tax_total: Decimal::ZERO,
            shipping_total: Decimal::ZERO,
            discount_total: Decimal::ZERO,
            total: Decimal::ZERO,
        }
    }
}

impl OrderTotals {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_items(calculator: &OrderCalculator, items: &[OrderItem]) -> Self {
        calculator.calculate_totals(items)
    }

    /// Check if totals are valid
    pub fn is_valid(&self) -> bool {
        let calculated_total =
            self.subtotal + self.tax_total + self.shipping_total - self.discount_total;
        self.total == calculated_total
    }

    /// Format for display
    pub fn format(&self) -> String {
        format!(
            "Subtotal: ${:.2}, Tax: ${:.2}, Shipping: ${:.2}, Discount: ${:.2}, Total: ${:.2}",
            self.subtotal, self.tax_total, self.shipping_total, self.discount_total, self.total
        )
    }
}

/// Tax calculator helper
pub struct TaxCalculator {
    default_rate: Decimal,
    exemption_threshold: Option<Decimal>,
}

impl TaxCalculator {
    pub fn new(default_rate: Decimal) -> Self {
        Self {
            default_rate,
            exemption_threshold: None,
        }
    }

    /// Set tax exemption threshold (e.g., for wholesale orders)
    pub fn with_exemption_threshold(mut self, threshold: Decimal) -> Self {
        self.exemption_threshold = Some(threshold);
        self
    }

    /// Calculate tax for amount
    pub fn calculate(&self, amount: Decimal) -> Decimal {
        // Check if exempt
        if let Some(threshold) = self.exemption_threshold {
            if amount >= threshold {
                return Decimal::ZERO;
            }
        }

        amount * self.default_rate
    }

    /// Calculate tax for multiple items with different rates
    pub fn calculate_items(&self, items: &[(Decimal, Decimal)]) -> Decimal {
        items.iter().map(|(amount, rate)| amount * rate).sum()
    }
}

/// Shipping calculator helper
pub struct ShippingCalculator {
    base_rate: Decimal,
    weight_rate: Decimal,
    free_shipping_threshold: Option<Decimal>,
}

impl ShippingCalculator {
    pub fn new(base_rate: Decimal, weight_rate: Decimal) -> Self {
        Self {
            base_rate,
            weight_rate,
            free_shipping_threshold: None,
        }
    }

    /// Set free shipping threshold
    pub fn with_free_shipping_threshold(mut self, threshold: Decimal) -> Self {
        self.free_shipping_threshold = Some(threshold);
        self
    }

    /// Calculate shipping for an order
    pub fn calculate(&self, subtotal: Decimal, total_weight: Decimal) -> Decimal {
        // Check if qualifies for free shipping
        if let Some(threshold) = self.free_shipping_threshold {
            if subtotal >= threshold {
                return Decimal::ZERO;
            }
        }

        self.base_rate + (total_weight * self.weight_rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn test_order_calculator() {
        let calculator = OrderCalculator::new(dec!(0.08), dec!(5.00));

        let items = vec![OrderItem {
            id: Uuid::new_v4(),
            order_id: Uuid::new_v4(),
            product_id: Uuid::new_v4(),
            variant_id: None,
            quantity: 2,
            price: dec!(29.99),
            subtotal: dec!(59.98),
            tax_amount: dec!(4.80),
            total: dec!(64.78),
            sku: None,
            name: "Test Item".to_string(),
            variant_name: None,
            weight: Some(dec!(0.5)),
            weight_unit: None,
            is_bundle_component: None,
            metadata: serde_json::json!({}),
            created_at: Utc::now(),
        }];

        let totals = calculator.calculate_totals(&items);

        assert_eq!(totals.subtotal, dec!(59.98));
        assert_eq!(totals.tax_total, dec!(4.80));
        assert!(totals.total > dec!(64.00));
    }

    #[test]
    fn test_tax_calculator() {
        let calculator = TaxCalculator::new(dec!(0.08));

        assert_eq!(calculator.calculate(dec!(100.00)), dec!(8.00));
        assert_eq!(calculator.calculate(dec!(50.00)), dec!(4.00));
    }

    #[test]
    fn test_shipping_calculator() {
        let calculator = ShippingCalculator::new(dec!(5.00), dec!(1.00))
            .with_free_shipping_threshold(dec!(100.00));

        // Should charge shipping for $50 order
        assert_eq!(calculator.calculate(dec!(50.00), dec!(1.0)), dec!(6.00));

        // Should be free shipping for $100+ order
        assert_eq!(calculator.calculate(dec!(100.00), dec!(1.0)), dec!(0.00));
        assert_eq!(calculator.calculate(dec!(150.00), dec!(2.0)), dec!(0.00));
    }
}
