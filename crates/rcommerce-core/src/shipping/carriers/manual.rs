//! Manual shipping provider with flat-rate options

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::common::Address;
use crate::shipping::{
    AddressValidation, CustomsInfo, Package, RateOptions, ServiceFeature, Shipment,
    ShippingProvider, ShippingRate, ShippingService, TrackingEvent, TrackingInfo, TrackingStatus,
};
use crate::Error;
use crate::Result;

/// Manual flat-rate shipping provider
pub struct ManualProvider {
    rates: Vec<ManualRate>,
}

#[derive(Debug, Clone)]
struct ManualRate {
    service_code: String,
    service_name: String,
    rate: Decimal,
    delivery_days: Option<i32>,
}

impl ManualProvider {
    pub fn new() -> Self {
        Self {
            rates: vec![
                ManualRate {
                    service_code: "standard".to_string(),
                    service_name: "Standard Shipping".to_string(),
                    rate: dec!(10.00),
                    delivery_days: Some(5),
                },
                ManualRate {
                    service_code: "express".to_string(),
                    service_name: "Express Shipping".to_string(),
                    rate: dec!(25.00),
                    delivery_days: Some(2),
                },
                ManualRate {
                    service_code: "free".to_string(),
                    service_name: "Free Shipping".to_string(),
                    rate: Decimal::ZERO,
                    delivery_days: Some(7),
                },
            ],
        }
    }
}

#[async_trait]
impl ShippingProvider for ManualProvider {
    fn id(&self) -> &'static str {
        "manual"
    }

    fn name(&self) -> &'static str {
        "Manual Flat Rate"
    }

    fn is_available(&self) -> bool {
        true
    }

    async fn get_rates(
        &self,
        _from_address: &Address,
        _to_address: &Address,
        _package: &Package,
        _options: &RateOptions,
    ) -> Result<Vec<ShippingRate>> {
        Ok(self
            .rates
            .iter()
            .map(|r| {
                ShippingRate::new(
                    "manual",
                    "Manual",
                    &r.service_code,
                    &r.service_name,
                    r.rate,
                    "USD",
                )
                .with_delivery(r.delivery_days.unwrap_or(5), None)
            })
            .collect())
    }

    async fn create_shipment(
        &self,
        _from_address: &Address,
        _to_address: &Address,
        _package: &Package,
        service_code: &str,
        _customs_info: Option<&CustomsInfo>,
    ) -> Result<Shipment> {
        Err(Error::shipping(format!(
            "Manual provider cannot create real shipments for service {}",
            service_code
        )))
    }

    async fn track_shipment(&self, tracking_number: &str) -> Result<TrackingInfo> {
        Err(Error::shipping(format!(
            "Manual provider cannot track shipments: {}",
            tracking_number
        )))
    }

    async fn cancel_shipment(&self, _shipment_id: &str) -> Result<bool> {
        Ok(false)
    }

    async fn validate_address(&self, address: &Address) -> Result<AddressValidation> {
        Ok(AddressValidation {
            is_valid: true,
            normalized_address: Some(address.clone()),
            messages: vec![],
            residential: Some(false),
        })
    }

    fn get_services(&self) -> Vec<ShippingService> {
        self.rates
            .iter()
            .map(|r| ShippingService {
                code: r.service_code.clone(),
                name: r.service_name.clone(),
                carrier: "Manual".to_string(),
                domestic: true,
                international: true,
                transit_time_days: r.delivery_days.map(|d| (d, d + 2)),
                features: vec![ServiceFeature::Tracking],
            })
            .collect()
    }

    async fn estimate_delivery(
        &self,
        _from_address: &Address,
        _to_address: &Address,
        service_code: &str,
    ) -> Result<Option<DateTime<Utc>>> {
        let days = self
            .rates
            .iter()
            .find(|r| r.service_code == service_code)
            .and_then(|r| r.delivery_days)
            .unwrap_or(5);
        Ok(Some(Utc::now() + chrono::Duration::days(days.into())))
    }
}
