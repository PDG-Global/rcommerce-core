//! Tax Models
//!
//! Database models and types for the tax system.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Tax zone type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaxZoneType {
    Country,
    State,
    City,
    PostalCode,
    Custom,
}

impl std::fmt::Display for TaxZoneType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaxZoneType::Country => write!(f, "country"),
            TaxZoneType::State => write!(f, "state"),
            TaxZoneType::City => write!(f, "city"),
            TaxZoneType::PostalCode => write!(f, "postal_code"),
            TaxZoneType::Custom => write!(f, "custom"),
        }
    }
}

impl std::str::FromStr for TaxZoneType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "country" => Ok(TaxZoneType::Country),
            "state" => Ok(TaxZoneType::State),
            "city" => Ok(TaxZoneType::City),
            "postal_code" => Ok(TaxZoneType::PostalCode),
            "custom" => Ok(TaxZoneType::Custom),
            _ => Err(format!("Unknown tax zone type: {}", s)),
        }
    }
}

/// VAT type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VatType {
    Standard,
    Reduced,
    SuperReduced,
    Zero,
    Exempt,
}

impl std::fmt::Display for VatType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VatType::Standard => write!(f, "standard"),
            VatType::Reduced => write!(f, "reduced"),
            VatType::SuperReduced => write!(f, "super_reduced"),
            VatType::Zero => write!(f, "zero"),
            VatType::Exempt => write!(f, "exempt"),
        }
    }
}

impl std::str::FromStr for VatType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "standard" => Ok(VatType::Standard),
            "reduced" => Ok(VatType::Reduced),
            "super_reduced" | "super-reduced" | "superreduced" => Ok(VatType::SuperReduced),
            "zero" => Ok(VatType::Zero),
            "exempt" => Ok(VatType::Exempt),
            _ => Err(format!("Unknown VAT type: {}", s)),
        }
    }
}

/// Tax rate type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaxRateType {
    Percentage,
    Fixed,
}

impl std::fmt::Display for TaxRateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaxRateType::Percentage => write!(f, "percentage"),
            TaxRateType::Fixed => write!(f, "fixed"),
        }
    }
}

/// Tax exemption type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExemptionType {
    Resale,
    Nonprofit,
    Government,
    Diplomatic,
    Educational,
    Medical,
    Other,
}

impl std::fmt::Display for ExemptionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExemptionType::Resale => write!(f, "resale"),
            ExemptionType::Nonprofit => write!(f, "nonprofit"),
            ExemptionType::Government => write!(f, "government"),
            ExemptionType::Diplomatic => write!(f, "diplomatic"),
            ExemptionType::Educational => write!(f, "educational"),
            ExemptionType::Medical => write!(f, "medical"),
            ExemptionType::Other => write!(f, "other"),
        }
    }
}

/// Create tax zone request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaxZoneRequest {
    pub name: String,
    pub code: String,
    pub country_code: String,
    pub region_code: Option<String>,
    pub postal_code_pattern: Option<String>,
    pub zone_type: TaxZoneType,
    pub parent_id: Option<Uuid>,
}

/// Update tax zone request
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateTaxZoneRequest {
    pub name: Option<String>,
    pub region_code: Option<String>,
    pub postal_code_pattern: Option<String>,
    pub parent_id: Option<Uuid>,
}

/// Create tax category request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaxCategoryRequest {
    pub name: String,
    pub code: String,
    pub description: Option<String>,
    pub is_digital: bool,
    pub is_food: bool,
    pub is_luxury: bool,
    pub is_medical: bool,
    pub is_educational: bool,
}

/// Create tax rate request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaxRateRequest {
    pub name: String,
    pub tax_zone_id: Uuid,
    pub tax_category_id: Option<Uuid>,
    pub rate: Decimal,
    pub rate_type: TaxRateType,
    pub is_vat: bool,
    pub vat_type: Option<VatType>,
    pub b2b_exempt: bool,
    pub reverse_charge: bool,
    pub valid_from: NaiveDate,
    pub valid_until: Option<NaiveDate>,
    pub priority: i32,
}

/// Update tax rate request
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateTaxRateRequest {
    pub name: Option<String>,
    pub rate: Option<Decimal>,
    pub b2b_exempt: Option<bool>,
    pub reverse_charge: Option<bool>,
    pub valid_until: Option<NaiveDate>,
    pub priority: Option<i32>,
}

/// Tax rate with zone and category info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxRateWithDetails {
    pub id: Uuid,
    pub name: String,
    pub rate: Decimal,
    pub rate_type: String,
    pub is_vat: bool,
    pub vat_type: Option<String>,
    pub b2b_exempt: bool,
    pub reverse_charge: bool,
    pub tax_zone: TaxZoneInfo,
    pub tax_category: Option<TaxCategoryInfo>,
    pub valid_from: NaiveDate,
    pub valid_until: Option<NaiveDate>,
}

/// Simplified tax zone info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxZoneInfo {
    pub id: Uuid,
    pub name: String,
    pub code: String,
    pub country_code: String,
}

/// Simplified tax category info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxCategoryInfo {
    pub id: Uuid,
    pub name: String,
    pub code: String,
}

/// Tax calculation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalculateTaxRequest {
    pub items: Vec<TaxableItemRequest>,
    pub shipping_address: TaxAddressRequest,
    pub billing_address: Option<TaxAddressRequest>,
    pub customer_id: Option<Uuid>,
    pub vat_id: Option<String>,
    pub currency: String,
}

/// Taxable item in calculation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxableItemRequest {
    pub id: Uuid,
    pub product_id: Uuid,
    pub quantity: i32,
    pub unit_price: Decimal,
    pub tax_category_id: Option<Uuid>,
    pub is_digital: bool,
    pub title: String,
    pub sku: Option<String>,
}

/// Tax address request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxAddressRequest {
    pub country_code: String,
    pub region_code: Option<String>,
    pub postal_code: Option<String>,
    pub city: Option<String>,
}

/// Tax calculation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxCalculationResponse {
    pub line_items: Vec<LineItemTaxResponse>,
    pub shipping_tax: Decimal,
    pub total_tax: Decimal,
    pub tax_breakdown: Vec<TaxBreakdownResponse>,
    pub currency: String,
}

/// Line item tax response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineItemTaxResponse {
    pub item_id: Uuid,
    pub taxable_amount: Decimal,
    pub tax_amount: Decimal,
    pub tax_rate: Decimal,
    pub tax_rate_id: Uuid,
    pub tax_zone_id: Uuid,
}

/// Tax breakdown response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxBreakdownResponse {
    pub tax_zone_id: Uuid,
    pub tax_zone_name: String,
    pub tax_rate_id: Uuid,
    pub tax_rate_name: String,
    pub rate: Decimal,
    pub taxable_amount: Decimal,
    pub tax_amount: Decimal,
}

/// VAT ID validation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateVatIdRequest {
    pub vat_id: String,
}

/// VAT ID validation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VatValidationResponse {
    pub vat_id: String,
    pub country_code: String,
    pub is_valid: bool,
    pub business_name: Option<String>,
    pub business_address: Option<String>,
    pub validated_at: DateTime<Utc>,
    pub error_message: Option<String>,
}

/// OSS report request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateOssReportRequest {
    pub scheme: String, // 'union', 'non_union', 'import'
    pub period: String, // "YYYY-MM"
    pub member_state: String,
}

/// Create tax exemption request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaxExemptionRequest {
    pub customer_id: Uuid,
    pub tax_zone_id: Option<Uuid>,
    pub exemption_type: String,
    pub exemption_number: Option<String>,
    pub document_url: Option<String>,
    pub valid_from: NaiveDate,
    pub valid_until: Option<NaiveDate>,
}

/// Tax rate query parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxRateQuery {
    pub country_code: Option<String>,
    pub region_code: Option<String>,
    pub postal_code: Option<String>,
    pub tax_category_id: Option<Uuid>,
    pub valid_on: Option<NaiveDate>,
}

/// Tax zone query parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxZoneQuery {
    pub country_code: Option<String>,
    pub zone_type: Option<String>,
    pub search: Option<String>,
}

/// Economic nexus threshold info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicNexusThreshold {
    pub state_code: String,
    pub state_name: String,
    pub threshold_amount: Decimal,
    pub threshold_type: String, // 'revenue', 'transactions', 'both'
    pub transaction_threshold: Option<i32>,
    pub measurement_period: String,
    pub marketplace_included: bool,
}

/// US State sales tax info (2026)
pub fn get_us_economic_nexus_thresholds() -> Vec<EconomicNexusThreshold> {
    vec![
        EconomicNexusThreshold {
            state_code: "AL".to_string(),
            state_name: "Alabama".to_string(),
            threshold_amount: Decimal::from(250000),
            threshold_type: "revenue".to_string(),
            transaction_threshold: None,
            measurement_period: "previous_calendar_year".to_string(),
            marketplace_included: false,
        },
        EconomicNexusThreshold {
            state_code: "AK".to_string(),
            state_name: "Alaska".to_string(),
            threshold_amount: Decimal::from(100000),
            threshold_type: "revenue".to_string(),
            transaction_threshold: None,
            measurement_period: "current_or_previous_year".to_string(),
            marketplace_included: true,
        },
        EconomicNexusThreshold {
            state_code: "AZ".to_string(),
            state_name: "Arizona".to_string(),
            threshold_amount: Decimal::from(100000),
            threshold_type: "revenue".to_string(),
            transaction_threshold: None,
            measurement_period: "current_or_previous_year".to_string(),
            marketplace_included: false,
        },
        EconomicNexusThreshold {
            state_code: "AR".to_string(),
            state_name: "Arkansas".to_string(),
            threshold_amount: Decimal::from(100000),
            threshold_type: "both".to_string(),
            transaction_threshold: Some(200),
            measurement_period: "current_or_previous_year".to_string(),
            marketplace_included: false,
        },
        EconomicNexusThreshold {
            state_code: "CA".to_string(),
            state_name: "California".to_string(),
            threshold_amount: Decimal::from(500000),
            threshold_type: "revenue".to_string(),
            transaction_threshold: None,
            measurement_period: "current_or_previous_year".to_string(),
            marketplace_included: true,
        },
        EconomicNexusThreshold {
            state_code: "CO".to_string(),
            state_name: "Colorado".to_string(),
            threshold_amount: Decimal::from(100000),
            threshold_type: "revenue".to_string(),
            transaction_threshold: None,
            measurement_period: "current_or_previous_year".to_string(),
            marketplace_included: false,
        },
        EconomicNexusThreshold {
            state_code: "CT".to_string(),
            state_name: "Connecticut".to_string(),
            threshold_amount: Decimal::from(100000),
            threshold_type: "both".to_string(),
            transaction_threshold: Some(200),
            measurement_period: "12_month_period".to_string(),
            marketplace_included: true,
        },
        EconomicNexusThreshold {
            state_code: "FL".to_string(),
            state_name: "Florida".to_string(),
            threshold_amount: Decimal::from(100000),
            threshold_type: "revenue".to_string(),
            transaction_threshold: None,
            measurement_period: "previous_calendar_year".to_string(),
            marketplace_included: false,
        },
        EconomicNexusThreshold {
            state_code: "GA".to_string(),
            state_name: "Georgia".to_string(),
            threshold_amount: Decimal::from(100000),
            threshold_type: "both".to_string(),
            transaction_threshold: Some(200),
            measurement_period: "current_or_previous_year".to_string(),
            marketplace_included: false,
        },
        EconomicNexusThreshold {
            state_code: "HI".to_string(),
            state_name: "Hawaii".to_string(),
            threshold_amount: Decimal::from(100000),
            threshold_type: "both".to_string(),
            transaction_threshold: Some(200),
            measurement_period: "current_or_previous_year".to_string(),
            marketplace_included: true,
        },
        EconomicNexusThreshold {
            state_code: "ID".to_string(),
            state_name: "Idaho".to_string(),
            threshold_amount: Decimal::from(100000),
            threshold_type: "revenue".to_string(),
            transaction_threshold: None,
            measurement_period: "current_or_previous_year".to_string(),
            marketplace_included: true,
        },
        EconomicNexusThreshold {
            state_code: "IL".to_string(),
            state_name: "Illinois".to_string(),
            threshold_amount: Decimal::from(100000),
            threshold_type: "revenue".to_string(),
            transaction_threshold: None,
            measurement_period: "12_month_period".to_string(),
            marketplace_included: false,
        },
        EconomicNexusThreshold {
            state_code: "IN".to_string(),
            state_name: "Indiana".to_string(),
            threshold_amount: Decimal::from(100000),
            threshold_type: "revenue".to_string(),
            transaction_threshold: None,
            measurement_period: "current_or_previous_year".to_string(),
            marketplace_included: false,
        },
        EconomicNexusThreshold {
            state_code: "NY".to_string(),
            state_name: "New York".to_string(),
            threshold_amount: Decimal::from(500000),
            threshold_type: "both".to_string(),
            transaction_threshold: Some(100),
            measurement_period: "previous_four_quarters".to_string(),
            marketplace_included: true,
        },
        EconomicNexusThreshold {
            state_code: "TX".to_string(),
            state_name: "Texas".to_string(),
            threshold_amount: Decimal::from(500000),
            threshold_type: "revenue".to_string(),
            transaction_threshold: None,
            measurement_period: "12_month_period".to_string(),
            marketplace_included: true,
        },
    ]
}
