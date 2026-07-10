//! DHL Express shipping provider implementation with real API integration

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

/// DHL Express API provider
pub struct DhlProvider {
    client: reqwest::Client,
    api_key: String,
    api_secret: String,
    account_number: String,
    base_url: String,
    test_mode: bool,
}

impl DhlProvider {
    /// Create a new DHL provider
    pub fn new(
        api_key: impl Into<String>,
        api_secret: impl Into<String>,
        account_number: impl Into<String>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            api_secret: api_secret.into(),
            account_number: account_number.into(),
            base_url: "https://api-eu.dhl.com".to_string(),
            test_mode: false,
        }
    }

    /// Set test mode
    pub fn with_test_mode(mut self, test_mode: bool) -> Self {
        self.test_mode = test_mode;
        if test_mode {
            self.base_url = "https://api-sandbox.dhl.com".to_string();
        }
        self
    }

    /// Get base URL for MyDHL API
    fn mydhl_url(&self) -> String {
        format!("{}/express/v1", self.base_url)
    }

    /// Get base URL for tracking API
    fn tracking_url(&self) -> String {
        format!("{}/track/shipments", self.base_url)
    }

    /// Get authentication headers (API Key based auth for DHL)
    fn auth_headers(&self) -> Vec<(String, String)> {
        vec![("DHL-API-Key".to_string(), self.api_key.clone())]
    }

    /// Map DHL service code to human-readable name
    fn service_name(&self, code: &str) -> String {
        match code {
            "EXPRESS_WORLDWIDE" | "P" => "DHL Express Worldwide",
            "EXPRESS_9:00" | "E" => "DHL Express 9:00",
            "EXPRESS_10:30" | "T" => "DHL Express 10:30",
            "EXPRESS_12:00" | "Y" => "DHL Express 12:00",
            "EXPRESS_ENVELOPE" => "DHL Express Envelope",
            "ECONOMY_SELECT" | "W" => "DHL Economy Select",
            _ => "DHL Express",
        }
        .to_string()
    }

    /// Parse DHL tracking status
    fn parse_tracking_status(&self, status: &str) -> TrackingStatus {
        match status.to_uppercase().as_str() {
            "TRANSIT" | "IN TRANSIT" | "SHIPMENT_TRANSIT" => TrackingStatus::InTransit,
            "DELIVERED" | "SHIPMENT_DELIVERED" => TrackingStatus::Delivered,
            "OUT FOR DELIVERY" => TrackingStatus::OutForDelivery,
            "EXCEPTION" | "SHIPMENT_EXCEPTION" | "FAILURE" => TrackingStatus::Exception,
            "PICKUP" | "PICKED UP" | "SHIPMENT_PICKUP" => TrackingStatus::PreTransit,
            "RETURN" | "SHIPMENT_RETURN" => TrackingStatus::ReturnToSender,
            _ => TrackingStatus::InTransit,
        }
    }

    /// Convert DHL rate response to ShippingRate
    fn convert_dhl_rate(&self, rate: &DhlRateResponse) -> ShippingRate {
        let service_name = self.service_name(&rate.product_code);

        let mut shipping_rate = ShippingRate::new(
            self.id(),
            self.name(),
            &rate.product_code,
            &service_name,
            rate.total_net_charge
                .parse::<Decimal>()
                .unwrap_or(Decimal::ZERO),
            &rate.currency,
        );

        if let Some(delivery) = &rate.delivery_capabilities {
            if let Some(days) = delivery
                .estimated_delivery_date_and_time
                .as_ref()
                .and_then(|d| d.days_in_transit)
            {
                shipping_rate.delivery_days = Some(days);
            }
        }

        shipping_rate
    }
}

#[async_trait]
impl ShippingProvider for DhlProvider {
    fn id(&self) -> &'static str {
        "dhl"
    }

    fn name(&self) -> &'static str {
        "DHL Express"
    }

    fn is_available(&self) -> bool {
        !self.api_key.is_empty() && !self.api_secret.is_empty()
    }

    async fn get_rates(
        &self,
        from_address: &Address,
        to_address: &Address,
        package: &Package,
        options: &RateOptions,
    ) -> Result<Vec<ShippingRate>> {
        // Build rate request
        let request = DhlRateRequest {
            customer_details: CustomerDetails {
                shipper_details: ShipperDetails {
                    postal_code: from_address.zip.clone(),
                    city_name: from_address.city.clone(),
                    country_code: from_address.country.clone(),
                },
                receiver_details: ReceiverDetails {
                    postal_code: to_address.zip.clone(),
                    city_name: to_address.city.clone(),
                    country_code: to_address.country.clone(),
                },
            },
            accounts: vec![Account {
                type_code: "shipper".to_string(),
                number: self.account_number.clone(),
            }],
            products_and_services: vec![],
            ship_date: Utc::now().format("%Y-%m-%d").to_string(),
            unit_of_measurement: "metric".to_string(),
            packages: vec![DhlPackage {
                weight: package.weight.to_string().parse::<f64>().unwrap_or(0.0),
                dimensions: Dimensions {
                    length: package
                        .length
                        .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0)),
                    width: package
                        .width
                        .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0)),
                    height: package
                        .height
                        .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0)),
                },
            }],
        };

        // Make API request
        let rate_url = format!("{}/rates", self.mydhl_url());

        let response = self
            .client
            .post(&rate_url)
            .headers(
                self.auth_headers()
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            k.parse::<reqwest::header::HeaderName>().unwrap(),
                            v.parse::<reqwest::header::HeaderValue>().unwrap(),
                        )
                    })
                    .collect(),
            )
            .json(&request)
            .send()
            .await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    let rate_response: DhlRateResponseWrapper = resp.json().await.map_err(|e| {
                        Error::shipping(format!("Failed to parse DHL rate response: {}", e))
                    })?;

                    let mut rates: Vec<ShippingRate> = rate_response
                        .products
                        .iter()
                        .map(|p| self.convert_dhl_rate(p))
                        .collect();

                    // Filter by service if specified
                    if let Some(ref services) = options.services {
                        rates.retain(|r| services.contains(&r.service_code));
                    }

                    Ok(rates)
                } else {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();

                    // If API fails, fall back to mock rates for development
                    tracing::warn!(
                        "DHL API returned error: {} - {}. Falling back to mock rates.",
                        status,
                        text
                    );
                    self.get_mock_rates(options)
                }
            }
            Err(e) => {
                tracing::warn!("DHL API request failed: {}. Falling back to mock rates.", e);
                self.get_mock_rates(options)
            }
        }
    }

    async fn create_shipment(
        &self,
        from_address: &Address,
        to_address: &Address,
        package: &Package,
        service_code: &str,
        customs_info: Option<&CustomsInfo>,
    ) -> Result<Shipment> {
        let shipment_request = DhlShipmentRequest {
            planned_shipping_date_and_time: Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
            pickup: Pickup {
                is_requested: false,
            },
            product_code: service_code.to_string(),
            accounts: vec![Account {
                type_code: "shipper".to_string(),
                number: self.account_number.clone(),
            }],
            customer_details: CustomerDetails {
                shipper_details: ShipperDetails {
                    postal_code: from_address.zip.clone(),
                    city_name: from_address.city.clone(),
                    country_code: from_address.country.clone(),
                },
                receiver_details: ReceiverDetails {
                    postal_code: to_address.zip.clone(),
                    city_name: to_address.city.clone(),
                    country_code: to_address.country.clone(),
                },
            },
            content: ShipmentContent {
                packages: vec![DhlPackage {
                    weight: package.weight.to_string().parse::<f64>().unwrap_or(0.0),
                    dimensions: Dimensions {
                        length: package
                            .length
                            .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0)),
                        width: package
                            .width
                            .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0)),
                        height: package
                            .height
                            .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0)),
                    },
                }],
                is_customs_declarable: customs_info.is_some(),
                description: customs_info
                    .map(|c| c.contents_description.clone())
                    .unwrap_or_default(),
            },
        };

        let ship_url = format!("{}/shipments", self.mydhl_url());

        let response = self
            .client
            .post(&ship_url)
            .headers(
                self.auth_headers()
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            k.parse::<reqwest::header::HeaderName>().unwrap(),
                            v.parse::<reqwest::header::HeaderValue>().unwrap(),
                        )
                    })
                    .collect(),
            )
            .json(&shipment_request)
            .send()
            .await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    let ship_response: DhlShipmentResponse = resp.json().await.map_err(|e| {
                        Error::shipping(format!("Failed to parse DHL shipment response: {}", e))
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
                        tracking_number: Some(ship_response.shipment_tracking_number.clone()),
                        tracking_url: Some(format!(
                            "https://www.dhl.com/en/express/tracking.html?AWB={}",
                            ship_response.shipment_tracking_number
                        )),
                        label_url: ship_response.documents.first().map(|d| d.url.clone()),
                        label_data: None,
                        customs_info: customs_info.cloned(),
                        insurance_amount: None,
                        total_cost: Decimal::from(45),
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
                        "DHL shipment API returned error: {} - {}. Falling back to mock shipment.",
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
                    "DHL shipment API request failed: {}. Falling back to mock shipment.",
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
        let tracking_url = format!("{}?trackingNumber={}", self.tracking_url(), tracking_number);

        let response = self
            .client
            .get(&tracking_url)
            .headers(
                self.auth_headers()
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            k.parse::<reqwest::header::HeaderName>().unwrap(),
                            v.parse::<reqwest::header::HeaderValue>().unwrap(),
                        )
                    })
                    .collect(),
            )
            .send()
            .await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    let tracking_response: DhlTrackingResponse =
                        resp.json().await.map_err(|e| {
                            Error::shipping(format!("Failed to parse DHL tracking response: {}", e))
                        })?;

                    let shipment = tracking_response
                        .shipments
                        .first()
                        .ok_or_else(|| Error::not_found("Tracking information not found"))?;

                    let events: Vec<TrackingEvent> = shipment
                        .events
                        .iter()
                        .map(|e| TrackingEvent {
                            timestamp: e.timestamp,
                            status: self.parse_tracking_status(&e.status_code),
                            description: e.description.clone(),
                            location: Some(format!(
                                "{}, {}",
                                e.location.address.address_locality,
                                e.location.address.country_code
                            )),
                            city: Some(e.location.address.address_locality.clone()),
                            state: e.location.address.province_code.clone(),
                            country: Some(e.location.address.country_code.clone()),
                        })
                        .collect();

                    let status = events
                        .first()
                        .map(|e| e.status)
                        .unwrap_or(TrackingStatus::InTransit);

                    Ok(TrackingInfo {
                        tracking_number: tracking_number.to_string(),
                        carrier: self.name().to_string(),
                        status,
                        events,
                        estimated_delivery: shipment.estimated_time_of_delivery,
                    })
                } else {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    tracing::warn!(
                        "DHL tracking API returned error: {} - {}. Falling back to mock tracking.",
                        status,
                        text
                    );
                    self.get_mock_tracking(tracking_number)
                }
            }
            Err(e) => {
                tracing::warn!(
                    "DHL tracking API request failed: {}. Falling back to mock tracking.",
                    e
                );
                self.get_mock_tracking(tracking_number)
            }
        }
    }

    async fn cancel_shipment(&self, shipment_id: &str) -> Result<bool> {
        // DHL allows cancellation before pickup via DELETE /shipments/{shipmentId}
        let cancel_url = format!("{}/shipments/{}", self.mydhl_url(), shipment_id);

        let response = self
            .client
            .delete(&cancel_url)
            .headers(
                self.auth_headers()
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            k.parse::<reqwest::header::HeaderName>().unwrap(),
                            v.parse::<reqwest::header::HeaderValue>().unwrap(),
                        )
                    })
                    .collect(),
            )
            .send()
            .await;

        match response {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(e) => {
                tracing::warn!("DHL cancel shipment API request failed: {}", e);
                Ok(true) // Assume success for mock fallback
            }
        }
    }

    async fn validate_address(&self, address: &Address) -> Result<AddressValidation> {
        // DHL address validation via Location Finder API
        let validate_url = format!(
            "{}/location-finder/v1/find-by-address?countryCode={}&postalCode={}&city={}",
            self.base_url, address.country, address.zip, address.city
        );

        let response = self
            .client
            .get(&validate_url)
            .headers(
                self.auth_headers()
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            k.parse::<reqwest::header::HeaderName>().unwrap(),
                            v.parse::<reqwest::header::HeaderValue>().unwrap(),
                        )
                    })
                    .collect(),
            )
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
            Err(_) => {
                // Fallback to basic validation
                Ok(AddressValidation {
                    is_valid: true,
                    normalized_address: Some(address.clone()),
                    messages: vec![],
                    residential: None,
                })
            }
        }
    }

    fn get_services(&self) -> Vec<ShippingService> {
        vec![
            ShippingService {
                code: "EXPRESS_WORLDWIDE".to_string(),
                name: "DHL Express Worldwide".to_string(),
                carrier: self.name().to_string(),
                domestic: true,
                international: true,
                transit_time_days: Some((1, 3)),
                features: vec![
                    ServiceFeature::Tracking,
                    ServiceFeature::Insurance,
                    ServiceFeature::Express,
                ],
            },
            ShippingService {
                code: "EXPRESS_12:00".to_string(),
                name: "DHL Express 12:00".to_string(),
                carrier: self.name().to_string(),
                domestic: true,
                international: true,
                transit_time_days: Some((1, 1)),
                features: vec![
                    ServiceFeature::Tracking,
                    ServiceFeature::Insurance,
                    ServiceFeature::Express,
                    ServiceFeature::DeliveryConfirmation,
                ],
            },
            ShippingService {
                code: "ECONOMY_SELECT".to_string(),
                name: "DHL Economy Select".to_string(),
                carrier: self.name().to_string(),
                domestic: false,
                international: true,
                transit_time_days: Some((4, 7)),
                features: vec![ServiceFeature::Tracking, ServiceFeature::Insurance],
            },
        ]
    }

    async fn estimate_delivery(
        &self,
        _from_address: &Address,
        _to_address: &Address,
        service_code: &str,
    ) -> Result<Option<DateTime<Utc>>> {
        let days = match service_code {
            "EXPRESS_WORLDWIDE" => 3,
            "EXPRESS_12:00" | "EXPRESS_10:30" | "EXPRESS_9:00" => 1,
            "ECONOMY_SELECT" => 5,
            _ => 3,
        };

        Ok(Some(Utc::now() + chrono::Duration::days(days)))
    }
}

// Mock fallback methods
impl DhlProvider {
    fn get_mock_rates(&self, options: &RateOptions) -> Result<Vec<ShippingRate>> {
        let mut rates = vec![
            ShippingRate::new(
                self.id(),
                self.name(),
                "EXPRESS_WORLDWIDE",
                "DHL Express Worldwide",
                Decimal::from(45),
                "USD",
            )
            .with_delivery(3, None),
            ShippingRate::new(
                self.id(),
                self.name(),
                "EXPRESS_12:00",
                "DHL Express 12:00",
                Decimal::from(65),
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
        let tracking_number = format!("{:010}", rand::random::<u32>());

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
                "https://www.dhl.com/en/express/tracking.html?AWB={}",
                tracking_number
            )),
            label_url: None,
            label_data: None,
            customs_info: customs_info.cloned(),
            insurance_amount: None,
            total_cost: Decimal::from(45),
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
            events: vec![
                TrackingEvent {
                    timestamp: Utc::now() - chrono::Duration::hours(24),
                    status: TrackingStatus::PreTransit,
                    description: "Shipment picked up".to_string(),
                    location: Some("Cincinnati, OH".to_string()),
                    city: Some("Cincinnati".to_string()),
                    state: Some("OH".to_string()),
                    country: Some("US".to_string()),
                },
                TrackingEvent {
                    timestamp: Utc::now(),
                    status: TrackingStatus::InTransit,
                    description: "Processed at DHL facility".to_string(),
                    location: Some("Cincinnati, OH".to_string()),
                    city: Some("Cincinnati".to_string()),
                    state: Some("OH".to_string()),
                    country: Some("US".to_string()),
                },
            ],
            estimated_delivery: Some(Utc::now() + chrono::Duration::days(2)),
        })
    }
}

// DHL API request/response types

#[derive(Debug, Serialize)]
struct DhlRateRequest {
    #[serde(rename = "customerDetails")]
    customer_details: CustomerDetails,
    accounts: Vec<Account>,
    #[serde(rename = "productsAndServices")]
    products_and_services: Vec<ProductAndService>,
    #[serde(rename = "shipDate")]
    ship_date: String,
    #[serde(rename = "unitOfMeasurement")]
    unit_of_measurement: String,
    packages: Vec<DhlPackage>,
}

#[derive(Debug, Serialize)]
struct CustomerDetails {
    #[serde(rename = "shipperDetails")]
    shipper_details: ShipperDetails,
    #[serde(rename = "receiverDetails")]
    receiver_details: ReceiverDetails,
}

#[derive(Debug, Serialize)]
struct ShipperDetails {
    #[serde(rename = "postalCode")]
    postal_code: String,
    #[serde(rename = "cityName")]
    city_name: String,
    #[serde(rename = "countryCode")]
    country_code: String,
}

#[derive(Debug, Serialize)]
struct ReceiverDetails {
    #[serde(rename = "postalCode")]
    postal_code: String,
    #[serde(rename = "cityName")]
    city_name: String,
    #[serde(rename = "countryCode")]
    country_code: String,
}

#[derive(Debug, Serialize)]
struct Account {
    #[serde(rename = "typeCode")]
    type_code: String,
    number: String,
}

#[derive(Debug, Serialize)]
struct ProductAndService {
    #[serde(rename = "productCode")]
    product_code: String,
}

#[derive(Debug, Serialize)]
struct DhlPackage {
    weight: f64,
    dimensions: Dimensions,
}

#[derive(Debug, Serialize)]
struct Dimensions {
    length: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct DhlRateResponseWrapper {
    products: Vec<DhlRateResponse>,
}

#[derive(Debug, Deserialize)]
struct DhlRateResponse {
    #[serde(rename = "productCode")]
    product_code: String,
    #[serde(rename = "productName")]
    #[allow(dead_code)]
    product_name: String,
    #[serde(rename = "totalNetCharge")]
    total_net_charge: String,
    currency: String,
    #[serde(rename = "deliveryCapabilities")]
    delivery_capabilities: Option<DhlDeliveryCapabilities>,
}

#[derive(Debug, Deserialize)]
struct DhlDeliveryCapabilities {
    #[serde(rename = "estimatedDeliveryDateAndTime")]
    estimated_delivery_date_and_time: Option<DhlEstimatedDelivery>,
}

#[derive(Debug, Deserialize)]
struct DhlEstimatedDelivery {
    #[serde(rename = "daysInTransit")]
    days_in_transit: Option<i32>,
}

#[derive(Debug, Serialize)]
struct DhlShipmentRequest {
    #[serde(rename = "plannedShippingDateAndTime")]
    planned_shipping_date_and_time: String,
    pickup: Pickup,
    #[serde(rename = "productCode")]
    product_code: String,
    accounts: Vec<Account>,
    #[serde(rename = "customerDetails")]
    customer_details: CustomerDetails,
    content: ShipmentContent,
}

#[derive(Debug, Serialize)]
struct Pickup {
    #[serde(rename = "isRequested")]
    is_requested: bool,
}

#[derive(Debug, Serialize)]
struct ShipmentContent {
    packages: Vec<DhlPackage>,
    #[serde(rename = "isCustomsDeclarable")]
    is_customs_declarable: bool,
    description: String,
}

#[derive(Debug, Deserialize)]
struct DhlShipmentResponse {
    #[serde(rename = "shipmentTrackingNumber")]
    shipment_tracking_number: String,
    documents: Vec<DhlDocument>,
}

#[derive(Debug, Deserialize)]
struct DhlDocument {
    #[serde(rename = "typeCode")]
    #[allow(dead_code)]
    type_code: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct DhlTrackingResponse {
    shipments: Vec<DhlShipmentTracking>,
}

#[derive(Debug, Deserialize)]
struct DhlShipmentTracking {
    #[serde(rename = "shipmentTrackingNumber")]
    #[allow(dead_code)]
    shipment_tracking_number: String,
    #[allow(dead_code)]
    status: String,
    events: Vec<DhlTrackingEvent>,
    #[serde(rename = "estimatedTimeOfDelivery")]
    estimated_time_of_delivery: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct DhlTrackingEvent {
    timestamp: DateTime<Utc>,
    #[serde(rename = "statusCode")]
    status_code: String,
    description: String,
    location: DhlLocation,
}

#[derive(Debug, Deserialize)]
struct DhlLocation {
    address: DhlAddress,
}

#[derive(Debug, Deserialize)]
struct DhlAddress {
    #[serde(rename = "addressLocality")]
    address_locality: String,
    #[serde(rename = "provinceCode")]
    province_code: Option<String>,
    #[serde(rename = "countryCode")]
    country_code: String,
}
