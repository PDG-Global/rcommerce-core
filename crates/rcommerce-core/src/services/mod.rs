pub mod api_key_scopes;
pub mod auth_service;
pub mod bundle_service;
pub mod cart_service;
pub mod checkout_service;
pub mod coupon_service;
pub mod customer_service;
pub mod digital_product_service;
pub mod dunning_service;
pub mod newsletter_service;
pub mod order_service;
pub mod product_service;
pub mod review_service;
pub mod statistics_service;
pub mod subscription_service;

pub use api_key_scopes::{presets as scope_presets, Action, Resource, Scope, ScopeChecker};
pub use auth_service::ApiKey;
pub use auth_service::AuthService;
pub use auth_service::AuthenticatedUser;
pub use auth_service::JwtClaims;
pub use auth_service::TokenType;
pub use bundle_service::BundleService;
pub use cart_service::CartService;
pub use checkout_service::{
    CheckoutConfig, CheckoutResult, CheckoutService, CheckoutSummary, CompleteCheckoutRequest,
    ConfirmCheckoutRequest, InitiateCheckoutRequest, SelectShippingRequest,
    SubscriptionCheckoutOption, TaxBreakdownItem as CheckoutTaxBreakdownItem,
};
pub use coupon_service::CouponService;
pub use customer_service::CustomerService;
pub use digital_product_service::DigitalProductService;
pub use dunning_service::{
    DunningHistory, DunningService, RetryProcessingResult, RetryableInvoice,
};
pub use newsletter_service::NewsletterService;
pub use order_service::OrderService;
pub use product_service::ProductService;
pub use review_service::ReviewService;
pub use statistics_service::{FullDashboardData, SalesReport, StatisticsService};
pub use subscription_service::{
    next_billing_at, BillingReport, CreateSubscriptionFromOrder, SubscriptionService,
};

use crate::Result;

/// Common service trait for dependency injection
#[async_trait::async_trait]
pub trait Service: Send + Sync {
    async fn health_check(&self) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct PaginationParams {
    pub page: i64,
    pub per_page: i64,
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 20,
        }
    }
}

impl PaginationParams {
    pub fn offset(&self) -> i64 {
        (self.page - 1) * self.per_page
    }

    pub fn limit(&self) -> i64 {
        self.per_page
    }
}

#[derive(Debug, Clone)]
pub struct PaginationInfo {
    pub page: i64,
    pub per_page: i64,
    pub total: i64,
    pub total_pages: i64,
}
