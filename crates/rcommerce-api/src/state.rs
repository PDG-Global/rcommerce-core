use std::sync::Arc;

use rcommerce_core::cache::RedisPool;
use rcommerce_core::notification::channels::email::{EmailChannel, SmtpConfig};
use rcommerce_core::notification::service::NotificationService;
use rcommerce_core::payment::agnostic::PaymentService;
use rcommerce_core::repository::{
    Database, PostgresApiKeyRepository, PostgresSubscriptionRepository,
};
use rcommerce_core::services::{
    AuthService, CartService, CheckoutService, CouponService, CustomerService, OrderService,
    ProductService, SubscriptionService,
};
use rcommerce_core::shipping::ShippingProviderFactory;
use rcommerce_core::tax::DefaultTaxService;
use rcommerce_core::{BundleService, DigitalProductService, FileUploadService};

use crate::middleware::AuthRateLimiter;

/// Parameters for creating AppState
pub struct AppStateParams {
    pub product_service: ProductService,
    pub customer_service: CustomerService,
    pub auth_service: AuthService,
    pub db: Database,
    pub redis: Option<RedisPool>,
    pub api_key_repository: PostgresApiKeyRepository,
    pub subscription_repository: PostgresSubscriptionRepository,
    pub coupon_service: CouponService,
    pub payment_service: PaymentService,
    pub file_upload_service: FileUploadService,
    pub cart_service: Arc<CartService>,
    pub order_service: Arc<OrderService>,
    pub tax_service: Arc<DefaultTaxService>,
    pub shipping_factory: Arc<ShippingProviderFactory>,
    pub checkout_service: Arc<CheckoutService>,
    pub inventory_service: Arc<rcommerce_core::inventory::InventoryService>,
    pub smtp_config: Option<SmtpConfig>,
}

impl AppStateParams {
    /// Create a new AppStateParams with all required fields
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        product_service: ProductService,
        customer_service: CustomerService,
        auth_service: AuthService,
        db: Database,
        redis: Option<RedisPool>,
        api_key_repository: PostgresApiKeyRepository,
        subscription_repository: PostgresSubscriptionRepository,
        coupon_service: CouponService,
        payment_service: PaymentService,
        file_upload_service: FileUploadService,
        cart_service: Arc<CartService>,
        order_service: Arc<OrderService>,
        tax_service: Arc<DefaultTaxService>,
        shipping_factory: Arc<ShippingProviderFactory>,
        checkout_service: Arc<CheckoutService>,
        inventory_service: Arc<rcommerce_core::inventory::InventoryService>,
        smtp_config: Option<SmtpConfig>,
    ) -> Self {
        Self {
            product_service,
            customer_service,
            auth_service,
            db,
            redis,
            api_key_repository,
            subscription_repository,
            coupon_service,
            payment_service,
            file_upload_service,
            cart_service,
            order_service,
            tax_service,
            shipping_factory,
            checkout_service,
            inventory_service,
            smtp_config,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub product_service: ProductService,
    pub customer_service: CustomerService,
    pub auth_service: AuthService,
    pub subscription_service: SubscriptionService<PostgresSubscriptionRepository>,
    pub subscription_repository: Arc<PostgresSubscriptionRepository>,
    pub coupon_service: CouponService,
    pub payment_service: Arc<PaymentService>,
    pub digital_product_service: Arc<DigitalProductService>,
    pub bundle_service: Arc<BundleService>,
    pub file_upload_service: Arc<FileUploadService>,
    pub cart_service: Arc<CartService>,
    pub order_service: Arc<OrderService>,
    pub tax_service: Arc<DefaultTaxService>,
    pub shipping_factory: Arc<ShippingProviderFactory>,
    pub checkout_service: Arc<CheckoutService>,
    pub inventory_service: Arc<rcommerce_core::inventory::InventoryService>,
    pub db: Database,
    pub redis: Option<RedisPool>,
    pub auth_rate_limiter: AuthRateLimiter,
    pub api_key_repository: Arc<PostgresApiKeyRepository>,
    pub email_channel: Option<Arc<EmailChannel>>,
    pub notification_service: Option<Arc<NotificationService>>,
}

impl AppState {
    pub async fn new(params: AppStateParams) -> Self {
        // Create auth rate limiter: 5 attempts per minute per IP
        let auth_rate_limiter = AuthRateLimiter::new(5, 60);

        // Create subscription service
        let subscription_service = SubscriptionService::new(params.subscription_repository.clone());

        // Create digital product service
        let file_upload_service = Arc::new(params.file_upload_service);
        let digital_product_service = Arc::new(DigitalProductService::new(
            params.db.clone(),
            file_upload_service.clone(),
        ));

        // Create bundle service
        let bundle_service = Arc::new(BundleService::new(params.db.clone()));

        // Create notification service if SMTP is configured
        let (email_channel, notification_service) = if let Some(smtp) = params.smtp_config {
            match EmailChannel::new_smtp(smtp).await {
                Ok(channel) => {
                    let channel = Arc::new(channel);
                    let svc = Arc::new(NotificationService::new(
                        channel.as_ref().clone(),
                        rcommerce_core::notification::channels::SmsChannel,
                        rcommerce_core::notification::channels::WebhookChannel,
                        params.db.pool().clone(),
                    ));
                    tracing::info!("Notification service initialized with SMTP");
                    (Some(channel), Some(svc))
                }
                Err(e) => {
                    tracing::error!("Failed to initialize SMTP notification service: {}", e);
                    (None, None)
                }
            }
        } else {
            tracing::info!("SMTP not configured; notification service disabled");
            (None, None)
        };

        Self {
            product_service: params.product_service,
            customer_service: params.customer_service,
            auth_service: params.auth_service,
            subscription_service,
            subscription_repository: Arc::new(params.subscription_repository),
            coupon_service: params.coupon_service,
            payment_service: Arc::new(params.payment_service),
            digital_product_service,
            bundle_service,
            file_upload_service,
            cart_service: params.cart_service,
            order_service: params.order_service,
            tax_service: params.tax_service,
            shipping_factory: params.shipping_factory,
            checkout_service: params.checkout_service,
            inventory_service: params.inventory_service,
            db: params.db,
            redis: params.redis,
            auth_rate_limiter,
            api_key_repository: Arc::new(params.api_key_repository),
            email_channel,
            notification_service,
        }
    }
}
