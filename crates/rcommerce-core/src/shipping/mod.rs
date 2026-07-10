//! Shipping calculation and carrier integration module
//!
//! This module provides comprehensive shipping functionality including:
//! - Weight-based and volumetric weight calculations
//! - Real-time rate calculation from multiple carriers
//! - Shipping label generation
//! - Shipment tracking
//! - Multi-carrier support (DHL, FedEx, UPS, USPS)
//! - Third-party aggregator support (EasyPost, ShipStation)

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::common::Address;
use crate::{Error, Result};

pub mod calculation;
pub mod carriers;
pub mod packaging;
pub mod providers;
pub mod rules;
pub mod zones;

pub use calculation::{ShippingCalculator, VolumetricWeightCalculator, WeightConverter};
pub use packaging::{Package, PackageType, PackagingCalculator};
pub use rules::{RuleAction, RuleCondition, ShippingRule, ShippingRuleEngine};
pub use zones::{ShippingZone, ZoneCalculator, ZoneRate};

/// Core shipping provider trait
#[async_trait]
pub trait ShippingProvider: Send + Sync + 'static {
    /// Provider identifier (e.g., "ups", "fedex", "dhl", "easypost")
    fn id(&self) -> &'static str;

    /// Provider display name
    fn name(&self) -> &'static str;

    /// Check if provider is available (credentials configured)
    fn is_available(&self) -> bool;

    /// Get available shipping rates
    async fn get_rates(
        &self,
        from_address: &Address,
        to_address: &Address,
        package: &Package,
        options: &RateOptions,
    ) -> Result<Vec<ShippingRate>>;

    /// Create a shipment and generate label
    async fn create_shipment(
        &self,
        from_address: &Address,
        to_address: &Address,
        package: &Package,
        service_code: &str,
        customs_info: Option<&CustomsInfo>,
    ) -> Result<Shipment>;

    /// Track a shipment
    async fn track_shipment(&self, tracking_number: &str) -> Result<TrackingInfo>;

    /// Cancel a shipment (if possible)
    async fn cancel_shipment(&self, shipment_id: &str) -> Result<bool>;

    /// Validate shipping address
    async fn validate_address(&self, address: &Address) -> Result<AddressValidation>;

    /// Get available services for this provider
    fn get_services(&self) -> Vec<ShippingService>;

    /// Estimate delivery date
    async fn estimate_delivery(
        &self,
        from_address: &Address,
        to_address: &Address,
        service_code: &str,
    ) -> Result<Option<DateTime<Utc>>>;
}

/// Shipping rate for a specific service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShippingRate {
    pub provider_id: String,
    pub carrier: String,
    pub service_code: String,
    pub service_name: String,
    pub rate: Decimal,
    pub currency: String,
    pub delivery_days: Option<i32>,
    pub delivery_date: Option<DateTime<Utc>>,
    pub estimated: bool,
    pub insurance_fee: Option<Decimal>,
    pub fuel_surcharge: Option<Decimal>,
    pub handling_fee: Option<Decimal>,
    pub other_fees: HashMap<String, Decimal>,
    pub total_cost: Decimal,
}

impl ShippingRate {
    /// Create a new shipping rate
    pub fn new(
        provider_id: impl Into<String>,
        carrier: impl Into<String>,
        service_code: impl Into<String>,
        service_name: impl Into<String>,
        rate: Decimal,
        currency: impl Into<String>,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            carrier: carrier.into(),
            service_code: service_code.into(),
            service_name: service_name.into(),
            rate,
            currency: currency.into(),
            delivery_days: None,
            delivery_date: None,
            estimated: true,
            insurance_fee: None,
            fuel_surcharge: None,
            handling_fee: None,
            other_fees: HashMap::new(),
            total_cost: rate,
        }
    }

    /// Add insurance fee
    pub fn with_insurance(mut self, fee: Decimal) -> Self {
        self.insurance_fee = Some(fee);
        self.recalculate_total();
        self
    }

    /// Add fuel surcharge
    pub fn with_fuel_surcharge(mut self, fee: Decimal) -> Self {
        self.fuel_surcharge = Some(fee);
        self.recalculate_total();
        self
    }

    /// Add handling fee
    pub fn with_handling_fee(mut self, fee: Decimal) -> Self {
        self.handling_fee = Some(fee);
        self.recalculate_total();
        self
    }

    /// Set delivery estimate
    pub fn with_delivery(mut self, days: i32, date: Option<DateTime<Utc>>) -> Self {
        self.delivery_days = Some(days);
        self.delivery_date = date;
        self
    }

    /// Recalculate total cost
    fn recalculate_total(&mut self) {
        self.total_cost = self.rate;
        if let Some(fee) = self.insurance_fee {
            self.total_cost += fee;
        }
        if let Some(fee) = self.fuel_surcharge {
            self.total_cost += fee;
        }
        if let Some(fee) = self.handling_fee {
            self.total_cost += fee;
        }
        for fee in self.other_fees.values() {
            self.total_cost += fee;
        }
    }
}

/// Shipment information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shipment {
    pub id: Uuid,
    pub order_id: Option<Uuid>,
    pub provider_id: String,
    pub carrier: String,
    pub service_code: String,
    pub service_name: String,
    pub status: ShipmentStatus,
    pub from_address: Address,
    pub to_address: Address,
    pub package: Package,
    pub tracking_number: Option<String>,
    pub tracking_url: Option<String>,
    pub label_url: Option<String>,
    pub label_data: Option<String>, // Base64 encoded
    pub customs_info: Option<CustomsInfo>,
    pub insurance_amount: Option<Decimal>,
    pub total_cost: Decimal,
    pub currency: String,
    pub created_at: DateTime<Utc>,
    pub shipped_at: Option<DateTime<Utc>>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub estimated_delivery: Option<DateTime<Utc>>,
    pub metadata: HashMap<String, String>,
}

/// Shipment status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "shipment_status", rename_all = "snake_case")]
pub enum ShipmentStatus {
    Pending,
    LabelCreated,
    InTransit,
    OutForDelivery,
    Delivered,
    Failed,
    Cancelled,
    Returned,
}

impl ShipmentStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ShipmentStatus::Delivered | ShipmentStatus::Cancelled | ShipmentStatus::Returned
        )
    }

    pub fn description(&self) -> &'static str {
        match self {
            ShipmentStatus::Pending => "Pending",
            ShipmentStatus::LabelCreated => "Label Created",
            ShipmentStatus::InTransit => "In Transit",
            ShipmentStatus::OutForDelivery => "Out for Delivery",
            ShipmentStatus::Delivered => "Delivered",
            ShipmentStatus::Failed => "Delivery Failed",
            ShipmentStatus::Cancelled => "Cancelled",
            ShipmentStatus::Returned => "Returned",
        }
    }
}

/// Tracking information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackingInfo {
    pub tracking_number: String,
    pub carrier: String,
    pub status: TrackingStatus,
    pub events: Vec<TrackingEvent>,
    pub estimated_delivery: Option<DateTime<Utc>>,
}

/// Tracking status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackingStatus {
    PreTransit,
    InTransit,
    OutForDelivery,
    Delivered,
    AvailableForPickup,
    ReturnToSender,
    Failure,
    Cancelled,
    Exception,
}

impl TrackingStatus {
    pub fn description(&self) -> &'static str {
        match self {
            TrackingStatus::PreTransit => "Pre-transit",
            TrackingStatus::InTransit => "In Transit",
            TrackingStatus::OutForDelivery => "Out for Delivery",
            TrackingStatus::Delivered => "Delivered",
            TrackingStatus::AvailableForPickup => "Available for Pickup",
            TrackingStatus::ReturnToSender => "Return to Sender",
            TrackingStatus::Failure => "Delivery Failed",
            TrackingStatus::Cancelled => "Cancelled",
            TrackingStatus::Exception => "Exception",
        }
    }
}

/// Tracking event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackingEvent {
    pub timestamp: DateTime<Utc>,
    pub status: TrackingStatus,
    pub description: String,
    pub location: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
}

/// Customs information for international shipping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomsInfo {
    pub contents_type: ContentsType,
    pub contents_description: String,
    pub non_delivery_option: NonDeliveryOption,
    pub restriction_type: Option<String>,
    pub restriction_comments: Option<String>,
    pub customs_items: Vec<CustomsItem>,
    pub declaration_value: Decimal,
    pub declaration_currency: String,
}

/// Customs item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomsItem {
    pub description: String,
    pub quantity: i32,
    pub value: Decimal,
    pub currency: String,
    pub weight: Option<Decimal>,
    pub weight_unit: Option<String>,
    pub hs_tariff_number: Option<String>,
    pub origin_country: String,
}

/// Contents type for customs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentsType {
    Merchandise,
    Gift,
    Documents,
    ReturnedGoods,
    Sample,
    Other,
}

impl ContentsType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentsType::Merchandise => "merchandise",
            ContentsType::Gift => "gift",
            ContentsType::Documents => "documents",
            ContentsType::ReturnedGoods => "returned_goods",
            ContentsType::Sample => "sample",
            ContentsType::Other => "other",
        }
    }
}

/// Non-delivery option for customs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NonDeliveryOption {
    Return,
    Abandon,
}

impl NonDeliveryOption {
    pub fn as_str(&self) -> &'static str {
        match self {
            NonDeliveryOption::Return => "return",
            NonDeliveryOption::Abandon => "abandon",
        }
    }
}

/// Rate calculation options
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RateOptions {
    pub carriers: Option<Vec<String>>,
    pub services: Option<Vec<String>>,
    pub include_insurance: bool,
    pub insurance_value: Option<Decimal>,
    pub signature_confirmation: bool,
    pub adult_signature: bool,
    pub saturday_delivery: bool,
    pub hold_for_pickup: bool,
    pub currency: Option<String>,
}

/// Address validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressValidation {
    pub is_valid: bool,
    pub normalized_address: Option<Address>,
    pub messages: Vec<String>,
    pub residential: Option<bool>,
}

/// Shipping service information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShippingService {
    pub code: String,
    pub name: String,
    pub carrier: String,
    pub domestic: bool,
    pub international: bool,
    pub transit_time_days: Option<(i32, i32)>, // (min, max)
    pub features: Vec<ServiceFeature>,
}

/// Service features
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceFeature {
    Tracking,
    Insurance,
    Signature,
    AdultSignature,
    SaturdayDelivery,
    HoldForPickup,
    DeliveryConfirmation,
    Express,
    Ground,
}

/// Shipping provider factory
pub struct ShippingProviderFactory {
    providers: HashMap<String, Box<dyn ShippingProvider>>,
}

impl Clone for ShippingProviderFactory {
    fn clone(&self) -> Self {
        let mut providers = HashMap::new();
        for id in self.providers.keys() {
            // We can't clone Box<dyn ShippingProvider> directly, so recreate from id/name
            // For now, only manual provider is clonable by re-creating it
            if id == "manual" {
                providers.insert(
                    id.clone(),
                    Box::new(crate::shipping::carriers::ManualProvider::new())
                        as Box<dyn ShippingProvider>,
                );
            }
        }
        Self { providers }
    }
}

impl ShippingProviderFactory {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Create a factory from configuration
    pub fn from_config(config: &crate::config::ShippingConfig) -> Self {
        let mut factory = Self::new();

        // Register manual flat-rate provider by default (always available)
        factory.register(Box::new(crate::shipping::carriers::ManualProvider::new()));

        // Register DHL if configured
        if config.dhl.enabled {
            if let (Some(api_key), Some(api_secret), Some(account_number)) = (
                &config.dhl.api_key,
                &config.dhl.api_secret,
                &config.dhl.account_number,
            ) {
                let provider = crate::shipping::carriers::DhlProvider::new(
                    api_key.clone(),
                    api_secret.clone(),
                    account_number.clone(),
                )
                .with_test_mode(config.dhl.sandbox || config.test_mode);
                factory.register(Box::new(provider));
            }
        }

        // Register FedEx if configured
        if config.fedex.enabled {
            if let (Some(api_key), Some(api_secret), Some(account_number)) = (
                &config.fedex.api_key,
                &config.fedex.api_secret,
                &config.fedex.account_number,
            ) {
                let provider = crate::shipping::carriers::FedExProvider::new(
                    api_key.clone(),
                    api_secret.clone(),
                    account_number.clone(),
                )
                .with_test_mode(config.fedex.sandbox || config.test_mode);
                factory.register(Box::new(provider));
            }
        }

        // Register UPS if configured
        if config.ups.enabled {
            if let (Some(api_key), Some(username), Some(password), Some(account_number)) = (
                &config.ups.api_key,
                &config.ups.username,
                &config.ups.password,
                &config.ups.account_number,
            ) {
                let provider = crate::shipping::carriers::UpsProvider::new(
                    api_key.clone(),
                    username.clone(),
                    password.clone(),
                    account_number.clone(),
                )
                .with_test_mode(config.ups.sandbox || config.test_mode);
                factory.register(Box::new(provider));
            }
        }

        // Register USPS if configured
        if config.usps.enabled {
            if let Some(api_key) = &config.usps.api_key {
                let provider = crate::shipping::carriers::UspsProvider::new(api_key.clone())
                    .with_test_mode(config.usps.sandbox || config.test_mode);
                factory.register(Box::new(provider));
            }
        }

        factory
    }

    /// Register a provider
    pub fn register(&mut self, provider: Box<dyn ShippingProvider>) {
        self.providers.insert(provider.id().to_string(), provider);
    }

    /// Get a provider by ID
    pub fn get(&self, id: &str) -> Result<&dyn ShippingProvider> {
        self.providers
            .get(id)
            .map(|p| p.as_ref())
            .ok_or_else(|| Error::not_found(format!("Shipping provider '{}' not found", id)))
    }

    /// Get all available providers
    pub fn get_available(&self) -> Vec<&dyn ShippingProvider> {
        self.providers
            .values()
            .filter(|p| p.is_available())
            .map(|p| p.as_ref())
            .collect()
    }

    /// Get all registered providers
    pub fn get_all(&self) -> Vec<&dyn ShippingProvider> {
        self.providers.values().map(|p| p.as_ref()).collect()
    }

    /// Check if provider exists
    pub fn has(&self, id: &str) -> bool {
        self.providers.contains_key(id)
    }
}

impl Default for ShippingProviderFactory {
    fn default() -> Self {
        Self::new()
    }
}

/// Multi-provider rate aggregator
pub struct ShippingRateAggregator {
    factory: ShippingProviderFactory,
}

impl ShippingRateAggregator {
    pub fn new(factory: ShippingProviderFactory) -> Self {
        Self { factory }
    }

    /// Get rates from all available providers
    pub async fn get_all_rates(
        &self,
        from_address: &Address,
        to_address: &Address,
        package: &Package,
        options: &RateOptions,
    ) -> Result<Vec<ShippingRate>> {
        let mut all_rates = Vec::new();

        for provider in self.factory.get_available() {
            match provider
                .get_rates(from_address, to_address, package, options)
                .await
            {
                Ok(mut rates) => all_rates.append(&mut rates),
                Err(e) => {
                    tracing::warn!("Failed to get rates from {}: {}", provider.name(), e);
                }
            }
        }

        // Sort by total cost
        all_rates.sort_by(|a, b| a.total_cost.cmp(&b.total_cost));

        Ok(all_rates)
    }

    /// Get rates from specific providers
    pub async fn get_rates_from_providers(
        &self,
        provider_ids: &[String],
        from_address: &Address,
        to_address: &Address,
        package: &Package,
        options: &RateOptions,
    ) -> Result<Vec<ShippingRate>> {
        let mut all_rates = Vec::new();

        for id in provider_ids {
            let provider = self.factory.get(id)?;
            match provider
                .get_rates(from_address, to_address, package, options)
                .await
            {
                Ok(mut rates) => all_rates.append(&mut rates),
                Err(e) => {
                    tracing::warn!("Failed to get rates from {}: {}", provider.name(), e);
                }
            }
        }

        all_rates.sort_by(|a, b| a.total_cost.cmp(&b.total_cost));
        Ok(all_rates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_shipping_rate_builder() {
        let rate = ShippingRate::new("ups", "UPS", "ground", "UPS Ground", dec!(10.00), "USD")
            .with_insurance(dec!(2.00))
            .with_fuel_surcharge(dec!(1.50))
            .with_delivery(3, None);

        assert_eq!(rate.total_cost, dec!(13.50));
        assert_eq!(rate.delivery_days, Some(3));
    }

    #[test]
    fn test_shipment_status() {
        assert!(ShipmentStatus::Delivered.is_terminal());
        assert!(ShipmentStatus::Cancelled.is_terminal());
        assert!(!ShipmentStatus::InTransit.is_terminal());
    }
}
