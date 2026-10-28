pub mod accounting;
pub mod cache;
pub mod common;
pub mod config;
pub mod db;
pub mod error;
pub mod import;
pub mod inventory;
pub mod invoice;
pub mod jobs;
pub mod media;
pub mod middleware;
pub mod models;
pub mod notification;
pub mod order;
pub mod payment;
pub mod performance;
pub mod repository;
pub mod search;
pub mod services;
pub mod shipping;
pub mod tax;
pub mod traits;
pub mod websocket;

// Re-export commonly used types
pub use config::{
    BrandingConfig, Config, DunningConfig, DunningEmailTemplates, GatewayDunningConfig,
    OrdersConfig,
};
pub use db::migrate::{auto_migrate, DbStatus, Migrator};
pub use error::{Error, Result};
pub use inventory::{
    InventoryConfig, InventoryLevel, InventoryLocation, InventoryService, LocationInventory,
    LowStockAlert, ProductInventory, ReservationStatus, StockAlertLevel, StockMovement,
    StockReservation, StockStatus,
};
pub use models::{
    Currency, OrderType, Pagination, ProductType, SortDirection, SortParams, SubscriptionInterval,
    SubscriptionStatus,
};
pub use payment::gateways::{
    airwallex_agnostic::AirwallexAgnosticGateway, alipay_agnostic::AliPayAgnosticGateway,
    stripe_agnostic::StripeAgnosticGateway, wechatpay_agnostic::WeChatPayAgnosticGateway,
};
pub use payment::gateways::{
    alipay::AliPayGateway, stripe::StripeGateway, wechatpay::WeChatPayGateway,
};
pub use payment::{
    CardDetails, CreatePaymentRequest, Payment, PaymentGateway, PaymentMethod, PaymentSession,
    PaymentSessionStatus, PaymentStatus, Refund, RefundStatus, WebhookEvent, WebhookEventType,
};
pub use repository::{create_pool, Database};
pub use services::dunning_service::{self, EmailService as DunningEmailService};
pub use services::{
    scope_presets, Action, ApiKey, AuthService, CustomerService, DunningHistory, DunningService,
    JwtClaims, OrderService, PaginationInfo, PaginationParams, ProductService, Resource,
    RetryProcessingResult, RetryableInvoice, Scope, ScopeChecker, Service,
};
pub use services::{BundleService, DigitalProductService};
pub use traits::Repository;
// Order types come from the order module (not models), which includes lifecycle, fulfillment, etc.
pub use order::{
    CreateOrderItem, CreateOrderRequest, Fulfillment, FulfillmentStatus, Order, OrderCalculator,
    OrderFilter, OrderItem, OrderService as OrderManager, OrderStatus, OrderTotals,
    PaymentStatus as OrderPaymentStatus, TrackingInfo as OrderTrackingInfo,
};

// Shipping module exports
pub use shipping::{
    AddressValidation, ContentsType, CustomsInfo, CustomsItem, NonDeliveryOption, Package,
    RateOptions, ServiceFeature, Shipment, ShipmentStatus, ShippingProvider,
    ShippingProviderFactory, ShippingRate, ShippingRateAggregator, ShippingService, TrackingEvent,
    TrackingInfo, TrackingStatus,
};

// Shipping calculation exports
pub use shipping::calculation::{
    ChargeableWeight, LengthConverter, LengthUnit, ShippingCalculation, ShippingCalculator,
    TieredShippingCalculator, VolumetricWeightCalculator, WeightConverter, WeightUnit,
};

// Shipping packaging exports
pub use shipping::packaging::{
    ItemDimensions, PackageRecommendation, PackageRegistry, PackageType, PackagingCalculator,
};

// Shipping zones exports
pub use shipping::zones::{ShippingZone, ZoneCalculator, ZonePresets, ZoneRate};

// Shipping rules exports
pub use shipping::rules::{
    RuleAction, RuleCondition, RulePresets, ShippingRule, ShippingRuleEngine,
};

// Shipping carriers
pub use shipping::carriers::{DhlProvider, FedExProvider, UpsProvider, UspsProvider};

// Shipping providers (aggregators)
pub use shipping::providers::{EasyPostProvider, ShipStationProvider};

// Media exports
pub use media::{FileMetadata, FileUploadService, StorageBackend};

// Tax module exports
pub use tax::{
    CalculateTaxRequest, CreateTaxCategoryRequest, CreateTaxExemptionRequest, CreateTaxRateRequest,
    CreateTaxZoneRequest, CustomerTaxInfo, DefaultTaxService, GenerateOssReportRequest,
    LineItemTax, OssReport, OssScheme, TaxAddress, TaxBreakdown, TaxCalculation,
    TaxCalculationResponse, TaxCalculator, TaxCategory, TaxCategoryInfo, TaxContext, TaxExemption,
    TaxRate, TaxRateQuery, TaxRateWithDetails, TaxService, TaxTransaction, TaxZone, TaxZoneInfo,
    TaxZoneQuery, TaxableItem, TransactionType, ValidateVatIdRequest, VatId, VatValidationResponse,
    VatValidationResult, ViesValidator,
};

/// Current version of rcommerce
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

impl Error {
    pub fn not_implemented<T: Into<String>>(msg: T) -> Self {
        Error::Other(format!("Not implemented: {}", msg.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_available() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_error_creation() {
        let err = Error::validation("Test validation error");
        assert_eq!(err.status_code(), 400);
        assert_eq!(err.category(), "validation");
    }
}
