//! Third-party shipping aggregator providers

pub mod easypost;
pub mod shipstation;

pub use easypost::EasyPostProvider;
pub use shipstation::ShipStationProvider;

use crate::common::Address;
use crate::shipping::{CustomsInfo, Package, Shipment, ShippingRate};

/// Aggregator response with multiple carrier rates
#[derive(Debug, Clone)]
pub struct AggregatorRatesResponse {
    pub rates: Vec<ShippingRate>,
    pub errors: Vec<AggregatorError>,
}

/// Aggregator error for individual carriers
#[derive(Debug, Clone)]
pub struct AggregatorError {
    pub carrier: String,
    pub message: String,
}

/// Shipment batch request for multiple packages
#[derive(Debug, Clone)]
pub struct BatchShipmentRequest {
    pub shipments: Vec<BatchShipmentItem>,
}

/// Individual shipment in a batch
#[derive(Debug, Clone)]
pub struct BatchShipmentItem {
    pub from_address: Address,
    pub to_address: Address,
    pub package: Package,
    pub service_code: String,
    pub customs_info: Option<CustomsInfo>,
    pub reference: Option<String>,
}

/// Batch shipment response
#[derive(Debug, Clone)]
pub struct BatchShipmentResponse {
    pub batch_id: String,
    pub shipments: Vec<BatchShipmentResult>,
    pub status: BatchStatus,
}

/// Individual batch result
#[derive(Debug, Clone)]
pub struct BatchShipmentResult {
    pub reference: Option<String>,
    pub shipment: Option<Shipment>,
    pub error: Option<String>,
}

/// Batch processing status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

/// Insurance options
#[derive(Debug, Clone)]
pub struct InsuranceOptions {
    pub amount: rust_decimal::Decimal,
    pub currency: String,
    pub provider: InsuranceProvider,
}

/// Insurance provider
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsuranceProvider {
    Carrier,
    ThirdParty,
}

/// Address verification result
#[derive(Debug, Clone)]
pub struct AddressVerificationResult {
    pub address: Address,
    pub verified: bool,
    pub residential: bool,
    pub deliverable: bool,
    pub suggestions: Vec<Address>,
}
