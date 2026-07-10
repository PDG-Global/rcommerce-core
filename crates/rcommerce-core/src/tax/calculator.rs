//! Tax Calculator
//!
//! Core tax calculation logic for the tax system.

use rust_decimal::Decimal;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::tax::{
    CustomerTaxInfo, TaxAddress, TaxCategory, TaxContext, TaxRate, TaxZone, TaxableItem,
    TransactionType,
};
use crate::Result;

/// Tax calculation result
#[derive(Debug, Clone)]
pub struct TaxCalculation {
    /// Tax for each line item
    pub line_items: Vec<LineItemTax>,
    /// Tax on shipping
    pub shipping_tax: Decimal,
    /// Total tax amount
    pub total_tax: Decimal,
    /// Breakdown by tax rate
    pub tax_breakdown: Vec<TaxBreakdown>,
}

impl TaxCalculation {
    /// Create empty calculation
    pub fn new() -> Self {
        Self {
            line_items: Vec::new(),
            shipping_tax: Decimal::ZERO,
            total_tax: Decimal::ZERO,
            tax_breakdown: Vec::new(),
        }
    }

    /// Check if calculation is empty (no tax)
    pub fn is_empty(&self) -> bool {
        self.total_tax == Decimal::ZERO
    }
}

impl Default for TaxCalculation {
    fn default() -> Self {
        Self::new()
    }
}

/// Tax for a single line item
#[derive(Debug, Clone)]
pub struct LineItemTax {
    /// Item ID
    pub item_id: Uuid,
    /// Taxable amount
    pub taxable_amount: Decimal,
    /// Tax amount
    pub tax_amount: Decimal,
    /// Tax rate applied
    pub tax_rate: Decimal,
    /// Tax rate ID
    pub tax_rate_id: Uuid,
    /// Tax zone ID
    pub tax_zone_id: Uuid,
}

/// Tax breakdown by jurisdiction
#[derive(Debug, Clone)]
pub struct TaxBreakdown {
    /// Tax zone ID
    pub tax_zone_id: Uuid,
    /// Tax zone name
    pub tax_zone_name: String,
    /// Tax rate ID
    pub tax_rate_id: Uuid,
    /// Tax rate name
    pub tax_rate_name: String,
    /// Tax rate percentage
    pub rate: Decimal,
    /// Total taxable amount
    pub taxable_amount: Decimal,
    /// Total tax amount
    pub tax_amount: Decimal,
}

/// Tax calculator
pub struct TaxCalculator {
    /// Tax rates for the destination
    rates: Vec<TaxRate>,
    /// Tax zones
    zones: Vec<TaxZone>,
    /// Tax categories
    #[allow(dead_code)]
    categories: Vec<TaxCategory>,
}

impl TaxCalculator {
    /// Create new calculator with tax data
    pub fn new(rates: Vec<TaxRate>, zones: Vec<TaxZone>, categories: Vec<TaxCategory>) -> Self {
        Self {
            rates,
            zones,
            categories,
        }
    }

    /// Calculate tax for items
    pub fn calculate(&self, items: &[TaxableItem], context: &TaxContext) -> Result<TaxCalculation> {
        // Check if customer is tax exempt
        if context.customer.is_tax_exempt {
            info!("Customer is tax exempt, returning zero tax");
            return Ok(TaxCalculation::new());
        }

        // Determine applicable tax zone
        let tax_zone = self.determine_tax_zone(&context.shipping_address)?;
        debug!("Using tax zone: {} ({})", tax_zone.name, tax_zone.code);

        // Check for B2B reverse charge
        if context.transaction_type == TransactionType::B2B {
            if let Some(vat_id) = &context.customer.vat_id {
                if vat_id.is_validated {
                    // Check if reverse charge applies
                    if let Some(rate) = self.get_b2b_rate(&tax_zone) {
                        if rate.reverse_charge {
                            info!("B2B reverse charge applies, customer accounts for VAT");
                            return Ok(TaxCalculation::new());
                        }
                    }
                }
            }
        }

        let mut line_items = Vec::new();
        let mut breakdown_map: std::collections::HashMap<Uuid, (TaxRate, Decimal, Decimal)> =
            std::collections::HashMap::new();

        for item in items {
            let item_tax = self.calculate_item_tax(item, &tax_zone, context)?;

            // Aggregate for breakdown
            let entry = breakdown_map
                .entry(item_tax.tax_rate_id)
                .or_insert_with(|| {
                    let rate = self
                        .rates
                        .iter()
                        .find(|r| r.id == item_tax.tax_rate_id)
                        .cloned()
                        .unwrap_or_else(|| TaxRate {
                            id: Uuid::nil(),
                            name: "Zero Rate".to_string(),
                            tax_zone_id: tax_zone.id,
                            tax_category_id: None,
                            rate: Decimal::ZERO,
                            rate_type: "percentage".to_string(),
                            is_vat: false,
                            vat_type: None,
                            b2b_exempt: false,
                            reverse_charge: false,
                            valid_from: chrono::Local::now().date_naive(),
                            valid_until: None,
                            priority: 0,
                            created_at: chrono::Utc::now(),
                            updated_at: chrono::Utc::now(),
                        });
                    (rate, Decimal::ZERO, Decimal::ZERO)
                });
            entry.1 += item_tax.taxable_amount;
            entry.2 += item_tax.tax_amount;

            line_items.push(item_tax);
        }

        // Build breakdown
        let tax_breakdown: Vec<TaxBreakdown> = breakdown_map
            .into_iter()
            .map(|(_, (rate, taxable, tax))| TaxBreakdown {
                tax_zone_id: rate.tax_zone_id,
                tax_zone_name: self
                    .zones
                    .iter()
                    .find(|z| z.id == rate.tax_zone_id)
                    .map(|z| z.name.clone())
                    .unwrap_or_default(),
                tax_rate_id: rate.id,
                tax_rate_name: rate.name.clone(),
                rate: rate.rate,
                taxable_amount: taxable,
                tax_amount: tax,
            })
            .collect();

        let total_tax: Decimal = line_items.iter().map(|li| li.tax_amount).sum();

        Ok(TaxCalculation {
            line_items,
            shipping_tax: Decimal::ZERO, // TODO: Calculate shipping tax
            total_tax,
            tax_breakdown,
        })
    }

    /// Calculate tax for a single item
    fn calculate_item_tax(
        &self,
        item: &TaxableItem,
        tax_zone: &TaxZone,
        context: &TaxContext,
    ) -> Result<LineItemTax> {
        let taxable_amount = item.total_price;

        // Find applicable tax rate
        let tax_rate = self.find_tax_rate(tax_zone, item.tax_category_id, context)?;

        // Check for exemption
        if let Some(exemption) = self.find_exemption(&context.customer, tax_zone) {
            if exemption.is_active {
                info!(
                    "Tax exemption applies for customer {} in zone {}",
                    context.customer.customer_id.unwrap_or_default(),
                    tax_zone.code
                );
                return Ok(LineItemTax {
                    item_id: item.id,
                    taxable_amount,
                    tax_amount: Decimal::ZERO,
                    tax_rate: Decimal::ZERO,
                    tax_rate_id: tax_rate.id,
                    tax_zone_id: tax_zone.id,
                });
            }
        }

        // Calculate tax
        let tax_amount = match tax_rate.rate_type.as_str() {
            "percentage" => (taxable_amount * tax_rate.rate).round_dp(2),
            "fixed" => tax_rate.rate * Decimal::from(item.quantity),
            _ => {
                warn!("Unknown tax rate type: {}", tax_rate.rate_type);
                Decimal::ZERO
            }
        };

        Ok(LineItemTax {
            item_id: item.id,
            taxable_amount,
            tax_amount,
            tax_rate: tax_rate.rate,
            tax_rate_id: tax_rate.id,
            tax_zone_id: tax_zone.id,
        })
    }

    /// Find applicable tax rate
    fn find_tax_rate(
        &self,
        tax_zone: &TaxZone,
        category_id: Option<Uuid>,
        _context: &TaxContext,
    ) -> Result<TaxRate> {
        let now = chrono::Local::now().date_naive();

        // Filter rates by zone, validity, and priority
        let mut applicable_rates: Vec<&TaxRate> = self
            .rates
            .iter()
            .filter(|r| {
                r.tax_zone_id == tax_zone.id
                    && r.valid_from <= now
                    && r.valid_until.map_or(true, |until| until >= now)
            })
            .collect();

        // Sort by priority (highest first)
        applicable_rates.sort_by(|a, b| b.priority.cmp(&a.priority));

        // Try to find rate for specific category
        if let Some(cat_id) = category_id {
            if let Some(rate) = applicable_rates
                .iter()
                .find(|r| r.tax_category_id == Some(cat_id))
            {
                return Ok((*rate).clone());
            }
        }

        // Fall back to default rate (no category)
        if let Some(rate) = applicable_rates
            .iter()
            .find(|r| r.tax_category_id.is_none())
        {
            return Ok((*rate).clone());
        }

        // No rate found - return zero rate
        warn!(
            "No tax rate found for zone {} and category {:?}",
            tax_zone.code, category_id
        );

        Ok(TaxRate {
            id: Uuid::nil(),
            name: "Zero Rate".to_string(),
            tax_zone_id: tax_zone.id,
            tax_category_id: category_id,
            rate: Decimal::ZERO,
            rate_type: "percentage".to_string(),
            is_vat: false,
            vat_type: None,
            b2b_exempt: false,
            reverse_charge: false,
            valid_from: now,
            valid_until: None,
            priority: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }

    /// Determine tax zone from address
    fn determine_tax_zone(&self, address: &TaxAddress) -> Result<TaxZone> {
        // Try to find most specific zone first

        // 1. Try postal code pattern match
        if let Some(postal) = &address.postal_code {
            if let Some(zone) = self.zones.iter().find(|z| {
                z.country_code == address.country_code
                    && z.postal_code_pattern.as_ref().is_some_and(|pattern| {
                        regex::Regex::new(pattern).is_ok_and(|re| re.is_match(postal))
                    })
            }) {
                return Ok(zone.clone());
            }
        }

        // 2. Try region (state/province) match
        if let Some(region) = &address.region_code {
            if let Some(zone) = self.zones.iter().find(|z| {
                z.country_code == address.country_code && z.region_code.as_ref() == Some(region)
            }) {
                return Ok(zone.clone());
            }
        }

        // 3. Fall back to country-level zone
        if let Some(zone) = self
            .zones
            .iter()
            .find(|z| z.country_code == address.country_code && z.region_code.is_none())
        {
            return Ok(zone.clone());
        }

        // 4. Create default zone if none found
        warn!(
            "No tax zone found for country {}, using default",
            address.country_code
        );

        Ok(TaxZone {
            id: Uuid::nil(),
            name: format!("Default {}", address.country_code),
            code: address.country_code.clone(),
            country_code: address.country_code.clone(),
            region_code: None,
            postal_code_pattern: None,
            zone_type: "country".to_string(),
            parent_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }

    /// Get B2B tax rate for reverse charge determination
    fn get_b2b_rate(&self, tax_zone: &TaxZone) -> Option<&TaxRate> {
        let now = chrono::Local::now().date_naive();

        self.rates.iter().find(|r| {
            r.tax_zone_id == tax_zone.id
                && r.valid_from <= now
                && r.valid_until.map_or(true, |until| until >= now)
        })
    }

    /// Find tax exemption for customer
    fn find_exemption(
        &self,
        _customer: &CustomerTaxInfo,
        _tax_zone: &TaxZone,
    ) -> Option<crate::tax::TaxExemption> {
        // TODO: Implement exemption lookup from database
        None
    }

    /// Calculate shipping tax
    pub fn calculate_shipping_tax(
        &self,
        shipping_amount: Decimal,
        destination: &TaxAddress,
    ) -> Result<Decimal> {
        let tax_zone = self.determine_tax_zone(destination)?;

        // Find shipping tax rate (usually same as goods, but can be different)
        let now = chrono::Local::now().date_naive();

        let shipping_rate = self.rates.iter().find(|r| {
            r.tax_zone_id == tax_zone.id
                && r.tax_category_id.is_none() // Default rate
                && r.valid_from <= now
                && r.valid_until.map_or(true, |until| until >= now)
        });

        if let Some(rate) = shipping_rate {
            let tax = (shipping_amount * rate.rate).round_dp(2);
            Ok(tax)
        } else {
            Ok(Decimal::ZERO)
        }
    }
}

/// Check if OSS (One Stop Shop) applies
pub fn check_oss_applicability(
    seller_country: &str,
    customer_country: &str,
    total_sales_to_country: Decimal,
) -> bool {
    use crate::tax::is_eu_country;

    // Must be intra-EU sale
    if !is_eu_country(seller_country) || !is_eu_country(customer_country) {
        return false;
    }

    // Same country - no OSS
    if seller_country == customer_country {
        return false;
    }

    // OSS applies if seller exceeds €10,000 threshold in destination country
    // or if seller chooses to use OSS
    let threshold = Decimal::from(10000);
    total_sales_to_country >= threshold
}

/// Determine if tax should be collected based on economic nexus
pub fn check_economic_nexus(
    state_code: &str,
    sales_in_state: Decimal,
    transaction_count: i32,
) -> bool {
    use crate::tax::models::get_us_economic_nexus_thresholds;

    let thresholds = get_us_economic_nexus_thresholds();

    if let Some(threshold) = thresholds.iter().find(|t| t.state_code == state_code) {
        match threshold.threshold_type.as_str() {
            "revenue" => sales_in_state >= threshold.threshold_amount,
            "transactions" => {
                if let Some(txn_threshold) = threshold.transaction_threshold {
                    transaction_count >= txn_threshold
                } else {
                    false
                }
            }
            "both" => {
                let revenue_met = sales_in_state >= threshold.threshold_amount;
                let transactions_met = threshold
                    .transaction_threshold
                    .is_some_and(|t| transaction_count >= t);
                revenue_met || transactions_met
            }
            _ => false,
        }
    } else {
        // No threshold data - assume nexus exists to be safe
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use rust_decimal_macros::dec;

    fn create_test_calculator() -> TaxCalculator {
        let zones = vec![TaxZone {
            id: Uuid::new_v4(),
            name: "Germany".to_string(),
            code: "DE".to_string(),
            country_code: "DE".to_string(),
            region_code: None,
            postal_code_pattern: None,
            zone_type: "country".to_string(),
            parent_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }];

        let rates = vec![TaxRate {
            id: Uuid::new_v4(),
            name: "German Standard VAT".to_string(),
            tax_zone_id: zones[0].id,
            tax_category_id: None,
            rate: dec!(0.19),
            rate_type: "percentage".to_string(),
            is_vat: true,
            vat_type: Some("standard".to_string()),
            b2b_exempt: false,
            reverse_charge: false,
            valid_from: chrono::NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            valid_until: None,
            priority: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }];

        TaxCalculator::new(rates, zones, vec![])
    }

    #[test]
    fn test_calculate_item_tax() {
        let calculator = create_test_calculator();

        let item = TaxableItem {
            id: Uuid::new_v4(),
            product_id: Uuid::new_v4(),
            quantity: 1,
            unit_price: dec!(100.00),
            total_price: dec!(100.00),
            tax_category_id: None,
            is_digital: false,
            title: "Test Product".to_string(),
            sku: None,
        };

        let context = TaxContext {
            customer: CustomerTaxInfo::default(),
            shipping_address: crate::tax::TaxAddress::new("DE"),
            billing_address: crate::tax::TaxAddress::new("DE"),
            currency: crate::models::Currency::EUR,
            transaction_type: TransactionType::B2C,
        };

        let tax_zone = calculator
            .determine_tax_zone(&context.shipping_address)
            .unwrap();
        let result = calculator
            .calculate_item_tax(&item, &tax_zone, &context)
            .unwrap();

        assert_eq!(result.taxable_amount, dec!(100.00));
        assert_eq!(result.tax_amount, dec!(19.00));
        assert_eq!(result.tax_rate, dec!(0.19));
    }

    #[test]
    fn test_check_oss_applicability() {
        // Intra-EU, different countries, over threshold
        assert!(check_oss_applicability("DE", "FR", dec!(15000)));

        // Same country
        assert!(!check_oss_applicability("DE", "DE", dec!(15000)));

        // Under threshold
        assert!(!check_oss_applicability("DE", "FR", dec!(5000)));

        // Non-EU
        assert!(!check_oss_applicability("US", "DE", dec!(15000)));
    }

    #[test]
    fn test_check_economic_nexus() {
        // California - $500k threshold
        assert!(check_economic_nexus("CA", dec!(600000), 0));
        assert!(!check_economic_nexus("CA", dec!(400000), 0));

        // New York - $500k AND 100 transactions
        assert!(check_economic_nexus("NY", dec!(600000), 50));
        assert!(check_economic_nexus("NY", dec!(400000), 150));
        assert!(!check_economic_nexus("NY", dec!(400000), 50));
    }
}
