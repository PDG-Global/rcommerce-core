//! Tax System Module
//!
//! Comprehensive tax calculation and management for global e-commerce.
//! Supports VAT, sales tax, GST, and customs duties.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

pub mod calculator;
pub mod models;
pub mod providers;
pub mod service;
pub mod vat_validation;

pub use calculator::{LineItemTax, TaxBreakdown, TaxCalculation, TaxCalculator};
pub use models::*;
pub use service::{DefaultTaxService, TaxService};
pub use vat_validation::{VatId, VatValidationResult, ViesValidator};

use crate::Result;

/// Transaction type for tax determination
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TransactionType {
    /// Business to Consumer
    #[default]
    B2C,
    /// Business to Business
    B2B,
}

/// Tax calculation context
#[derive(Debug, Clone)]
pub struct TaxContext {
    /// Customer information
    pub customer: CustomerTaxInfo,
    /// Shipping destination address
    pub shipping_address: TaxAddress,
    /// Billing address (may differ from shipping)
    pub billing_address: TaxAddress,
    /// Transaction currency
    pub currency: crate::models::Currency,
    /// B2B or B2C transaction
    pub transaction_type: TransactionType,
}

/// Customer tax information
#[derive(Debug, Clone, Default)]
pub struct CustomerTaxInfo {
    /// Customer ID if registered
    pub customer_id: Option<Uuid>,
    /// Whether customer is tax exempt
    pub is_tax_exempt: bool,
    /// VAT/GST ID for B2B transactions
    pub vat_id: Option<VatId>,
    /// Tax exemption documents/certs
    pub exemptions: Vec<TaxExemption>,
}

/// Address for tax determination
#[derive(Debug, Clone, Default)]
pub struct TaxAddress {
    /// Country code (ISO 3166-1 alpha-2)
    pub country_code: String,
    /// Region/State/Province code
    pub region_code: Option<String>,
    /// Postal/ZIP code
    pub postal_code: Option<String>,
    /// City
    pub city: Option<String>,
}

impl TaxAddress {
    /// Create a new tax address
    pub fn new(country_code: impl Into<String>) -> Self {
        Self {
            country_code: country_code.into(),
            region_code: None,
            postal_code: None,
            city: None,
        }
    }

    /// Set region code
    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region_code = Some(region.into());
        self
    }

    /// Set postal code
    pub fn with_postal_code(mut self, postal: impl Into<String>) -> Self {
        self.postal_code = Some(postal.into());
        self
    }

    /// Set city
    pub fn with_city(mut self, city: impl Into<String>) -> Self {
        self.city = Some(city.into());
        self
    }
}

/// OSS (One Stop Shop) scheme types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OssScheme {
    /// Union OSS for EU businesses
    Union,
    /// Non-Union OSS for non-EU businesses
    NonUnion,
    /// Import OSS for low-value goods (≤ €150)
    Import,
}

impl OssScheme {
    /// Get the filing frequency
    pub fn filing_frequency(&self) -> &str {
        match self {
            OssScheme::Union | OssScheme::NonUnion => "quarterly",
            OssScheme::Import => "monthly",
        }
    }

    /// Get scheme name
    pub fn name(&self) -> &str {
        match self {
            OssScheme::Union => "Union OSS",
            OssScheme::NonUnion => "Non-Union OSS",
            OssScheme::Import => "Import OSS (IOSS)",
        }
    }
}

/// Tax provider trait for external integrations
#[async_trait::async_trait]
pub trait TaxProvider: Send + Sync {
    /// Provider name
    fn name(&self) -> &str;

    /// Calculate tax for items
    async fn calculate_tax(
        &self,
        items: &[TaxableItem],
        context: &TaxContext,
    ) -> Result<TaxCalculation>;

    /// Validate an address for tax purposes
    async fn validate_address(&self, address: &TaxAddress) -> Result<TaxAddress>;
}

/// Item that can be taxed
#[derive(Debug, Clone)]
pub struct TaxableItem {
    /// Item ID
    pub id: Uuid,
    /// Product ID
    pub product_id: Uuid,
    /// Quantity
    pub quantity: i32,
    /// Unit price
    pub unit_price: Decimal,
    /// Total price (quantity × unit_price)
    pub total_price: Decimal,
    /// Tax category ID
    pub tax_category_id: Option<Uuid>,
    /// Whether item is digital
    pub is_digital: bool,
    /// Product name
    pub title: String,
    /// SKU
    pub sku: Option<String>,
}

/// Tax rate database model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TaxRate {
    pub id: Uuid,
    pub name: String,
    pub tax_zone_id: Uuid,
    pub tax_category_id: Option<Uuid>,
    pub rate: Decimal,
    pub rate_type: String, // 'percentage', 'fixed'
    pub is_vat: bool,
    pub vat_type: Option<String>, // 'standard', 'reduced', 'super_reduced', 'zero'
    pub b2b_exempt: bool,
    pub reverse_charge: bool,
    pub valid_from: NaiveDate,
    pub valid_until: Option<NaiveDate>,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Tax zone database model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TaxZone {
    pub id: Uuid,
    pub name: String,
    pub code: String,
    pub country_code: String,
    pub region_code: Option<String>,
    pub postal_code_pattern: Option<String>,
    pub zone_type: String, // 'country', 'state', 'city', 'custom'
    pub parent_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Tax category database model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TaxCategory {
    pub id: Uuid,
    pub name: String,
    pub code: String,
    pub description: Option<String>,
    pub is_digital: bool,
    pub is_food: bool,
    pub is_luxury: bool,
    pub is_medical: bool,
    pub is_educational: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Tax exemption database model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TaxExemption {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub tax_zone_id: Option<Uuid>,
    pub exemption_type: String, // 'resale', 'nonprofit', 'government', 'diplomatic', 'other'
    pub exemption_number: Option<String>,
    pub document_url: Option<String>,
    pub valid_from: NaiveDate,
    pub valid_until: Option<NaiveDate>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

/// Tax transaction record for reporting
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct TaxTransaction {
    pub id: Uuid,
    pub order_id: Uuid,
    pub order_item_id: Option<Uuid>,
    pub tax_rate_id: Uuid,
    pub tax_zone_id: Uuid,
    pub tax_category_id: Option<Uuid>,
    pub taxable_amount: Decimal,
    pub tax_amount: Decimal,
    pub tax_rate: Decimal,
    pub country_code: String,
    pub region_code: Option<String>,
    pub oss_scheme: Option<String>,
    pub oss_period: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// VAT validation cache entry
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct VatValidationCache {
    pub id: Uuid,
    pub vat_id: String,
    pub country_code: String,
    pub business_name: Option<String>,
    pub business_address: Option<String>,
    pub is_valid: bool,
    pub validated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// OSS report data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OssReport {
    pub scheme: OssScheme,
    pub period: String, // "YYYY-MM"
    pub member_state: String,
    pub transactions: Vec<OssTransaction>,
    pub summary: OssSummary,
}

/// OSS transaction line
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OssTransaction {
    pub country_code: String,
    pub vat_rate: Decimal,
    pub taxable_amount: Decimal,
    pub vat_amount: Decimal,
    pub transaction_count: i32,
}

/// OSS summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OssSummary {
    pub total_taxable_amount: Decimal,
    pub total_vat_amount: Decimal,
    pub total_transactions: i32,
    pub by_country: Vec<CountrySummary>,
}

/// Country summary for OSS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountrySummary {
    pub country_code: String,
    pub country_name: String,
    pub vat_rate: Decimal,
    pub taxable_amount: Decimal,
    pub vat_amount: Decimal,
    pub transaction_count: i32,
}

/// Determine if a transaction qualifies for OSS
pub fn determine_oss_scheme(
    seller_country: &str,
    customer_country: &str,
    is_eu_business: bool,
    import_value: Option<Decimal>,
) -> Option<OssScheme> {
    // Same country - no OSS
    if seller_country == customer_country {
        return None;
    }

    let eu_countries = vec![
        "AT", "BE", "BG", "HR", "CY", "CZ", "DK", "EE", "FI", "FR", "DE", "GR", "HU", "IE", "IT",
        "LV", "LT", "LU", "MT", "NL", "PL", "PT", "RO", "SK", "SI", "ES", "SE",
    ];

    let seller_in_eu = eu_countries.contains(&seller_country);
    let customer_in_eu = eu_countries.contains(&customer_country);

    // Import OSS for low-value goods from outside EU
    if !seller_in_eu && customer_in_eu {
        if let Some(value) = import_value {
            if value <= Decimal::from(150) {
                return Some(OssScheme::Import);
            }
        }
        return None; // High value imports - customer pays at customs
    }

    // Intra-EU sales
    if seller_in_eu && customer_in_eu {
        if is_eu_business {
            return None; // B2B - reverse charge applies
        }
        return Some(OssScheme::Union);
    }

    // Non-EU business selling to EU
    if !seller_in_eu && customer_in_eu && !is_eu_business {
        return Some(OssScheme::NonUnion);
    }

    None
}

/// Get EU VAT rates for 2026
pub fn get_eu_vat_rate(country_code: &str, vat_type: &str) -> Option<Decimal> {
    let rates: std::collections::HashMap<&str, std::collections::HashMap<&str, &str>> = [
        (
            "AT",
            [
                ("standard", "0.20"),
                ("reduced", "0.13"),
                ("super_reduced", "0.10"),
            ]
            .into(),
        ),
        (
            "BE",
            [
                ("standard", "0.21"),
                ("reduced", "0.12"),
                ("super_reduced", "0.06"),
            ]
            .into(),
        ),
        ("BG", [("standard", "0.20"), ("reduced", "0.09")].into()),
        (
            "HR",
            [
                ("standard", "0.25"),
                ("reduced", "0.13"),
                ("super_reduced", "0.05"),
            ]
            .into(),
        ),
        (
            "CY",
            [
                ("standard", "0.19"),
                ("reduced", "0.09"),
                ("super_reduced", "0.05"),
            ]
            .into(),
        ),
        (
            "CZ",
            [
                ("standard", "0.21"),
                ("reduced", "0.15"),
                ("super_reduced", "0.10"),
            ]
            .into(),
        ),
        ("DK", [("standard", "0.25")].into()),
        (
            "EE",
            [
                ("standard", "0.22"),
                ("reduced", "0.09"),
                ("super_reduced", "0.05"),
            ]
            .into(),
        ),
        (
            "FI",
            [
                ("standard", "0.25"),
                ("reduced", "0.14"),
                ("super_reduced", "0.10"),
            ]
            .into(),
        ),
        (
            "FR",
            [
                ("standard", "0.20"),
                ("reduced", "0.10"),
                ("super_reduced", "0.055"),
            ]
            .into(),
        ),
        ("DE", [("standard", "0.19"), ("reduced", "0.07")].into()),
        (
            "GR",
            [
                ("standard", "0.24"),
                ("reduced", "0.13"),
                ("super_reduced", "0.06"),
            ]
            .into(),
        ),
        (
            "HU",
            [
                ("standard", "0.27"),
                ("reduced", "0.18"),
                ("super_reduced", "0.05"),
            ]
            .into(),
        ),
        (
            "IE",
            [
                ("standard", "0.23"),
                ("reduced", "0.13.5"),
                ("super_reduced", "0.09"),
            ]
            .into(),
        ),
        (
            "IT",
            [
                ("standard", "0.22"),
                ("reduced", "0.10"),
                ("super_reduced", "0.05"),
            ]
            .into(),
        ),
        (
            "LV",
            [
                ("standard", "0.21"),
                ("reduced", "0.12"),
                ("super_reduced", "0.05"),
            ]
            .into(),
        ),
        (
            "LT",
            [
                ("standard", "0.21"),
                ("reduced", "0.09"),
                ("super_reduced", "0.05"),
            ]
            .into(),
        ),
        (
            "LU",
            [
                ("standard", "0.17"),
                ("reduced", "0.14"),
                ("super_reduced", "0.08"),
            ]
            .into(),
        ),
        (
            "MT",
            [
                ("standard", "0.18"),
                ("reduced", "0.07"),
                ("super_reduced", "0.05"),
            ]
            .into(),
        ),
        ("NL", [("standard", "0.21"), ("reduced", "0.09")].into()),
        (
            "PL",
            [
                ("standard", "0.23"),
                ("reduced", "0.08"),
                ("super_reduced", "0.05"),
            ]
            .into(),
        ),
        (
            "PT",
            [
                ("standard", "0.23"),
                ("reduced", "0.13"),
                ("super_reduced", "0.06"),
            ]
            .into(),
        ),
        (
            "RO",
            [
                ("standard", "0.19"),
                ("reduced", "0.09"),
                ("super_reduced", "0.05"),
            ]
            .into(),
        ),
        ("SK", [("standard", "0.20"), ("reduced", "0.10")].into()),
        ("SI", [("standard", "0.22"), ("reduced", "0.09.5")].into()),
        (
            "ES",
            [
                ("standard", "0.21"),
                ("reduced", "0.10"),
                ("super_reduced", "0.04"),
            ]
            .into(),
        ),
        (
            "SE",
            [
                ("standard", "0.25"),
                ("reduced", "0.12"),
                ("super_reduced", "0.06"),
            ]
            .into(),
        ),
    ]
    .into();

    rates
        .get(country_code)?
        .get(vat_type)
        .and_then(|r| r.parse().ok())
}

/// Check if country is in the EU
pub fn is_eu_country(country_code: &str) -> bool {
    let eu_countries = [
        "AT", "BE", "BG", "HR", "CY", "CZ", "DK", "EE", "FI", "FR", "DE", "GR", "HU", "IE", "IT",
        "LV", "LT", "LU", "MT", "NL", "PL", "PT", "RO", "SK", "SI", "ES", "SE",
    ];
    eu_countries.contains(&country_code.to_uppercase().as_str())
}

/// Get US state sales tax rate (simplified - 2026 rates)
pub fn get_us_state_tax_rate(state_code: &str) -> Option<Decimal> {
    let rates: std::collections::HashMap<&str, &str> = [
        ("AL", "0.04"),
        ("AK", "0.00"),
        ("AZ", "0.056"),
        ("AR", "0.065"),
        ("CA", "0.0725"),
        ("CO", "0.029"),
        ("CT", "0.0635"),
        ("DE", "0.00"),
        ("FL", "0.06"),
        ("GA", "0.04"),
        ("HI", "0.04"),
        ("ID", "0.06"),
        ("IL", "0.0625"),
        ("IN", "0.07"),
        ("IA", "0.06"),
        ("KS", "0.065"),
        ("KY", "0.06"),
        ("LA", "0.0445"),
        ("ME", "0.055"),
        ("MD", "0.06"),
        ("MA", "0.0625"),
        ("MI", "0.06"),
        ("MN", "0.06875"),
        ("MS", "0.07"),
        ("MO", "0.04225"),
        ("MT", "0.00"),
        ("NE", "0.055"),
        ("NV", "0.0685"),
        ("NH", "0.00"),
        ("NJ", "0.06625"),
        ("NM", "0.05125"),
        ("NY", "0.04"),
        ("NC", "0.0475"),
        ("ND", "0.05"),
        ("OH", "0.0575"),
        ("OK", "0.045"),
        ("OR", "0.00"),
        ("PA", "0.06"),
        ("RI", "0.07"),
        ("SC", "0.06"),
        ("SD", "0.045"),
        ("TN", "0.07"),
        ("TX", "0.0625"),
        ("UT", "0.0485"),
        ("VT", "0.06"),
        ("VA", "0.043"),
        ("WA", "0.065"),
        ("WV", "0.06"),
        ("WI", "0.05"),
        ("WY", "0.04"),
        ("DC", "0.06"),
    ]
    .into();

    rates
        .get(state_code.to_uppercase().as_str())
        .and_then(|r| r.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tax_address_builder() {
        let addr = TaxAddress::new("DE")
            .with_region("BY")
            .with_postal_code("80331")
            .with_city("Munich");

        assert_eq!(addr.country_code, "DE");
        assert_eq!(addr.region_code, Some("BY".to_string()));
        assert_eq!(addr.postal_code, Some("80331".to_string()));
        assert_eq!(addr.city, Some("Munich".to_string()));
    }

    #[test]
    fn test_is_eu_country() {
        assert!(is_eu_country("DE"));
        assert!(is_eu_country("FR"));
        assert!(!is_eu_country("US"));
        assert!(!is_eu_country("GB"));
    }

    #[test]
    fn test_get_eu_vat_rate() {
        let de_standard = get_eu_vat_rate("DE", "standard");
        assert_eq!(de_standard, Some(Decimal::from_str_exact("0.19").unwrap()));

        let de_reduced = get_eu_vat_rate("DE", "reduced");
        assert_eq!(de_reduced, Some(Decimal::from_str_exact("0.07").unwrap()));

        let fr_standard = get_eu_vat_rate("FR", "standard");
        assert_eq!(fr_standard, Some(Decimal::from_str_exact("0.20").unwrap()));
    }

    #[test]
    fn test_get_us_state_tax_rate() {
        let ca_rate = get_us_state_tax_rate("CA");
        assert!(ca_rate.is_some());

        let or_rate = get_us_state_tax_rate("OR");
        assert_eq!(or_rate, Some(Decimal::ZERO));
    }

    #[test]
    fn test_determine_oss_scheme() {
        // Same country - no OSS
        let result = determine_oss_scheme("DE", "DE", false, None);
        assert_eq!(result, None);

        // Intra-EU B2C
        let result = determine_oss_scheme("DE", "FR", false, None);
        assert_eq!(result, Some(OssScheme::Union));

        // Intra-EU B2B
        let result = determine_oss_scheme("DE", "FR", true, None);
        assert_eq!(result, None);

        // Import OSS (low value)
        let result = determine_oss_scheme("US", "DE", false, Some(Decimal::from(100)));
        assert_eq!(result, Some(OssScheme::Import));

        // High value import
        let result = determine_oss_scheme("US", "DE", false, Some(Decimal::from(200)));
        assert_eq!(result, None);
    }
}
