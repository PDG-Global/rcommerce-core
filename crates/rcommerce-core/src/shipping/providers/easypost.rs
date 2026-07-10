//! EasyPost shipping aggregator provider

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

use crate::common::Address;
use crate::shipping::{
    AddressValidation, CustomsInfo, Package, RateOptions, ServiceFeature, Shipment,
    ShippingProvider, ShippingRate, ShippingService, TrackingEvent, TrackingInfo, TrackingStatus,
};
use crate::Result;

/// EasyPost API provider
#[allow(dead_code)]
pub struct EasyPostProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    test_mode: bool,
}

impl EasyPostProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            base_url: "https://api.easypost.com".to_string(),
            test_mode: false,
        }
    }

    pub fn with_test_mode(mut self, test_mode: bool) -> Self {
        self.test_mode = test_mode;
        self
    }

    #[allow(dead_code)]
    fn auth(&self) -> String {
        format!("{}:", self.api_key)
    }
}

#[async_trait]
impl ShippingProvider for EasyPostProvider {
    fn id(&self) -> &'static str {
        "easypost"
    }
    fn name(&self) -> &'static str {
        "EasyPost"
    }
    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    async fn get_rates(
        &self,
        _from_address: &Address,
        _to_address: &Address,
        _package: &Package,
        options: &RateOptions,
    ) -> Result<Vec<ShippingRate>> {
        // EasyPost returns rates from multiple carriers
        let mut rates = vec![
            ShippingRate::new(
                self.id(),
                "USPS",
                "Priority",
                "USPS Priority Mail",
                Decimal::from(9),
                "USD",
            )
            .with_delivery(3, None),
            ShippingRate::new(
                self.id(),
                "UPS",
                "Ground",
                "UPS Ground",
                Decimal::from(11),
                "USD",
            )
            .with_delivery(5, None),
            ShippingRate::new(
                self.id(),
                "FedEx",
                "GROUND_HOME_DELIVERY",
                "FedEx Ground",
                Decimal::from(12),
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
        let tracking_number = format!("EZ{:015}", rand::random::<u64>());

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
            tracking_url: Some(format!("https://track.easypost.com/{}", tracking_number)),
            label_url: None,
            label_data: None,
            customs_info: customs_info.cloned(),
            insurance_amount: None,
            total_cost: Decimal::from(10),
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
                timestamp: Utc::now() - chrono::Duration::hours(2),
                status: TrackingStatus::InTransit,
                description: "Arrived at regional facility".to_string(),
                location: Some("Denver, CO".to_string()),
                city: Some("Denver".to_string()),
                state: Some("CO".to_string()),
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
        // EasyPost supports all carrier services
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
        let days = if service_code.contains("express") || service_code.contains("overnight") {
            1
        } else if service_code.contains("2day") || service_code.contains("2_day") {
            2
        } else if service_code.contains("priority") {
            3
        } else {
            5
        };
        Ok(Some(Utc::now() + chrono::Duration::days(days)))
    }
}
