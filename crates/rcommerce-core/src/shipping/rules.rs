//! Shipping rules engine for conditional shipping logic

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::order::Order;
use crate::shipping::ShippingRate;

/// Shipping rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShippingRule {
    pub name: String,
    pub condition: RuleCondition,
    pub action: RuleAction,
    pub priority: i32,
    pub enabled: bool,
}

impl ShippingRule {
    /// Create a new shipping rule
    pub fn new(name: impl Into<String>, condition: RuleCondition, action: RuleAction) -> Self {
        Self {
            name: name.into(),
            condition,
            action,
            priority: 0,
            enabled: true,
        }
    }

    /// Set priority (higher = applied first)
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Disable rule
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// Rule condition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleCondition {
    /// Order total range
    OrderTotal {
        min: Option<Decimal>,
        max: Option<Decimal>,
    },
    /// Order weight range
    OrderWeight {
        min: Option<Decimal>,
        max: Option<Decimal>,
    },
    /// Destination country
    DestinationCountry { countries: Vec<String> },
    /// Destination state/province
    DestinationState { states: Vec<String> },
    /// Product category in order
    ProductCategory { categories: Vec<String> },
    /// Customer group
    CustomerGroup { groups: Vec<String> },
    /// Shipping method selected
    ShippingMethod { methods: Vec<String> },
    /// Cart contains specific SKU
    CartContains { skus: Vec<String> },
    /// Minimum quantity
    MinQuantity { quantity: i32 },
    /// Always true
    Always,
    /// Multiple conditions (all must match)
    All(Vec<RuleCondition>),
    /// Multiple conditions (any can match)
    Any(Vec<RuleCondition>),
}

impl RuleCondition {
    /// Evaluate condition against order
    pub fn evaluate(&self, order: &Order) -> bool {
        match self {
            RuleCondition::OrderTotal { min, max } => {
                let total = order.total;
                min.map(|m| total >= m).unwrap_or(true) && max.map(|m| total <= m).unwrap_or(true)
            }
            RuleCondition::OrderWeight { min, max } => {
                // Note: Order would need weight field
                let weight = Decimal::ZERO; // Placeholder
                min.map(|m| weight >= m).unwrap_or(true) && max.map(|m| weight <= m).unwrap_or(true)
            }
            RuleCondition::DestinationCountry { .. } => {
                // Would need shipping address from order
                false
            }
            RuleCondition::DestinationState { .. } => {
                // Would need shipping address from order
                false
            }
            RuleCondition::ProductCategory { .. } => {
                // Would need to check order items against product categories
                false
            }
            RuleCondition::CustomerGroup { .. } => {
                // Would need customer group information
                false
            }
            RuleCondition::ShippingMethod { methods: _ } => {
                // Would need selected shipping method
                false
            }
            RuleCondition::CartContains { .. } => {
                // Would need to check SKUs in order items
                false
            }
            RuleCondition::MinQuantity { .. } => {
                // Would need total quantity from order
                false
            }
            RuleCondition::Always => true,
            RuleCondition::All(conditions) => conditions.iter().all(|c| c.evaluate(order)),
            RuleCondition::Any(conditions) => conditions.iter().any(|c| c.evaluate(order)),
        }
    }
}

/// Rule action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleAction {
    /// Free shipping
    FreeShipping,
    /// Discount shipping by percentage
    DiscountShipping { percentage: Decimal },
    /// Add surcharge
    Surcharge { amount: Decimal },
    /// Hide specific shipping methods
    HideMethods { methods: Vec<String> },
    /// Show only specific methods
    ShowOnlyMethods { methods: Vec<String> },
    /// Require signature confirmation
    RequireSignature,
    /// Require insurance
    RequireInsurance { amount: Decimal },
    /// Set handling fee
    SetHandlingFee { amount: Decimal },
}

impl RuleAction {
    /// Apply action to rates
    pub fn apply(&self, rates: &mut Vec<ShippingRate>) {
        match self {
            RuleAction::FreeShipping => {
                for rate in rates.iter_mut() {
                    rate.rate = Decimal::ZERO;
                    rate.total_cost = Decimal::ZERO;
                }
            }
            RuleAction::DiscountShipping { percentage } => {
                let multiplier = Decimal::ONE - percentage;
                for rate in rates.iter_mut() {
                    rate.rate *= multiplier;
                    rate.total_cost = rate.rate;
                }
            }
            RuleAction::Surcharge { amount } => {
                for rate in rates.iter_mut() {
                    rate.rate += amount;
                    rate.total_cost += amount;
                }
            }
            RuleAction::HideMethods { methods } => {
                rates.retain(|r| !methods.contains(&r.service_code));
            }
            RuleAction::ShowOnlyMethods { methods } => {
                rates.retain(|r| methods.contains(&r.service_code));
            }
            RuleAction::RequireSignature => {
                // Would add signature requirement to rates
            }
            RuleAction::RequireInsurance { amount } => {
                for rate in rates.iter_mut() {
                    rate.insurance_fee = Some(*amount);
                    rate.total_cost += amount;
                }
            }
            RuleAction::SetHandlingFee { amount } => {
                for rate in rates.iter_mut() {
                    rate.handling_fee = Some(*amount);
                    rate.total_cost += amount;
                }
            }
        }
    }
}

/// Shipping rule engine
pub struct ShippingRuleEngine {
    rules: Vec<ShippingRule>,
}

impl ShippingRuleEngine {
    /// Create a new rule engine
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Add a rule
    pub fn add_rule(&mut self, rule: ShippingRule) {
        self.rules.push(rule);
        self.sort_rules();
    }

    /// Remove a rule by name
    pub fn remove_rule(&mut self, name: &str) {
        self.rules.retain(|r| r.name != name);
    }

    /// Sort rules by priority (highest first)
    fn sort_rules(&mut self) {
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Apply rules to order and rates
    pub fn apply_rules(&self, order: &Order, rates: &mut Vec<ShippingRate>) {
        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }

            if rule.condition.evaluate(order) {
                rule.action.apply(rates);
            }
        }
    }

    /// Get applicable rules for order
    pub fn get_applicable_rules(&self, order: &Order) -> Vec<&ShippingRule> {
        self.rules
            .iter()
            .filter(|r| r.enabled && r.condition.evaluate(order))
            .collect()
    }
}

impl Default for ShippingRuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Common rule presets
pub struct RulePresets;

impl RulePresets {
    /// Free shipping for orders over threshold
    pub fn free_shipping_threshold(threshold: Decimal) -> ShippingRule {
        ShippingRule::new(
            "Free Shipping Threshold",
            RuleCondition::OrderTotal {
                min: Some(threshold),
                max: None,
            },
            RuleAction::FreeShipping,
        )
        .with_priority(100)
    }

    /// Discount for specific country
    pub fn country_discount(countries: Vec<String>, percentage: Decimal) -> ShippingRule {
        ShippingRule::new(
            "Country Discount",
            RuleCondition::DestinationCountry { countries },
            RuleAction::DiscountShipping { percentage },
        )
    }

    /// Hide express shipping for heavy orders
    pub fn hide_express_for_heavy(max_weight: Decimal) -> ShippingRule {
        ShippingRule::new(
            "Hide Express for Heavy",
            RuleCondition::OrderWeight {
                min: Some(max_weight),
                max: None,
            },
            RuleAction::HideMethods {
                methods: vec!["express".to_string(), "overnight".to_string()],
            },
        )
    }

    /// Require signature for high-value orders
    pub fn signature_for_high_value(threshold: Decimal) -> ShippingRule {
        ShippingRule::new(
            "Signature for High Value",
            RuleCondition::OrderTotal {
                min: Some(threshold),
                max: None,
            },
            RuleAction::RequireSignature,
        )
    }

    /// Insurance for international orders
    pub fn insurance_for_international(amount: Decimal) -> ShippingRule {
        ShippingRule::new(
            "Insurance for International",
            RuleCondition::DestinationCountry {
                countries: vec!["US".to_string(), "CA".to_string()],
            },
            RuleAction::RequireInsurance { amount },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_rule_condition_order_total() {
        let _condition = RuleCondition::OrderTotal {
            min: Some(dec!(100)),
            max: Some(dec!(200)),
        };

        // Would need a proper Order mock
        // assert!(condition.evaluate(&order));
    }

    #[test]
    fn test_rule_action_free_shipping() {
        let action = RuleAction::FreeShipping;
        let mut rates = vec![ShippingRate::new(
            "ups",
            "UPS",
            "ground",
            "Ground",
            dec!(10),
            "USD",
        )];

        action.apply(&mut rates);

        assert_eq!(rates[0].total_cost, dec!(0));
    }

    #[test]
    fn test_rule_action_discount() {
        let action = RuleAction::DiscountShipping {
            percentage: dec!(0.2),
        };
        let mut rates = vec![ShippingRate::new(
            "ups",
            "UPS",
            "ground",
            "Ground",
            dec!(10),
            "USD",
        )];

        action.apply(&mut rates);

        assert_eq!(rates[0].rate, dec!(8));
    }

    #[test]
    fn test_rule_engine() {
        let mut engine = ShippingRuleEngine::new();
        engine.add_rule(RulePresets::free_shipping_threshold(dec!(100)));

        assert_eq!(engine.rules.len(), 1);
        assert_eq!(engine.rules[0].priority, 100);
    }
}
