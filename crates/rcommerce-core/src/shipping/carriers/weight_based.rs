//! Weight-based shipping provider with per-zone rates

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::common::Address;
use crate::shipping::{
    AddressValidation, CustomsInfo, Package, RateOptions, ServiceFeature, Shipment,
    ShippingProvider, ShippingRate, ShippingService, TrackingInfo,
};
use crate::Error;
use crate::Result;

/// Weight-based shipping provider with per-zone rates
pub struct WeightBasedProvider {
    zones: Vec<ShippingZone>,
    /// Carrier name shown on rates and order records (defaults to the
    /// store's branding name via the factory)
    carrier_name: String,
}

/// A shipping zone: a set of 2-letter ISO country codes quoted at the same
/// rate. Zones are checked in order, first match wins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShippingZone {
    pub name: String,
    pub countries: Vec<String>,
    pub rate_per_half_kg: Decimal,
    pub base_rate: Decimal,
    pub delivery_days: i32,
    pub free_shipping: bool,
}

impl Default for WeightBasedProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl WeightBasedProvider {
    pub fn new() -> Self {
        Self {
            carrier_name: "Weight Based Shipping".to_string(),
            // Zones are checked in order (WooCommerce first-match-wins); the
            // catch-all fallback lives in `calculate_shipping_cost`.
            zones: default_zones(),
        }
    }

    /// Replace the built-in zones entirely (config-driven zones).
    pub fn with_zones(mut self, zones: Vec<ShippingZone>) -> Self {
        self.zones = zones;
        self
    }

    /// Override the carrier name shown on rates (store branding)
    pub fn with_carrier_name(mut self, name: &str) -> Self {
        if !name.trim().is_empty() {
            self.carrier_name = name.trim().to_string();
        }
        self
    }

    /// Sorted list of all shippable 2-letter country codes across all zones.
    pub fn shippable_countries(&self) -> Vec<String> {
        let mut codes: Vec<String> = self.zones.iter().flat_map(|z| z.countries.clone()).collect();
        codes.sort_unstable();
        codes.dedup();
        codes
    }

    fn find_zone(&self, country_code: &str) -> Option<&ShippingZone> {
        let cc = country_code.to_uppercase();
        self.zones.iter().find(|z| z.countries.contains(&cc))
    }

    /// Quote for a destination, or `None` when the country is not shippable
    /// (no zone covers it). Weight rounds up to half-kg units.
    fn calculate_shipping_cost(
        &self,
        weight_kg: Decimal,
        country_code: &str,
    ) -> Option<(Decimal, &str, i32)> {
        // Round up to nearest 0.5kg
        let half_kg_units = (weight_kg * dec!(2)).ceil().max(dec!(1));

        let zone = self.find_zone(country_code)?;
        if zone.free_shipping {
            return Some((Decimal::ZERO, zone.name.as_str(), zone.delivery_days));
        }
        let cost = zone.base_rate + (zone.rate_per_half_kg * half_kg_units);
        Some((cost, zone.name.as_str(), zone.delivery_days))
    }
}

#[async_trait]
impl ShippingProvider for WeightBasedProvider {
    fn id(&self) -> &'static str {
        "weight_based"
    }

    fn name(&self) -> &'static str {
        "Weight Based Shipping"
    }

    fn is_available(&self) -> bool {
        true
    }

    async fn get_rates(
        &self,
        _from_address: &Address,
        to_address: &Address,
        package: &Package,
        _options: &RateOptions,
    ) -> Result<Vec<ShippingRate>> {
        let country_code = to_address.country.to_uppercase();
        let weight_kg = if package.weight_unit.to_lowercase() == "kg" {
            package.weight
        } else if package.weight_unit.to_lowercase() == "lb" {
            package.weight * dec!(0.453592)
        } else {
            package.weight // Assume kg
        };

        // Countries outside the allowlist are not shippable: return no rates
        // instead of a made-up quote.
        let Some((cost, zone_name, delivery_days)) =
            self.calculate_shipping_cost(weight_kg, &country_code)
        else {
            return Ok(vec![]);
        };

        let service_name = if cost == Decimal::ZERO {
            format!("Free Shipping ({})", zone_name)
        } else {
            format!("Standard Shipping ({})", zone_name)
        };

        let rate = ShippingRate::new(
            "weight_based",
            &self.carrier_name,
            "weight_standard",
            &service_name,
            cost,
            "USD",
        );

        Ok(vec![ShippingRate {
            delivery_days: Some(delivery_days),
            total_cost: cost,
            ..rate
        }])
    }

    async fn create_shipment(
        &self,
        _from_address: &Address,
        _to_address: &Address,
        _package: &Package,
        _service_code: &str,
        _customs_info: Option<&CustomsInfo>,
    ) -> Result<Shipment> {
        Err(Error::not_found(
            "Shipment creation not supported for weight-based provider",
        ))
    }

    async fn track_shipment(&self, _tracking_number: &str) -> Result<TrackingInfo> {
        Err(Error::not_found(
            "Tracking not available for weight-based provider",
        ))
    }

    async fn cancel_shipment(&self, _shipment_id: &str) -> Result<bool> {
        Err(Error::not_found(
            "Shipment cancellation not supported for weight-based provider",
        ))
    }

    async fn estimate_delivery(
        &self,
        _from_address: &Address,
        to_address: &Address,
        _service_code: &str,
    ) -> Result<Option<DateTime<Utc>>> {
        let country_code = to_address.country.to_uppercase();
        let Some(zone) = self.find_zone(&country_code) else {
            return Ok(None);
        };
        let delivery_date = Utc::now() + chrono::Duration::days(zone.delivery_days as i64);
        Ok(Some(delivery_date))
    }

    async fn validate_address(&self, _address: &Address) -> Result<AddressValidation> {
        Ok(AddressValidation {
            is_valid: true,
            normalized_address: None,
            messages: vec![],
            residential: None,
        })
    }

    fn get_services(&self) -> Vec<ShippingService> {
        vec![]
    }
}

/// Calculate EU VAT on underdeclared product values
/// Wheels: $10 per item, Bushings: $2 per item, VAT rate: 20%
pub fn calculate_eu_vat(items: &[(String, i32)], // (category_slug, quantity)
) -> Decimal {
    let mut total_declared_value = Decimal::ZERO;

    for (category, quantity) in items {
        let value_per_item = match category.as_str() {
            "wheels" => dec!(10.00),
            "bushings" => dec!(2.00),
            _ => dec!(5.00), // Default for accessories etc
        };
        total_declared_value += value_per_item * Decimal::from(*quantity);
    }

    // 20% VAT
    total_declared_value * dec!(0.20)
}

fn zone(
    name: &str,
    countries: &[&str],
    rate_per_half_kg: Decimal,
    base_rate: Decimal,
    delivery_days: i32,
    free_shipping: bool,
) -> ShippingZone {
    ShippingZone {
        name: name.to_string(),
        countries: countries.iter().map(|c| c.to_string()).collect(),
        rate_per_half_kg,
        base_rate,
        delivery_days,
        free_shipping,
    }
}

/// Built-in default zones (88wheels' WooCommerce rate card). Used when the
/// configuration does not define `[shipping.weight_based] zones`.
fn default_zones() -> Vec<ShippingZone> {
    vec![
        // United States: base $5 + $5/0.5kg
        zone(
            "Standard Shipping",
            &["US"],
            dec!(5.00),
            dec!(5.00),
            7,
            false,
        ),
        // Switzerland: $7.5/0.5kg
        zone(
            "Standard Shipping",
            &["CH"],
            dec!(7.50),
            dec!(0.00),
            10,
            false,
        ),
        // Canada: $5/0.5kg
        zone("Standard Shipping", &["CA"], dec!(5.00), dec!(0.00), 10, false),
        // Russia: $9/0.5kg
        zone("Standard Shipping", &["RU"], dec!(9.00), dec!(0.00), 21, false),
        // South Africa: $16/0.5kg
        zone(
            "Standard Shipping",
            &["ZA"],
            dec!(16.00),
            dec!(0.00),
            21,
            false,
        ),
        // Latin America / selected: $12.5/0.5kg (MD moved to the
        // non-EU Europe list below)
        zone(
            "Standard Shipping",
            &[
                "AR", "BR", "CL", "CO", "CR", "DO", "EC", "SV", "HN", "JM", "LR", "NI", "PA",
                "UY",
            ],
            dec!(12.50),
            dec!(0.00),
            21,
            false,
        ),
        // EU 27: $4.5/0.5kg
        zone(
            "Standard Shipping",
            &[
                "AT", "BE", "BG", "HR", "CY", "CZ", "DK", "EE", "FI", "FR", "DE", "GR", "HU",
                "IE", "IT", "LV", "LT", "LU", "MT", "NL", "PL", "PT", "RO", "SK", "SI", "ES",
                "SE",
            ],
            dec!(4.50),
            dec!(0.00),
            10,
            false,
        ),
        // Other allowed countries (non-EU Europe + Asia-Pacific):
        // $3.5/0.5kg. Anything NOT in any zone gets no rates at all.
        zone(
            "Standard Shipping",
            &[
                // Non-EU Europe
                "GB", "NO", "IS", "LI", "MC", "AD", "SM", "AL", "RS", "ME", "MK", "BA", "MD",
                "XK", "GI", "GG", "JE", "IM", "FO", // Asia-Pacific / Asia
                "JP", "KR", "AU", "NZ", "MY", "TH", "VN", "ID", "PH", "IN", "BD", "LK", "NP",
                "KH", "LA", "BN", "PG", "FJ", "MN", "AE",
            ],
            dec!(3.50),
            dec!(0.00),
            21,
            false,
        ),
        // China: free via SF Express
        zone("SF Express", &["CN"], dec!(0.00), dec!(0.00), 10, true),
        // Hong Kong / Taiwan / Singapore: $2.5/0.5kg via SF Express
        zone("SF Express", &["HK", "TW", "SG"], dec!(2.50), dec!(0.00), 7, false),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cost(weight: &str, country: &str) -> Option<Decimal> {
        let provider = WeightBasedProvider::new();
        provider
            .calculate_shipping_cost(weight.parse().unwrap(), country)
            .map(|(cost, _, _)| cost)
    }

    #[test]
    fn test_singapore_sf_express_rate() {
        assert_eq!(cost("1.0", "SG"), Some(dec!(5.00))); // 2 half-kg units x $2.5
        assert_eq!(cost("0.5", "sg"), Some(dec!(2.50)));
    }

    #[test]
    fn test_hk_tw_sf_express_rate() {
        assert_eq!(cost("1.0", "HK"), Some(dec!(5.00)));
        assert_eq!(cost("0.5", "TW"), Some(dec!(2.50)));
    }

    #[test]
    fn test_macau_not_shippable() {
        // MO was dropped from the $2.5 zone and is not in the allowlist
        assert_eq!(cost("1.0", "MO"), None);
    }

    #[test]
    fn test_us_base_plus_rate() {
        assert_eq!(cost("0.5", "US"), Some(dec!(10.00))); // $5 + $5 x 1
        assert_eq!(cost("1.0", "US"), Some(dec!(15.00))); // $5 + $5 x 2
    }

    #[test]
    fn test_switzerland_canada_russia_south_africa() {
        assert_eq!(cost("1.0", "CH"), Some(dec!(15.00))); // 2 x $7.5
        assert_eq!(cost("1.0", "CA"), Some(dec!(10.00))); // 2 x $5
        assert_eq!(cost("0.5", "RU"), Some(dec!(9.00)));
        assert_eq!(cost("0.5", "ZA"), Some(dec!(16.00)));
    }

    #[test]
    fn test_israel_not_shippable() {
        assert_eq!(cost("1.0", "IL"), None);
    }

    #[test]
    fn test_disallowed_countries_get_no_rates() {
        for cc in [
            "UA", "BY", "KZ", "UZ", "TM", "KG", "TJ", "AF", "PK", "AM", "AZ", "GE", "TR", "SA",
            "QA", "BH", "KW", "OM", "JO", "LB", "IQ", "IR", "SY", "YE", "EG", "MX", "MO",
        ] {
            assert_eq!(cost("1.0", cc), None, "{cc} must not be shippable");
        }
    }

    #[test]
    fn test_explicit_allowed_list_at_fallback_rate() {
        for cc in [
            "GB", "NO", "IS", "LI", "MC", "AD", "SM", "AL", "RS", "ME", "MK", "BA", "MD", "XK",
            "GI", "GG", "JE", "IM", "FO", "JP", "KR", "AU", "NZ", "MY", "TH", "VN", "ID", "PH",
            "IN", "BD", "LK", "NP", "KH", "LA", "BN", "PG", "FJ", "MN", "AE",
        ] {
            assert_eq!(cost("1.0", cc), Some(dec!(7.00)), "{cc} should be 2 x $3.5");
        }
    }

    #[test]
    fn test_shippable_countries_list() {
        let provider = WeightBasedProvider::new();
        let codes = provider.shippable_countries();
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(codes, sorted, "codes must be sorted and unique");
        for cc in [
            "AE", "US", "RU", "ZA", "GB", "JP", "CN", "HK", "SG", "DE", "BR",
        ] {
            assert!(codes.contains(&cc.to_string()), "{cc} must be shippable");
        }
        for cc in ["IL", "UA", "KZ", "TR", "MO"] {
            assert!(
                !codes.contains(&cc.to_string()),
                "{cc} must not be shippable"
            );
        }
    }

    #[test]
    fn test_eu_zone_rate() {
        assert_eq!(cost("1.0", "DE"), Some(dec!(9.00))); // 2 x $4.5
        assert_eq!(cost("0.5", "FR"), Some(dec!(4.50)));
    }

    #[test]
    fn test_china_free() {
        assert_eq!(cost("2.0", "CN"), Some(dec!(0.00)));
    }

    #[test]
    fn test_latam_zone() {
        assert_eq!(cost("1.0", "BR"), Some(dec!(25.00))); // 2 x $12.5
        assert_eq!(cost("0.5", "UY"), Some(dec!(12.50)));
    }

    #[test]
    fn test_get_rates_returns_empty_for_disallowed_country() {        let provider = WeightBasedProvider::new();
        let mut addr = test_address("IL");
        let package = Package {
            weight: dec!(1.0),
            weight_unit: "kg".to_string(),
            length: None,
            width: None,
            height: None,
            dimension_unit: None,
            predefined_package: None,
        };
        let options = RateOptions {
            carriers: None,
            services: None,
            include_insurance: false,
            insurance_value: None,
            signature_confirmation: false,
            adult_signature: false,
            saturday_delivery: false,
            hold_for_pickup: false,
            currency: Some("USD".to_string()),
        };
        let rates =
            futures::executor::block_on(provider.get_rates(&addr, &addr, &package, &options))
                .unwrap();
        assert!(rates.is_empty(), "IL must produce no rates");

        addr.country = "GB".to_string();
        let rates =
            futures::executor::block_on(provider.get_rates(&addr, &addr, &package, &options))
                .unwrap();
        assert_eq!(rates.len(), 1);
        assert_eq!(rates[0].total_cost, dec!(7.00));
        assert_eq!(
            rates[0].service_name,
            "Standard Shipping (Standard Shipping)"
        );
    }

    fn au_nz_zones() -> Vec<ShippingZone> {
        vec![
            ShippingZone {
                name: "Standard Shipping".to_string(),
                countries: vec!["AU".to_string()],
                rate_per_half_kg: dec!(0.00),
                base_rate: dec!(0.00),
                delivery_days: 5,
                free_shipping: true,
            },
            ShippingZone {
                name: "Standard Shipping".to_string(),
                countries: vec!["NZ".to_string()],
                rate_per_half_kg: dec!(0.00),
                base_rate: dec!(0.00),
                delivery_days: 7,
                free_shipping: true,
            },
        ]
    }

    #[test]
    fn test_custom_zones_replace_defaults() {
        // Config-driven zones must REPLACE the built-in zones entirely:
        // AU/NZ-only config must not quote the default-zone countries.
        let provider = WeightBasedProvider::new().with_zones(au_nz_zones());

        // AU/NZ get free rates
        assert_eq!(
            provider
                .calculate_shipping_cost(dec!(2.0), "AU")
                .map(|(cost, _, _)| cost),
            Some(dec!(0.00))
        );
        assert_eq!(
            provider
                .calculate_shipping_cost(dec!(2.0), "nz") // case-insensitive
                .map(|(cost, _, days)| (cost, days)),
            Some((dec!(0.00), 7))
        );

        // Countries from the default zones are no longer shippable
        for cc in ["US", "CN", "GB", "DE", "SG", "JP"] {
            assert_eq!(
                provider.calculate_shipping_cost(dec!(1.0), cc),
                None,
                "{cc} must not be shippable with AU/NZ-only zones"
            );
        }

        assert_eq!(
            provider.shippable_countries(),
            vec!["AU".to_string(), "NZ".to_string()]
        );
    }

    #[test]
    fn test_custom_zones_get_rates_via_provider_trait() {
        let provider = WeightBasedProvider::new().with_zones(au_nz_zones());
        let addr = test_address("AU");
        let package = Package {
            weight: dec!(1.5),
            weight_unit: "kg".to_string(),
            length: None,
            width: None,
            height: None,
            dimension_unit: None,
            predefined_package: None,
        };
        let options = RateOptions {
            carriers: None,
            services: None,
            include_insurance: false,
            insurance_value: None,
            signature_confirmation: false,
            adult_signature: false,
            saturday_delivery: false,
            hold_for_pickup: false,
            currency: Some("USD".to_string()),
        };

        let rates =
            futures::executor::block_on(provider.get_rates(&addr, &addr, &package, &options))
                .unwrap();
        assert_eq!(rates.len(), 1);
        assert_eq!(rates[0].total_cost, dec!(0.00));
        assert_eq!(
            rates[0].service_name,
            "Free Shipping (Standard Shipping)"
        );
        assert_eq!(rates[0].delivery_days, Some(5));

        // US produces no rates at all
        let us_addr = test_address("US");
        let rates =
            futures::executor::block_on(provider.get_rates(&us_addr, &us_addr, &package, &options))
                .unwrap();
        assert!(rates.is_empty(), "US must produce no rates");
    }

    fn test_address(country: &str) -> Address {        Address {
            id: uuid::Uuid::new_v4(),
            customer_id: uuid::Uuid::nil(),
            first_name: "Test".to_string(),
            last_name: "Buyer".to_string(),
            company: None,
            phone: None,
            address1: "1 Test St".to_string(),
            address2: None,
            city: "Testville".to_string(),
            state: None,
            country: country.to_string(),
            zip: "90210".to_string(),
            is_default_shipping: false,
            is_default_billing: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}
