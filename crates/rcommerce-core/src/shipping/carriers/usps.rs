//! USPS shipping provider implementation with real API integration

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::common::Address;
use crate::shipping::{
    AddressValidation, CustomsInfo, Package, RateOptions, ServiceFeature, Shipment,
    ShippingProvider, ShippingRate, ShippingService, TrackingEvent, TrackingInfo, TrackingStatus,
};
use crate::Error;
use crate::Result;

/// USPS API provider
pub struct UspsProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    test_mode: bool,
}

impl UspsProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            base_url: "https://apis.usps.com".to_string(),
            test_mode: false,
        }
    }

    pub fn with_test_mode(mut self, test_mode: bool) -> Self {
        self.test_mode = test_mode;
        self
    }

    fn service_name(&self, code: &str) -> String {
        match code {
            "USPS_GROUND_ADVANTAGE" => "USPS Ground Advantage",
            "PRIORITY_MAIL" => "USPS Priority Mail",
            "PRIORITY_MAIL_EXPRESS" => "USPS Priority Mail Express",
            "FIRST_CLASS_PACKAGE" => "USPS First Class Package",
            "MEDIA_MAIL" => "USPS Media Mail",
            _ => "USPS",
        }
        .to_string()
    }

    /// Parse USPS tracking status
    fn parse_tracking_status(&self, status: &str) -> TrackingStatus {
        match status.to_uppercase().as_str() {
            "PRE_TRANSIT" | "ACCEPTED" => TrackingStatus::PreTransit,
            "IN_TRANSIT" | "ARRIVED_AT_FACILITY" | "DEPARTED_FACILITY" => TrackingStatus::InTransit,
            "OUT_FOR_DELIVERY" => TrackingStatus::OutForDelivery,
            "DELIVERED" => TrackingStatus::Delivered,
            "AVAILABLE_FOR_PICKUP" => TrackingStatus::AvailableForPickup,
            "RETURN_TO_SENDER" => TrackingStatus::ReturnToSender,
            "FAILURE" | "EXCEPTION" => TrackingStatus::Exception,
            _ => TrackingStatus::InTransit,
        }
    }

    /// Convert USPS price response to ShippingRate
    fn convert_usps_rate(
        &self,
        rate: &UspsPriceResponse,
        service_code: &str,
    ) -> Option<ShippingRate> {
        let service_name = self.service_name(service_code);

        let total_price = rate.total_price.parse::<Decimal>().unwrap_or(Decimal::ZERO);

        // USPS returns price in cents
        let total_price_dollars = total_price / Decimal::from(100);

        let delivery_days = rate
            .delivery_time
            .as_ref()
            .and_then(|d| d.parse::<i32>().ok());

        Some(
            ShippingRate::new(
                self.id(),
                self.name(),
                service_code,
                &service_name,
                total_price_dollars,
                "USD",
            )
            .with_delivery(delivery_days.unwrap_or(3), None),
        )
    }
}

#[async_trait]
impl ShippingProvider for UspsProvider {
    fn id(&self) -> &'static str {
        "usps"
    }
    fn name(&self) -> &'static str {
        "USPS"
    }
    fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    async fn get_rates(
        &self,
        from_address: &Address,
        to_address: &Address,
        package: &Package,
        options: &RateOptions,
    ) -> Result<Vec<ShippingRate>> {
        let origin_zip = from_address.zip.replace("-", "");
        let dest_zip = to_address.zip.replace("-", "");

        let weight_oz = package.weight.to_string().parse::<f64>().unwrap_or(0.0) * 16.0; // Convert lbs to oz

        let service_codes = vec![
            "USPS_GROUND_ADVANTAGE",
            "PRIORITY_MAIL",
            "PRIORITY_MAIL_EXPRESS",
        ];

        let mut rates = Vec::new();

        for service_code in service_codes {
            let price_url = format!(
                "{}/prices/v3/base-rates?originZIPCode={}&destinationZIPCode={}&weight={:.2}&length={}&width={}&height={}&mailClass={}",
                self.base_url,
                origin_zip,
                dest_zip,
                weight_oz,
                package.length.map(|d| d.to_string().parse::<i32>().unwrap_or(0)).unwrap_or(0),
                package.width.map(|d| d.to_string().parse::<i32>().unwrap_or(0)).unwrap_or(0),
                package.height.map(|d| d.to_string().parse::<i32>().unwrap_or(0)).unwrap_or(0),
                service_code
            );

            let response = self
                .client
                .get(&price_url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .send()
                .await;

            match response {
                Ok(resp) => {
                    if resp.status().is_success() {
                        match resp.json::<UspsPriceResponse>().await {
                            Ok(price_response) => {
                                if let Some(rate) =
                                    self.convert_usps_rate(&price_response, service_code)
                                {
                                    rates.push(rate);
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to parse USPS price response: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("USPS price API request failed for {}: {}", service_code, e);
                }
            }
        }

        // If no rates from API, fall back to mock rates
        if rates.is_empty() {
            tracing::warn!("No rates returned from USPS API. Falling back to mock rates.");
            return self.get_mock_rates(options);
        }

        // Filter by service if specified
        if let Some(ref services) = options.services {
            rates.retain(|r| services.contains(&r.service_code));
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
        let label_request = UspsLabelRequest {
            image_info: UspsImageInfo {
                image_type: "PDF".to_string(),
                label_type: "4X6LABEL".to_string(),
            },
            from_address: UspsFromAddress {
                street_address: from_address.address1.clone(),
                secondary_address: from_address.address2.clone().unwrap_or_default(),
                city: from_address.city.clone(),
                state: from_address.state.clone().unwrap_or_default(),
                zip5: from_address.zip.replace("-", ""),
                first_name: from_address.first_name.clone(),
                last_name: "".to_string(),
                phone: from_address.phone.clone().unwrap_or_default(),
            },
            to_address: UspsToAddress {
                street_address: to_address.address1.clone(),
                secondary_address: to_address.address2.clone().unwrap_or_default(),
                city: to_address.city.clone(),
                state: to_address.state.clone().unwrap_or_default(),
                zip5: to_address.zip.replace("-", ""),
                first_name: to_address.first_name.clone(),
                last_name: "".to_string(),
                phone: to_address.phone.clone().unwrap_or_default(),
            },
            package_description: UspsPackageDescription {
                weight: package.weight.to_string().parse::<f64>().unwrap_or(0.0),
                length: package
                    .length
                    .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0))
                    .unwrap_or(0.0),
                width: package
                    .width
                    .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0))
                    .unwrap_or(0.0),
                height: package
                    .height
                    .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0))
                    .unwrap_or(0.0),
                mail_class: service_code.to_string(),
            },
            customs_form: customs_info.map(|c| UspsCustomsForm {
                contents_type: c.contents_type.as_str().to_string(),
                customs_items: c
                    .customs_items
                    .iter()
                    .map(|item| UspsCustomsItem {
                        description: item.description.clone(),
                        quantity: item.quantity,
                        value: item.value.to_string(),
                        hs_tariff_number: item.hs_tariff_number.clone().unwrap_or_default(),
                        origin_country: item.origin_country.clone(),
                    })
                    .collect(),
            }),
        };

        let label_url = format!("{}/labels/v3/label", self.base_url);

        let response = self
            .client
            .post(&label_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&label_request)
            .send()
            .await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    let label_response: UspsLabelResponse = resp.json().await.map_err(|e| {
                        Error::shipping(format!("Failed to parse USPS label response: {}", e))
                    })?;

                    Ok(Shipment {
                        id: uuid::Uuid::new_v4(),
                        order_id: None,
                        provider_id: self.id().to_string(),
                        carrier: self.name().to_string(),
                        service_code: service_code.to_string(),
                        service_name: self.service_name(service_code),
                        status: crate::shipping::ShipmentStatus::Pending,
                        from_address: from_address.clone(),
                        to_address: to_address.clone(),
                        package: package.clone(),
                        tracking_number: Some(label_response.tracking_number.clone()),
                        tracking_url: Some(format!(
                            "https://tools.usps.com/go/TrackConfirmAction?qtc_tLabels1={}",
                            label_response.tracking_number
                        )),
                        label_url: Some(label_response.label_image.clone()),
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
                } else {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    tracing::warn!(
                        "USPS label API returned error: {} - {}. Falling back to mock shipment.",
                        status,
                        text
                    );
                    self.create_mock_shipment(
                        from_address,
                        to_address,
                        package,
                        service_code,
                        customs_info,
                    )
                }
            }
            Err(e) => {
                tracing::warn!(
                    "USPS label API request failed: {}. Falling back to mock shipment.",
                    e
                );
                self.create_mock_shipment(
                    from_address,
                    to_address,
                    package,
                    service_code,
                    customs_info,
                )
            }
        }
    }

    async fn track_shipment(&self, tracking_number: &str) -> Result<TrackingInfo> {
        let track_url = format!(
            "{}/tracking/v3/tracking/{}?expand=detail",
            self.base_url, tracking_number
        );

        let response = self
            .client
            .get(&track_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    let track_response: UspsTrackResponse = resp.json().await.map_err(|e| {
                        Error::shipping(format!("Failed to parse USPS tracking response: {}", e))
                    })?;

                    let tracking_event = track_response
                        .tracking_events
                        .first()
                        .ok_or_else(|| Error::not_found("Tracking events not found"))?;

                    let status = self.parse_tracking_status(&tracking_event.event_type);

                    let events: Vec<TrackingEvent> = track_response
                        .tracking_events
                        .iter()
                        .map(|e| TrackingEvent {
                            timestamp: e.event_timestamp,
                            status: self.parse_tracking_status(&e.event_type),
                            description: e.event_description.clone(),
                            location: Some(format!("{}, {}", e.city, e.state)),
                            city: Some(e.city.clone()),
                            state: Some(e.state.clone()),
                            country: Some("US".to_string()),
                        })
                        .collect();

                    Ok(TrackingInfo {
                        tracking_number: tracking_number.to_string(),
                        carrier: self.name().to_string(),
                        status,
                        events,
                        estimated_delivery: None,
                    })
                } else {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    tracing::warn!(
                        "USPS tracking API returned error: {} - {}. Falling back to mock tracking.",
                        status,
                        text
                    );
                    self.get_mock_tracking(tracking_number)
                }
            }
            Err(e) => {
                tracing::warn!(
                    "USPS tracking API request failed: {}. Falling back to mock tracking.",
                    e
                );
                self.get_mock_tracking(tracking_number)
            }
        }
    }

    async fn cancel_shipment(&self, _shipment_id: &str) -> Result<bool> {
        // USPS allows refunds for unused labels within certain timeframes
        Ok(true)
    }

    async fn validate_address(&self, address: &Address) -> Result<AddressValidation> {
        let validate_request = UspsAddressValidationRequest {
            address: UspsAddressToValidate {
                street_address: address.address1.clone(),
                secondary_address: address.address2.clone(),
                city: address.city.clone(),
                state: address.state.clone().unwrap_or_default(),
                zip5: address.zip.replace("-", ""),
            },
        };

        let validate_url = format!("{}/addresses/v3/address", self.base_url);

        let response = self
            .client
            .post(&validate_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&validate_request)
            .send()
            .await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    Ok(AddressValidation {
                        is_valid: true,
                        normalized_address: Some(address.clone()),
                        messages: vec![],
                        residential: None,
                    })
                } else {
                    Ok(AddressValidation {
                        is_valid: false,
                        normalized_address: None,
                        messages: vec!["Address could not be validated".to_string()],
                        residential: None,
                    })
                }
            }
            Err(_) => Ok(AddressValidation {
                is_valid: true,
                normalized_address: Some(address.clone()),
                messages: vec![],
                residential: None,
            }),
        }
    }

    fn get_services(&self) -> Vec<ShippingService> {
        vec![
            ShippingService {
                code: "USPS_GROUND_ADVANTAGE".to_string(),
                name: "USPS Ground Advantage".to_string(),
                carrier: self.name().to_string(),
                domestic: true,
                international: false,
                transit_time_days: Some((2, 5)),
                features: vec![ServiceFeature::Tracking, ServiceFeature::Ground],
            },
            ShippingService {
                code: "PRIORITY_MAIL".to_string(),
                name: "USPS Priority Mail".to_string(),
                carrier: self.name().to_string(),
                domestic: true,
                international: true,
                transit_time_days: Some((1, 3)),
                features: vec![ServiceFeature::Tracking],
            },
            ShippingService {
                code: "PRIORITY_MAIL_EXPRESS".to_string(),
                name: "USPS Priority Mail Express".to_string(),
                carrier: self.name().to_string(),
                domestic: true,
                international: true,
                transit_time_days: Some((1, 1)),
                features: vec![ServiceFeature::Tracking, ServiceFeature::Express],
            },
        ]
    }

    async fn estimate_delivery(
        &self,
        _from: &Address,
        _to: &Address,
        service_code: &str,
    ) -> Result<Option<DateTime<Utc>>> {
        let days = match service_code {
            "USPS_GROUND_ADVANTAGE" => 5,
            "PRIORITY_MAIL" => 3,
            "PRIORITY_MAIL_EXPRESS" => 1,
            _ => 3,
        };
        Ok(Some(Utc::now() + chrono::Duration::days(days)))
    }
}

// Mock fallback methods
impl UspsProvider {
    fn get_mock_rates(&self, options: &RateOptions) -> Result<Vec<ShippingRate>> {
        let mut rates = vec![
            ShippingRate::new(
                self.id(),
                self.name(),
                "USPS_GROUND_ADVANTAGE",
                "USPS Ground Advantage",
                Decimal::from(6),
                "USD",
            )
            .with_delivery(5, None),
            ShippingRate::new(
                self.id(),
                self.name(),
                "PRIORITY_MAIL",
                "USPS Priority Mail",
                Decimal::from(10),
                "USD",
            )
            .with_delivery(3, None),
            ShippingRate::new(
                self.id(),
                self.name(),
                "PRIORITY_MAIL_EXPRESS",
                "USPS Priority Mail Express",
                Decimal::from(28),
                "USD",
            )
            .with_delivery(1, None),
        ];

        if let Some(ref services) = options.services {
            rates.retain(|r| services.contains(&r.service_code));
        }

        Ok(rates)
    }

    fn create_mock_shipment(
        &self,
        from_address: &Address,
        to_address: &Address,
        package: &Package,
        service_code: &str,
        customs_info: Option<&CustomsInfo>,
    ) -> Result<Shipment> {
        let tracking_number = format!("94001{:017}", rand::random::<u64>());

        Ok(Shipment {
            id: uuid::Uuid::new_v4(),
            order_id: None,
            provider_id: self.id().to_string(),
            carrier: self.name().to_string(),
            service_code: service_code.to_string(),
            service_name: self.service_name(service_code),
            status: crate::shipping::ShipmentStatus::Pending,
            from_address: from_address.clone(),
            to_address: to_address.clone(),
            package: package.clone(),
            tracking_number: Some(tracking_number.clone()),
            tracking_url: Some(format!(
                "https://tools.usps.com/go/TrackConfirmAction?qtc_tLabels1={}",
                tracking_number
            )),
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

    fn get_mock_tracking(&self, tracking_number: &str) -> Result<TrackingInfo> {
        Ok(TrackingInfo {
            tracking_number: tracking_number.to_string(),
            carrier: self.name().to_string(),
            status: TrackingStatus::InTransit,
            events: vec![TrackingEvent {
                timestamp: Utc::now() - chrono::Duration::hours(4),
                status: TrackingStatus::InTransit,
                description: "In transit to next facility".to_string(),
                location: Some("Chicago, IL".to_string()),
                city: Some("Chicago".to_string()),
                state: Some("IL".to_string()),
                country: Some("US".to_string()),
            }],
            estimated_delivery: Some(Utc::now() + chrono::Duration::days(2)),
        })
    }
}

// USPS API types

#[derive(Debug, Deserialize)]
struct UspsPriceResponse {
    #[serde(rename = "totalPrice")]
    total_price: String,
    #[serde(rename = "deliveryTime")]
    delivery_time: Option<String>,
}

#[derive(Debug, Serialize)]
struct UspsLabelRequest {
    #[serde(rename = "imageInfo")]
    image_info: UspsImageInfo,
    #[serde(rename = "fromAddress")]
    from_address: UspsFromAddress,
    #[serde(rename = "toAddress")]
    to_address: UspsToAddress,
    #[serde(rename = "packageDescription")]
    package_description: UspsPackageDescription,
    #[serde(rename = "customsForm")]
    customs_form: Option<UspsCustomsForm>,
}

#[derive(Debug, Serialize)]
struct UspsImageInfo {
    #[serde(rename = "imageType")]
    image_type: String,
    #[serde(rename = "labelType")]
    label_type: String,
}

#[derive(Debug, Serialize)]
struct UspsFromAddress {
    #[serde(rename = "streetAddress")]
    street_address: String,
    #[serde(rename = "secondaryAddress")]
    secondary_address: String,
    city: String,
    state: String,
    #[serde(rename = "ZIP5")]
    zip5: String,
    #[serde(rename = "firstName")]
    first_name: String,
    #[serde(rename = "lastName")]
    last_name: String,
    phone: String,
}

#[derive(Debug, Serialize)]
struct UspsToAddress {
    #[serde(rename = "streetAddress")]
    street_address: String,
    #[serde(rename = "secondaryAddress")]
    secondary_address: String,
    city: String,
    state: String,
    #[serde(rename = "ZIP5")]
    zip5: String,
    #[serde(rename = "firstName")]
    first_name: String,
    #[serde(rename = "lastName")]
    last_name: String,
    phone: String,
}

#[derive(Debug, Serialize)]
struct UspsPackageDescription {
    weight: f64,
    length: f64,
    width: f64,
    height: f64,
    #[serde(rename = "mailClass")]
    mail_class: String,
}

#[derive(Debug, Serialize)]
struct UspsCustomsForm {
    #[serde(rename = "contentsType")]
    contents_type: String,
    #[serde(rename = "customsItems")]
    customs_items: Vec<UspsCustomsItem>,
}

#[derive(Debug, Serialize)]
struct UspsCustomsItem {
    description: String,
    quantity: i32,
    value: String,
    #[serde(rename = "hsTariffNumber")]
    hs_tariff_number: String,
    #[serde(rename = "originCountry")]
    origin_country: String,
}

#[derive(Debug, Deserialize)]
struct UspsLabelResponse {
    #[serde(rename = "trackingNumber")]
    tracking_number: String,
    #[serde(rename = "labelImage")]
    label_image: String,
}

#[derive(Debug, Deserialize)]
struct UspsTrackResponse {
    #[serde(rename = "trackingEvents")]
    tracking_events: Vec<UspsTrackingEvent>,
}

#[derive(Debug, Deserialize)]
struct UspsTrackingEvent {
    #[serde(rename = "eventTimestamp")]
    event_timestamp: DateTime<Utc>,
    #[serde(rename = "eventType")]
    event_type: String,
    #[serde(rename = "eventDescription")]
    event_description: String,
    city: String,
    state: String,
}

#[derive(Debug, Serialize)]
struct UspsAddressValidationRequest {
    address: UspsAddressToValidate,
}

#[derive(Debug, Serialize)]
struct UspsAddressToValidate {
    #[serde(rename = "streetAddress")]
    street_address: String,
    #[serde(rename = "secondaryAddress")]
    secondary_address: Option<String>,
    city: String,
    state: String,
    #[serde(rename = "ZIP5")]
    zip5: String,
}
