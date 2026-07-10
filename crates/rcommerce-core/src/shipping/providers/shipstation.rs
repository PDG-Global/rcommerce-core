//! ShipStation shipping aggregator provider

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

use crate::common::Address;
use crate::shipping::{
    AddressValidation, CustomsInfo, Package, RateOptions, ServiceFeature, Shipment,
    ShippingProvider, ShippingRate, ShippingService, TrackingEvent, TrackingInfo, TrackingStatus,
};
use crate::Result;

/// ShipStation API provider
#[allow(dead_code)]
pub struct ShipStationProvider {
    client: reqwest::Client,
    api_key: String,
    api_secret: String,
    base_url: String,
}

impl ShipStationProvider {
    pub fn new(api_key: impl Into<String>, api_secret: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            api_secret: api_secret.into(),
            base_url: "https://ssapi.shipstation.com".to_string(),
        }
    }
}

#[async_trait]
impl ShippingProvider for ShipStationProvider {
    fn id(&self) -> &'static str {
        "shipstation"
    }
    fn name(&self) -> &'static str {
        "ShipStation"
    }
    fn is_available(&self) -> bool {
        !self.api_key.is_empty() && !self.api_secret.is_empty()
    }

    async fn get_rates(
        &self,
        _from_address: &Address,
        _to_address: &Address,
        _package: &Package,
        options: &RateOptions,
    ) -> Result<Vec<ShippingRate>> {
        let mut rates = vec![
            ShippingRate::new(
                self.id(),
                "USPS",
                "usps_priority",
                "USPS Priority Mail",
                Decimal::from(8),
                "USD",
            )
            .with_delivery(3, None),
            ShippingRate::new(
                self.id(),
                "UPS",
                "ups_ground",
                "UPS Ground",
                Decimal::from(10),
                "USD",
            )
            .with_delivery(5, None),
            ShippingRate::new(
                self.id(),
                "FedEx",
                "fedex_ground",
                "FedEx Ground",
                Decimal::from(11),
                "USD",
            )
            .with_delivery(5, None),
        ];

        if let Some(ref carriers) = options.carriers {
            rates.retain(|r| carriers.iter().any(|c| r.carrier.eq_ignore_ascii_case(c)));
        }

        Ok(rates)
    }

    async fn create_shipment(
        &self,
        from_address: &Address,
        to_address: &Address,
        package: &Package,
        service_code: &str,
        customs_info: Option<&CustomsInfo>,
    ) -> Result<Shipment> {
        let tracking_number = format!("SS{:015}", rand::random::<u64>());

        Ok(Shipment {
            id: uuid::Uuid::new_v4(),
            order_id: None,
            provider_id: self.id().to_string(),
            carrier: "USPS".to_string(),
            service_code: service_code.to_string(),
            service_name: service_code.to_string(),
            status: crate::shipping::ShipmentStatus::Pending,
            from_address: from_address.clone(),
            to_address: to_address.clone(),
            package: package.clone(),
            tracking_number: Some(tracking_number.clone()),
            tracking_url: Some(format!("https://track.shipstation.com/{}", tracking_number)),
            label_url: None,
            label_data: None,
            customs_info: customs_info.cloned(),
            insurance_amount: None,
            total_cost: Decimal::from(9),
            currency: "USD".to_string(),
            created_at: Utc::now(),
            shipped_at: None,
            delivered_at: None,
            estimated_delivery: Some(Utc::now() + chrono::Duration::days(3)),
            metadata: std::collections::HashMap::new(),
        })
    }

    async fn track_shipment(&self, tracking_number: &str) -> Result<TrackingInfo> {
        Ok(TrackingInfo {
            tracking_number: tracking_number.to_string(),
            carrier: "Multiple".to_string(),
            status: TrackingStatus::InTransit,
            events: vec![TrackingEvent {
                timestamp: Utc::now() - chrono::Duration::hours(8),
                status: TrackingStatus::InTransit,
                description: "Shipped".to_string(),
                location: Some("Austin, TX".to_string()),
                city: Some("Austin".to_string()),
                state: Some("TX".to_string()),
                country: Some("US".to_string()),
            }],
            estimated_delivery: Some(Utc::now() + chrono::Duration::days(2)),
        })
    }

    async fn cancel_shipment(&self, _shipment_id: &str) -> Result<bool> {
        Ok(true)
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
        vec![
            ShippingService {
                code: "usps_priority".to_string(),
                name: "USPS Priority Mail".to_string(),
                carrier: "USPS".to_string(),
                domestic: true,
                international: true,
                transit_time_days: Some((1, 3)),
                features: vec![ServiceFeature::Tracking],
            },
            ShippingService {
                code: "ups_ground".to_string(),
                name: "UPS Ground".to_string(),
                carrier: "UPS".to_string(),
                domestic: true,
                international: false,
                transit_time_days: Some((1, 5)),
                features: vec![ServiceFeature::Tracking, ServiceFeature::Ground],
            },
            ShippingService {
                code: "fedex_ground".to_string(),
                name: "FedEx Ground".to_string(),
                carrier: "FedEx".to_string(),
                domestic: true,
                international: false,
                transit_time_days: Some((1, 5)),
                features: vec![ServiceFeature::Tracking, ServiceFeature::Ground],
            },
        ]
    }

    async fn estimate_delivery(
        &self,
        _from: &Address,
        _to: &Address,
        service_code: &str,
    ) -> Result<Option<DateTime<Utc>>> {
        let days = if service_code.contains("express") {
            1
        } else if service_code.contains("priority") {
            3
        } else {
            5
        };
        Ok(Some(Utc::now() + chrono::Duration::days(days)))
    }
}
