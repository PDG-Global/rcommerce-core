//! Shipping zones and zone-based rate calculation

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::common::Address;

/// Shipping zone definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShippingZone {
    pub id: String,
    pub name: String,
    pub countries: Vec<String>,
    pub states: Vec<String>,
    pub postal_codes: Vec<String>,
    pub postal_code_ranges: Vec<(String, String)>, // (start, end)
    pub rates: Vec<ZoneRate>,
}

impl ShippingZone {
    /// Create a new shipping zone
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            countries: Vec::new(),
            states: Vec::new(),
            postal_codes: Vec::new(),
            postal_code_ranges: Vec::new(),
            rates: Vec::new(),
        }
    }

    /// Add country to zone
    pub fn with_country(mut self, country: impl Into<String>) -> Self {
        self.countries.push(country.into());
        self
    }

    /// Add state/province to zone
    pub fn with_state(mut self, state: impl Into<String>) -> Self {
        self.states.push(state.into());
        self
    }

    /// Add postal code to zone
    pub fn with_postal_code(mut self, code: impl Into<String>) -> Self {
        self.postal_codes.push(code.into());
        self
    }

    /// Add postal code range to zone
    pub fn with_postal_range(mut self, start: impl Into<String>, end: impl Into<String>) -> Self {
        self.postal_code_ranges.push((start.into(), end.into()));
        self
    }

    /// Add rate to zone
    pub fn with_rate(mut self, rate: ZoneRate) -> Self {
        self.rates.push(rate);
        self
    }

    /// Check if address is in this zone
    pub fn contains(&self, address: &Address) -> bool {
        // Check country
        if !self.countries.is_empty()
            && !self
                .countries
                .iter()
                .any(|c| c.eq_ignore_ascii_case(&address.country))
        {
            return false;
        }

        // Check state
        if !self.states.is_empty() {
            let state = address.state.as_deref().unwrap_or("");
            if !self.states.iter().any(|s| s.eq_ignore_ascii_case(state)) {
                return false;
            }
        }

        // Check postal code
        if !self.postal_codes.is_empty() && !self.postal_codes.iter().any(|p| p == &address.zip) {
            // Check ranges
            let in_range = self
                .postal_code_ranges
                .iter()
                .any(|(start, end)| address.zip >= start.clone() && address.zip <= end.clone());
            if !in_range {
                return false;
            }
        }

        true
    }
}

/// Zone-based shipping rate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneRate {
    pub name: String,
    pub base_rate: Decimal,
    pub weight_rate: Decimal,
    pub free_shipping_threshold: Option<Decimal>,
    pub min_weight: Option<Decimal>,
    pub max_weight: Option<Decimal>,
    pub handling_fee: Decimal,
}

impl ZoneRate {
    /// Create a new zone rate
    pub fn new(name: impl Into<String>, base_rate: Decimal, weight_rate: Decimal) -> Self {
        Self {
            name: name.into(),
            base_rate,
            weight_rate,
            free_shipping_threshold: None,
            min_weight: None,
            max_weight: None,
            handling_fee: Decimal::ZERO,
        }
    }

    /// Set free shipping threshold
    pub fn with_free_shipping_threshold(mut self, threshold: Decimal) -> Self {
        self.free_shipping_threshold = Some(threshold);
        self
    }

    /// Set weight limits
    pub fn with_weight_range(mut self, min: Decimal, max: Decimal) -> Self {
        self.min_weight = Some(min);
        self.max_weight = Some(max);
        self
    }

    /// Set handling fee
    pub fn with_handling_fee(mut self, fee: Decimal) -> Self {
        self.handling_fee = fee;
        self
    }

    /// Calculate rate for weight and subtotal
    pub fn calculate(&self, weight: Decimal, subtotal: Decimal) -> Decimal {
        // Check free shipping
        if let Some(threshold) = self.free_shipping_threshold {
            if subtotal >= threshold {
                return Decimal::ZERO;
            }
        }

        self.base_rate + (weight * self.weight_rate) + self.handling_fee
    }

    /// Check if rate applies to weight
    pub fn applies_to_weight(&self, weight: Decimal) -> bool {
        if let Some(min) = self.min_weight {
            if weight < min {
                return false;
            }
        }
        if let Some(max) = self.max_weight {
            if weight > max {
                return false;
            }
        }
        true
    }
}

/// Zone calculator for finding applicable zones and rates
pub struct ZoneCalculator {
    zones: Vec<ShippingZone>,
}

impl ZoneCalculator {
    /// Create a new zone calculator
    pub fn new() -> Self {
        Self { zones: Vec::new() }
    }

    /// Add a zone
    pub fn add_zone(&mut self, zone: ShippingZone) {
        self.zones.push(zone);
    }

    /// Find zone for address
    pub fn find_zone(&self, address: &Address) -> Option<&ShippingZone> {
        self.zones.iter().find(|z| z.contains(address))
    }

    /// Calculate shipping for address
    pub fn calculate_shipping(
        &self,
        address: &Address,
        weight: Decimal,
        subtotal: Decimal,
    ) -> Option<(Decimal, &ZoneRate)> {
        let zone = self.find_zone(address)?;

        zone.rates
            .iter()
            .filter(|r| r.applies_to_weight(weight))
            .map(|r| (r.calculate(weight, subtotal), r))
            .min_by_key(|(cost, _)| *cost)
    }

    /// Get all rates for address
    pub fn get_rates(
        &self,
        address: &Address,
        weight: Decimal,
        subtotal: Decimal,
    ) -> Vec<(Decimal, &ZoneRate)> {
        let zone = match self.find_zone(address) {
            Some(z) => z,
            None => return Vec::new(),
        };

        zone.rates
            .iter()
            .filter(|r| r.applies_to_weight(weight))
            .map(|r| (r.calculate(weight, subtotal), r))
            .collect()
    }
}

impl Default for ZoneCalculator {
    fn default() -> Self {
        Self::new()
    }
}

/// Predefined zone configurations
pub struct ZonePresets;

impl ZonePresets {
    /// Create default US shipping zones
    pub fn us_zones() -> Vec<ShippingZone> {
        vec![
            // Domestic zone
            ShippingZone::new("domestic", "United States")
                .with_country("US")
                .with_rate(
                    ZoneRate::new("Standard Ground", Decimal::from(8), Decimal::from(1))
                        .with_free_shipping_threshold(Decimal::from(100)),
                )
                .with_rate(ZoneRate::new(
                    "Express",
                    Decimal::from(25),
                    Decimal::from(2),
                )),
            // Canada zone
            ShippingZone::new("canada", "Canada")
                .with_country("CA")
                .with_rate(ZoneRate::new(
                    "Standard International",
                    Decimal::from(20),
                    Decimal::from(3),
                )),
            // Europe zone
            ShippingZone::new("europe", "Europe")
                .with_country("GB")
                .with_country("DE")
                .with_country("FR")
                .with_country("IT")
                .with_country("ES")
                .with_country("NL")
                .with_country("BE")
                .with_rate(ZoneRate::new(
                    "International",
                    Decimal::from(35),
                    Decimal::from(5),
                )),
            // Rest of world
            ShippingZone::new("international", "Rest of World").with_rate(ZoneRate::new(
                "International",
                Decimal::from(50),
                Decimal::from(8),
            )),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_zone_contains() {
        let zone = ShippingZone::new("test", "Test Zone")
            .with_country("US")
            .with_state("CA");

        let address = Address {
            id: uuid::Uuid::new_v4(),
            customer_id: uuid::Uuid::new_v4(),
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            company: None,
            phone: None,
            address1: "123 Main St".to_string(),
            address2: None,
            city: "Los Angeles".to_string(),
            state: Some("CA".to_string()),
            zip: "90210".to_string(),
            country: "US".to_string(),
            is_default_shipping: true,
            is_default_billing: false,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        assert!(zone.contains(&address));
    }

    #[test]
    fn test_zone_rate_calculate() {
        let rate =
            ZoneRate::new("Standard", dec!(10), dec!(2)).with_free_shipping_threshold(dec!(100));

        // 2kg weight, $50 subtotal
        let cost = rate.calculate(dec!(2), dec!(50));
        assert_eq!(cost, dec!(14)); // 10 + (2 * 2) = 14

        // Free shipping over $100
        let cost = rate.calculate(dec!(2), dec!(150));
        assert_eq!(cost, dec!(0));
    }

    #[test]
    fn test_zone_calculator() {
        let mut calc = ZoneCalculator::new();
        calc.add_zone(
            ShippingZone::new("us", "United States")
                .with_country("US")
                .with_rate(ZoneRate::new("Ground", dec!(8), dec!(1))),
        );

        let address = Address {
            id: uuid::Uuid::new_v4(),
            customer_id: uuid::Uuid::new_v4(),
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            company: None,
            phone: None,
            address1: "123 Main St".to_string(),
            address2: None,
            city: "New York".to_string(),
            state: Some("NY".to_string()),
            zip: "10001".to_string(),
            country: "US".to_string(),
            is_default_shipping: true,
            is_default_billing: false,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let result = calc.calculate_shipping(&address, dec!(5), dec!(50));
        assert!(result.is_some());

        let (cost, rate) = result.unwrap();
        assert_eq!(cost, dec!(13)); // 8 + (5 * 1) = 13
        assert_eq!(rate.name, "Ground");
    }
}
